//! Process-level cache of built project state for the C ABI.
//!
//! Every FFI call used to scan the tree, parse (or load the skeleton cache),
//! create a fresh `ApiState` and rebuild the graph, betweenness centrality
//! included. CKB issues several calls per request — `prepareChange` took 54 s
//! with Cartographer against 2 s without.
//!
//! The cache keeps one built `ApiState` (mapped files + graph) per canonical
//! root, at most [`CAPACITY`] roots, least recently used evicted. Each call
//! re-validates with a stat walk through the same scanner the build uses
//! (file set, size and mtime of every file); the walk costs a fraction of a
//! parse. On a match the call reuses the state as is. On a mismatch only the
//! changed files are re-parsed, a new `ApiState` is built from the patched map
//! (callers still holding the old one keep a consistent snapshot) and it
//! inherits the old state's topology-keyed centrality cache, so an edit that
//! does not change the import graph skips betweenness too.
//!
//! The on-disk skeleton cache (`.codecartographer_cache.json`) now carries the
//! same per-file stats, so a cold process reuses exactly the entries whose file
//! is unchanged. It used to be keyed by git HEAD alone and served stale
//! skeletons for every uncommitted edit.
//!
//! Concurrency: the slot list is behind one mutex held only for lookup; each
//! root has its own mutex held for validation and (re)build, so concurrent
//! calls on one root build once and calls on different roots run in parallel.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use rayon::prelude::*;

use crate::api::ApiState;
use crate::mapper::{extract_skeleton, MappedFile};
use crate::scanner::{is_ignored_path, scan_files_with_noise_tracking};

/// Roots kept in memory. CKB works on one repository per process; a few slots
/// cover federation-style use without holding many graphs.
pub(crate) const CAPACITY: usize = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct FileStat {
    pub mtime_ns: u64,
    pub size: u64,
}

pub(crate) type FileStats = HashMap<String, FileStat>;

struct ScannedFile {
    abs: PathBuf,
    rel: String,
    stat: FileStat,
}

/// What a lookup had to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Outcome {
    /// Tree unchanged since the last build: state reused.
    Hit,
    /// State (re)built. `from_disk`: no in-memory state, started from the
    /// on-disk skeleton cache (or nothing).
    Rebuilt { reparsed: usize, removed: usize, from_disk: bool },
}

struct Built {
    stats: FileStats,
    state: Arc<ApiState>,
}

struct Slot {
    built: Mutex<Option<Built>>,
    last_used: AtomicU64,
}

pub(crate) struct StateCache {
    cap: usize,
    slots: Mutex<Vec<(PathBuf, Arc<Slot>)>>,
    tick: AtomicU64,
}

/// The cache all FFI functions share.
pub(crate) fn global() -> &'static StateCache {
    static CACHE: StateCache = StateCache::new(CAPACITY);
    &CACHE
}

impl StateCache {
    pub(crate) const fn new(cap: usize) -> Self {
        Self {
            cap,
            slots: Mutex::new(Vec::new()),
            tick: AtomicU64::new(0),
        }
    }

    fn slot(&self, key: &Path) -> Arc<Slot> {
        let now = self.tick.fetch_add(1, Ordering::Relaxed);
        let mut slots = self.slots.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((_, s)) = slots.iter().find(|(k, _)| k == key) {
            s.last_used.store(now, Ordering::Relaxed);
            return Arc::clone(s);
        }
        if slots.len() >= self.cap.max(1) {
            // Evict the least recently used root. A caller still holding its
            // Arc<ApiState> keeps working; the memory goes when it finishes.
            if let Some(idx) = slots
                .iter()
                .enumerate()
                .min_by_key(|(_, (_, s))| s.last_used.load(Ordering::Relaxed))
                .map(|(i, _)| i)
            {
                slots.swap_remove(idx);
            }
        }
        let s = Arc::new(Slot {
            built: Mutex::new(None),
            last_used: AtomicU64::new(now),
        });
        slots.push((key.to_path_buf(), Arc::clone(&s)));
        s
    }

    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.slots.lock().unwrap().len()
    }

    /// Built state for `root` (graph included), reusing what is still valid.
    pub(crate) fn get(&self, root: &Path) -> Result<(Arc<ApiState>, Outcome), String> {
        let key = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
        let slot = self.slot(&key);
        let mut built = slot.built.lock().unwrap_or_else(|e| e.into_inner());

        let files = stat_tree(&key)?;
        let current: FileStats = files.iter().map(|f| (f.rel.clone(), f.stat)).collect();
        if let Some(b) = built.as_ref() {
            if b.stats == current {
                return Ok((Arc::clone(&b.state), Outcome::Hit));
            }
        }

        let previous = built.take();
        let from_disk = previous.is_none();
        let head = if from_disk { crate::git_head(&key) } else { String::new() };
        let (prev_files, prev_stats) = match &previous {
            Some(b) => (
                b.state.mapped_files.lock().map_err(|e| e.to_string())?.clone(),
                b.stats.clone(),
            ),
            None => load_disk_cache(&key, &head).unwrap_or_default(),
        };

        let (mapped, reparsed, removed) = patch(prev_files, &prev_stats, &files);

        let state = Arc::new(ApiState::new(key.clone()));
        *state.mapped_files.lock().map_err(|e| e.to_string())? = mapped;
        if let Some(b) = &previous {
            state.inherit_bc_cache(&b.state);
        }
        // Build the graph before publishing, so concurrent callers never race
        // to build it twice.
        state.rebuild_graph()?;

        if from_disk && (reparsed > 0 || removed > 0) {
            if let Ok(files) = state.mapped_files.lock() {
                save_disk_cache(&key, &head, &files, &current);
            }
        }

        *built = Some(Built {
            stats: current,
            state: Arc::clone(&state),
        });
        Ok((state, Outcome::Rebuilt { reparsed, removed, from_disk }))
    }
}

