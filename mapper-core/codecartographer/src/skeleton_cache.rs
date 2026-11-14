//! The on-disk skeleton cache, `<root>/.codecartographer_cache.json`, shared by
//! the CLI (`build_mapped_files_cached` in main.rs) and the C ABI
//! ([`crate::ffi_cache`]).
//!
//! Every entry is validated against its file's size and mtime, so a new process
//! reuses exactly the skeletons whose file is unchanged and re-parses the rest.
//! The whole file is also stamped with the crate version and
//! [`EXTRACTOR_VERSION`]: a skeleton extracted by another build (an upgrade,
//! or a develop build with a changed extractor) is discarded as a whole, since
//! an unchanged file would otherwise keep its stale imports and signatures
//! forever.
//!
//! Caches written before the stamp existed (keyed by git HEAD alone) fail
//! validation and are rebuilt once. The cache is only written inside a git
//! repository, as it always was.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use rayon::prelude::*;

use crate::mapper::{extract_skeleton, MappedFile};
use crate::scanner::{is_ignored_path, scan_files_with_noise_tracking};

/// Version of what `extract_skeleton` produces. **Bump it with every change to
/// extraction output** (imports, signatures, docstrings, test detection, ...):
/// it is what makes existing caches re-parse files that did not change. The
/// crate version is part of the stamp too, but develop builds share it with
/// the last release.
///
/// History: 1 = 4.0.3 and earlier (implicit); 2 = graph-resolution fixes
/// (Python function-level and relative imports, Rust `pub use`, grouped and
/// inline `crate::` paths, Go package imports, TS type-only imports).
pub(crate) const EXTRACTOR_VERSION: u32 = 2;

pub(crate) const CACHE_FILE: &str = ".codecartographer_cache.json";

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct FileStat {
    pub mtime_ns: u64,
    pub size: u64,
}

pub(crate) type FileStats = HashMap<String, FileStat>;

pub(crate) struct ScannedFile {
    pub abs: PathBuf,
    pub rel: String,
    pub stat: FileStat,
}

/// The file set a build parses, with size and mtime. Same scanner, same
/// filters as a full build, so the cache key covers exactly what was parsed.
pub(crate) fn stat_tree(root: &Path) -> Result<Vec<ScannedFile>, String> {
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

/// Keep the skeletons whose file stat is unchanged, re-parse the rest in
/// parallel on the current rayon pool (callers pick one with a large stack),
/// drop deleted files. Returns (map, reparsed, removed).
pub(crate) fn patch(
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
    let parsed: Vec<(String, MappedFile)> = dirty
        .par_iter()
        .filter_map(|f| {
            let content = std::fs::read_to_string(&f.abs).ok()?;
            Some((f.rel.clone(), extract_skeleton(&f.abs, &content)))
        })
        .collect();
    let reparsed = dirty.len();
    out.extend(parsed);
    (out, reparsed, removed)
}

#[derive(serde::Deserialize)]
struct DiskCache {
    #[serde(default)]
    version: String,
    #[serde(default)]
    extractor: u32,
    files: HashMap<String, MappedFile>,
    #[serde(default)]
    stats: FileStats,
}

/// Borrowed mirror of `DiskCache` for writing; owning it would deep-copy the
/// whole skeleton map just to hand it to serde.
#[derive(serde::Serialize)]
struct DiskCacheRef<'a> {
    version: &'a str,
    extractor: u32,
    files: &'a HashMap<String, MappedFile>,
    stats: &'a FileStats,
}

pub(crate) fn cache_path(root: &Path) -> PathBuf {
    root.join(CACHE_FILE)
}

/// Skeletons and their file stats from the disk cache, or `None` when there is
/// no usable cache: missing, unreadable, or written by another crate or
/// extractor version.
pub(crate) fn load(root: &Path) -> Option<(HashMap<String, MappedFile>, FileStats)> {
    let data = std::fs::read(cache_path(root)).ok()?;
    let cache: DiskCache = serde_json::from_slice(&data).ok()?;
    if cache.version != env!("CARGO_PKG_VERSION") || cache.extractor != EXTRACTOR_VERSION {
        return None;
    }
    Some((cache.files, cache.stats))
}

/// Write the cache, streamed from borrowed data. A partial write fails to
/// deserialize, so the next load returns `None` and rebuilds.
pub(crate) fn save(root: &Path, files: &HashMap<String, MappedFile>, stats: &FileStats) {
    let Ok(file) = std::fs::File::create(cache_path(root)) else {
        return;
    };
    let mut writer = std::io::BufWriter::new(file);
    let cache = DiskCacheRef {
        version: env!("CARGO_PKG_VERSION"),
        extractor: EXTRACTOR_VERSION,
        files,
        stats,
    };
    if serde_json::to_writer(&mut writer, &cache).is_ok() {
        use std::io::Write;
        let _ = writer.flush();
    }
}

/// What [`build`] had to do.
#[allow(dead_code)] // build() is the CLI's entry; the C ABI patches in memory
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct BuildOutcome {
    pub reparsed: usize,
    pub removed: usize,
    /// A valid cache existed (same versions); its unchanged entries were reused.
    pub cache_hit: bool,
}

/// Skeletons for every project file, starting from the disk cache when
/// `use_disk` (the callers pass "is a git repository") and writing it back
/// when anything had to be parsed or dropped.
#[allow(dead_code)]
pub(crate) fn build(
    root: &Path,
    use_disk: bool,
) -> Result<(HashMap<String, MappedFile>, FileStats, BuildOutcome), String> {
    let files = stat_tree(root)?;
    let current: FileStats = files.iter().map(|f| (f.rel.clone(), f.stat)).collect();
    let cached = if use_disk { load(root) } else { None };
    let cache_hit = cached.is_some();
    let (prev_files, prev_stats) = cached.unwrap_or_default();
    let (mapped, reparsed, removed) = patch(prev_files, &prev_stats, &files);
    if use_disk && (!cache_hit || reparsed > 0 || removed > 0) {
        save(root, &mapped, &current);
    }
    Ok((mapped, current, BuildOutcome { reparsed, removed, cache_hit }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(tag: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("carto-skc-{tag}-{}-{nanos}", std::process::id()));
        std::fs::create_dir_all(dir.join("src")).unwrap();
        let dir = dir.canonicalize().unwrap();
        std::fs::write(dir.join("src/a.rs"), "pub fn alpha() {}\n").unwrap();
        std::fs::write(dir.join("src/b.rs"), "use crate::a::alpha;\npub fn beta() { alpha() }\n").unwrap();
        dir
    }

    fn sigs(files: &HashMap<String, MappedFile>, rel: &str) -> Vec<String> {
        files[rel].signatures.iter().map(|s| s.raw.clone()).collect()
    }

    #[test]
    fn unchanged_tree_parses_nothing_and_edits_reparse_one_file() {
        let root = temp_root("edit");
        let (_, _, o) = build(&root, true).unwrap();
        assert_eq!(o, BuildOutcome { reparsed: 2, removed: 0, cache_hit: false });
        let (_, _, o) = build(&root, true).unwrap();
        assert_eq!(o, BuildOutcome { reparsed: 0, removed: 0, cache_hit: true });

        std::thread::sleep(std::time::Duration::from_millis(20));
        std::fs::write(root.join("src/a.rs"), "pub fn gamma() {}\n").unwrap();
        let (files, _, o) = build(&root, true).unwrap();
        assert_eq!(o, BuildOutcome { reparsed: 1, removed: 0, cache_hit: true });
        assert!(sigs(&files, "src/a.rs").iter().any(|s| s.contains("gamma")), "stale skeleton");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A cache from another extractor version (or from 4.0.3, which had no
    /// stamp) must not serve its skeletons for unchanged files.
    #[test]
    fn version_mismatch_discards_the_cache() {
        let root = temp_root("ver");
        let (files, stats, _) = build(&root, true).unwrap();

        // Plant a poisoned entry under the current stats with an old stamp.
        let mut poisoned = files.clone();
        poisoned.get_mut("src/a.rs").unwrap().imports = vec!["stale::import".into()];
        let write = |version: &str, extractor: u32| {
            let raw = serde_json::to_vec(&DiskCacheRef { version, extractor, files: &poisoned, stats: &stats }).unwrap();
            std::fs::write(cache_path(&root), raw).unwrap();
        };

        write(env!("CARGO_PKG_VERSION"), EXTRACTOR_VERSION - 1);
        let (got, _, o) = build(&root, true).unwrap();
        assert!(!o.cache_hit);
        assert_eq!(o.reparsed, 2);
        assert!(got["src/a.rs"].imports.is_empty());

        write("0.0.1", EXTRACTOR_VERSION);
        assert!(load(&root).is_none(), "crate version mismatch must invalidate");

        // The 4.0.3 layout: head + files, no stamp, no stats.
        let legacy = serde_json::json!({ "head": "abc", "files": poisoned });
        std::fs::write(cache_path(&root), serde_json::to_vec(&legacy).unwrap()).unwrap();
        assert!(load(&root).is_none(), "unstamped cache must invalidate");

        // Rebuilt caches carry the current stamp and are reused.
        let (_, _, o) = build(&root, true).unwrap();
        assert_eq!(o.reparsed, 2);
        assert!(load(&root).is_some());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn no_disk_cache_outside_git() {
        let root = temp_root("nogit");
        build(&root, false).unwrap();
        assert!(!cache_path(&root).exists());
        let _ = std::fs::remove_dir_all(&root);
    }
}