/// The file set the build parses, with size and mtime. Same scanner, same
/// filters as a full build, so the cache key covers exactly what was parsed.
fn stat_tree(root: &Path) -> Result<Vec<ScannedFile>, String> {
    let scan = scan_files_with_noise_tracking(root).map_err(|e| e.to_string())?;
    Ok(scan
        .files
        .into_iter()
        .filter(|p| !is_ignored_path(p))
        .filter_map(|abs| {
            let meta = std::fs::metadata(&abs).ok()?;
            let mtime_ns = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_nanos() as u64)
                .unwrap_or(0);
            let rel = abs
                .strip_prefix(root)
                .unwrap_or(&abs)
                .to_string_lossy()
                .replace('\\', "/");
            Some(ScannedFile {
                abs,
                rel,
                stat: FileStat { mtime_ns, size: meta.len() },
            })
        })
        .collect())
}

/// Keep the skeletons whose file stat is unchanged, re-parse the rest (in
/// parallel, on the large-stack parse pool), drop deleted files.
fn patch(
    mut prev: HashMap<String, MappedFile>,
    prev_stats: &FileStats,
    files: &[ScannedFile],
) -> (HashMap<String, MappedFile>, usize, usize) {
    let mut out: HashMap<String, MappedFile> = HashMap::with_capacity(files.len());
    let mut dirty: Vec<&ScannedFile> = Vec::new();
    for f in files {
        match (prev_stats.get(&f.rel), prev.remove(&f.rel)) {
            (Some(s), Some(m)) if *s == f.stat => {
                out.insert(f.rel.clone(), m);
            }
            _ => dirty.push(f),
        }
    }
    let removed = prev.len();
    let parsed: Vec<(String, MappedFile)> = crate::parse_pool().install(|| {
        dirty
            .par_iter()
            .filter_map(|f| {
                let content = std::fs::read_to_string(&f.abs).ok()?;
                Some((f.rel.clone(), extract_skeleton(&f.abs, &content)))
            })
            .collect()
    });
    let reparsed = dirty.len();
    out.extend(parsed);
    (out, reparsed, removed)
}

// ---------------------------------------------------------------------------
// Git history results
// ---------------------------------------------------------------------------

/// `git log -n <limit>` from HEAD depends on nothing but HEAD, so its derived
/// results are cached exactly by (root, HEAD, limit). One `git log` over 500
/// commits costs ~350 ms on CKB; churn, co-change, hidden coupling and
/// shotgun surgery each ran it again on every call.
struct HeadCache<T> {
    entries: Mutex<Vec<((PathBuf, String, usize), Arc<T>)>>,
}

const HEAD_CACHE_CAP: usize = 8;

impl<T> HeadCache<T> {
    const fn new() -> Self {
        Self { entries: Mutex::new(Vec::new()) }
    }

    fn get_or(&self, root: &Path, limit: usize, compute: impl FnOnce(&Path) -> T) -> Arc<T> {
        let head = crate::git_head(root);
        if head.is_empty() {
            return Arc::new(compute(root)); // not a git repo: nothing to key on
        }
        let key = (root.canonicalize().unwrap_or_else(|_| root.to_path_buf()), head, limit);
        {
            let mut entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(i) = entries.iter().position(|(k, _)| *k == key) {
                let hit = entries.remove(i);
                let value = Arc::clone(&hit.1);
                entries.push(hit); // most recently used last
                return value;
            }
        }
        // Computed outside the lock: concurrent misses may both run git, but
        // git calls on other roots are never serialized behind this one.
        let value = Arc::new(compute(root));
        let mut entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());
        if !entries.iter().any(|(k, _)| *k == key) {
            if entries.len() >= HEAD_CACHE_CAP {
                entries.remove(0);
            }
            entries.push((key, Arc::clone(&value)));
        }
        value
    }
}

pub(crate) fn git_cochange(root: &Path, limit: usize) -> Arc<Vec<crate::git_analysis::CoChangePair>> {
    static CACHE: HeadCache<Vec<crate::git_analysis::CoChangePair>> = HeadCache::new();
    CACHE.get_or(root, limit, |r| crate::git_analysis::git_cochange(r, limit))
}

pub(crate) fn git_churn(root: &Path, limit: usize) -> Arc<HashMap<String, usize>> {
    static CACHE: HeadCache<HashMap<String, usize>> = HeadCache::new();
    CACHE.get_or(root, limit, |r| crate::git_analysis::git_churn(r, limit))
}

// ---------------------------------------------------------------------------
// On-disk skeleton cache
// ---------------------------------------------------------------------------

/// `<root>/.codecartographer_cache.json`. `head` is still written because the
/// CLI's own cache reader keys on it; this side validates per file.
#[derive(serde::Deserialize)]
struct DiskCache {
    files: HashMap<String, MappedFile>,
    /// Absent in caches written before 4.0.4 (or by the CLI): then nothing in
    /// `files` can be validated and everything is re-parsed once.
    #[serde(default)]
    stats: FileStats,
}

/// Borrowed mirror of `DiskCache` for writing; owning it would deep-copy the
/// whole skeleton map just to hand it to serde.
#[derive(serde::Serialize)]
struct DiskCacheRef<'a> {
    head: &'a str,
    files: &'a HashMap<String, MappedFile>,
    stats: &'a FileStats,
}

fn cache_path(root: &Path) -> PathBuf {
    root.join(".codecartographer_cache.json")
}

fn load_disk_cache(root: &Path, head: &str) -> Option<(HashMap<String, MappedFile>, FileStats)> {
    if head.is_empty() {
        return None; // not a git repo — no cache file, as before
    }
    let data = std::fs::read(cache_path(root)).ok()?;
    let cache: DiskCache = serde_json::from_slice(&data).ok()?;
    Some((cache.files, cache.stats))
}

fn save_disk_cache(
    root: &Path,
    head: &str,
    files: &HashMap<String, MappedFile>,
    stats: &FileStats,
) {
    if head.is_empty() {
        return;
    }
    // Streamed from borrowed data. A partial write fails to deserialize, so
    // the next load returns None and rebuilds.
    let Ok(file) = std::fs::File::create(cache_path(root)) else {
        return;
    };
    let mut writer = std::io::BufWriter::new(file);
    if serde_json::to_writer(&mut writer, &DiskCacheRef { head, files, stats }).is_ok() {
        use std::io::Write;
        let _ = writer.flush();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ffi_tests::{fixture, temp_dir};

    fn touch(path: &Path, content: &str) {
        // Bump the mtime past the filesystem's granularity even if the size
        // stays the same.
        std::thread::sleep(std::time::Duration::from_millis(20));
        std::fs::write(path, content).unwrap();
    }

    #[test]
    fn second_call_on_unchanged_tree_is_a_hit() {
        let fx = fixture();
        let cache = StateCache::new(2);
        let (s1, o1) = cache.get(&fx.root).unwrap();
        assert!(matches!(o1, Outcome::Rebuilt { from_disk: true, .. }), "{o1:?}");
        let (s2, o2) = cache.get(&fx.root).unwrap();
        assert_eq!(o2, Outcome::Hit);
        assert!(Arc::ptr_eq(&s1, &s2));
        assert!(s2.project_graph.lock().unwrap().is_some(), "graph is prebuilt");
    }

    #[test]
    fn touching_a_file_reparses_only_that_file() {
        let fx = fixture();
        let cache = StateCache::new(2);
        let (s1, _) = cache.get(&fx.root).unwrap();
        let view = fx.root.join("src/ui/view.ts");
        touch(&view, "import { load } from '../db/my_model';\nexport function brandNew(): number { return 9; }\n");
        let (s2, o2) = cache.get(&fx.root).unwrap();
        assert_eq!(o2, Outcome::Rebuilt { reparsed: 1, removed: 0, from_disk: false });
        assert!(!Arc::ptr_eq(&s1, &s2));
        let sigs: Vec<String> = s2.mapped_files.lock().unwrap()["src/ui/view.ts"]
            .signatures
            .iter()
            .map(|s| s.raw.clone())
            .collect();
        assert!(sigs.iter().any(|s| s.contains("brandNew")), "stale skeleton: {sigs:?}");
        // The old snapshot is untouched for callers still holding it.
        assert!(s1.project_graph.lock().unwrap().is_some());

        // Deleting and adding files changes the file set.
        std::fs::remove_file(fx.root.join("src/util.ts")).unwrap();
        touch(&fx.root.join("src/extra.ts"), "export const e = 1;\n");
        let (s3, o3) = cache.get(&fx.root).unwrap();
        assert_eq!(o3, Outcome::Rebuilt { reparsed: 1, removed: 1, from_disk: false });
        let files = s3.mapped_files.lock().unwrap();
        assert!(files.contains_key("src/extra.ts") && !files.contains_key("src/util.ts"));
    }

    /// The disk cache used to be keyed by HEAD alone: an uncommitted edit was
    /// served from the stale cache in every new process.
    #[test]
    fn disk_cache_is_validated_per_file() {
        let fx = fixture();
        let (_, o) = StateCache::new(1).get(&fx.root).unwrap();
        assert!(matches!(o, Outcome::Rebuilt { from_disk: true, .. }));
        assert!(cache_path(&fx.root).exists());

        // A new process (fresh cache) on an unchanged tree parses nothing.
        let (_, o) = StateCache::new(1).get(&fx.root).unwrap();
        assert_eq!(o, Outcome::Rebuilt { reparsed: 0, removed: 0, from_disk: true });

        // An uncommitted edit (HEAD unchanged) is re-parsed, the rest reused.
        touch(&fx.root.join("src/config.ts"), "export function renamedPort(): number { return 2; }\n");
        let (s, o) = StateCache::new(1).get(&fx.root).unwrap();
        assert_eq!(o, Outcome::Rebuilt { reparsed: 1, removed: 0, from_disk: true });
        let raw: Vec<String> = s.mapped_files.lock().unwrap()["src/config.ts"]
            .signatures
            .iter()
            .map(|s| s.raw.clone())
            .collect();
        assert!(raw.iter().any(|s| s.contains("renamedPort")), "stale: {raw:?}");
    }

    #[test]
    fn lru_evicts_beyond_capacity() {
        let cache = StateCache::new(2);
        let dirs: Vec<PathBuf> = (0..3).map(|i| temp_dir(&format!("lru{i}"))).collect();
        for d in &dirs {
            std::fs::write(d.join("a.rs"), "pub fn a() {}\n").unwrap();
        }
        cache.get(&dirs[0]).unwrap();
        cache.get(&dirs[1]).unwrap();
        cache.get(&dirs[0]).unwrap(); // dirs[1] is now least recently used
        cache.get(&dirs[2]).unwrap();
        assert_eq!(cache.len(), 2);
        assert_eq!(cache.get(&dirs[0]).unwrap().1, Outcome::Hit);
        assert!(matches!(cache.get(&dirs[1]).unwrap().1, Outcome::Rebuilt { .. }));
        for d in &dirs {
            let _ = std::fs::remove_dir_all(d);
        }
    }

    /// History results are reused until HEAD moves.
    #[test]
    fn git_results_cached_by_head() {
        let fx = fixture();
        let a = git_cochange(&fx.root, 50);
        let b = git_cochange(&fx.root, 50);
        assert!(Arc::ptr_eq(&a, &b));
        assert!(!Arc::ptr_eq(&a, &git_cochange(&fx.root, 49)), "limit is part of the key");
        let c1 = git_churn(&fx.root, 50);
        std::fs::write(fx.root.join("src/util.ts"), "export function helper(): number { return 7; }\n").unwrap();
        let st = std::process::Command::new("git")
            .arg("-C").arg(&fx.root)
            .args(["commit", "-qam", "more"])
            .status()
            .unwrap();
        assert!(st.success());
        let c2 = git_churn(&fx.root, 50);
        assert!(!Arc::ptr_eq(&c1, &c2), "new HEAD must recompute");
        assert_eq!(c2["src/util.ts"], c1["src/util.ts"] + 1);
    }

    /// CKB calls from several goroutines at once; one root builds once.
    #[test]
    fn concurrent_calls_build_once() {
        let fx = fixture();
        let cache = Arc::new(StateCache::new(2));
        let outcomes: Vec<Outcome> = std::thread::scope(|s| {
            let hs: Vec<_> = (0..4)
                .map(|_| {
                    let c = Arc::clone(&cache);
                    let r = fx.root.clone();
                    s.spawn(move || c.get(&r).unwrap().1)
                })
                .collect();
            hs.into_iter().map(|h| h.join().unwrap()).collect()
        });
        assert_eq!(outcomes.iter().filter(|o| **o == Outcome::Hit).count(), 3, "{outcomes:?}");
    }
}
