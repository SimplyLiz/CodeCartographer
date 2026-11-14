use anyhow::Result;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use walkdir::WalkDir;

pub const IGNORED_DIRS: &[&str] = &[
    "node_modules",
    ".git",
    "target",
    "dist",
    "vendor",
    // Vendored / bundled third-party source. These are checked into many repos
    // (e.g. Godot's `thirdparty/`) so .gitignore never excludes them, yet they
    // dominate every structural metric with code the team does not own.
    "thirdparty",
    "third_party",
    "third-party",
    "3rdparty",
    "external",
    "externals",
    ".next",
    "build",
    // SwiftPM's build tree. Holds generated manifests, a full `checkouts/` copy
    // of every dependency's source, and an `index-build/` duplicate of both —
    // multi-GB on real projects, and none of it is code the team owns.
    ".build",
    "out",
    ".env",
    "__pycache__",
    ".venv",
    "venv",
    ".idea",
    ".vscode",
    "coverage",
    ".nuxt",
];

// =============================================================================
// CONTEXT SAFETY: "Villain" Hard-Coded Noise Blacklist
// These files are universally noise for LLMs - they burn tokens and crash context
// =============================================================================

/// Lock files - The #1 enemy of context windows
pub const NOISE_LOCK_FILES: &[&str] = &[
    // JavaScript ecosystem
    "package-lock.json",
    "yarn.lock",
    "pnpm-lock.yaml",
    "bun.lockb",
    // Rust
    "Cargo.lock",
    // Python
    "poetry.lock",
    "Pipfile.lock",
    // Ruby
    "Gemfile.lock",
    // PHP
    "composer.lock",
    // .NET
    "packages.lock.json",
    // Go
    "go.sum",
];

/// Log file extensions
pub const NOISE_LOG_EXTENSIONS: &[&str] = &["log"];

/// Source map extensions
pub const NOISE_MAP_EXTENSIONS: &[&str] = &["map"];

/// Minified JS pattern suffix
pub const NOISE_MINIFIED_JS_SUFFIX: &str = ".min.js";

/// Minified CSS pattern suffix
pub const NOISE_MINIFIED_CSS_SUFFIX: &str = ".min.css";

/// Binary/image extensions to always ignore
pub const NOISE_BINARY_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "ico", "gif", "webp", "bmp"];

/// Directories that are pure noise (already in IGNORED_DIRS but explicit for reporting)
#[allow(dead_code)]
pub const NOISE_DIRS: &[&str] = &["dist", "build", ".next", "out", "target"];

/// SVG size threshold in bytes (only ignore if > 2KB)
pub const SVG_SIZE_THRESHOLD: u64 = 2048;

/// Represents a file that was ignored due to noise filtering
#[derive(Debug, Clone)]
pub struct IgnoredFile {
    #[allow(dead_code)]
    pub path: String,
    #[allow(dead_code)]
    pub reason: NoiseReason,
    pub estimated_tokens: usize,
}

/// Why a file was ignored
#[derive(Debug, Clone)]
pub enum NoiseReason {
    LockFile,
    LogFile,
    SourceMap,
    MinifiedJs,
    MinifiedCss,
    BinaryImage,
    #[allow(dead_code)]
    LargeSvg(u64), // size in bytes
    #[allow(dead_code)]
    NoiseDirectory(String),
}

impl NoiseReason {
    #[allow(dead_code)]
    pub fn description(&self) -> String {
        match self {
            NoiseReason::LockFile => "Lock file".to_string(),
            NoiseReason::LogFile => "Log file".to_string(),
            NoiseReason::SourceMap => "Source map".to_string(),
            NoiseReason::MinifiedJs => "Minified JS".to_string(),
            NoiseReason::MinifiedCss => "Minified CSS".to_string(),
            NoiseReason::BinaryImage => "Binary image".to_string(),
            NoiseReason::LargeSvg(size) => format!("Large SVG ({}KB)", size / 1024),
            NoiseReason::NoiseDirectory(dir) => format!("In {} folder", dir),
        }
    }
}

/// Result of scanning with noise tracking
#[derive(Debug, Default)]
pub struct ScanResult {
    pub files: Vec<PathBuf>,
    pub ignored_noise: Vec<IgnoredFile>,
}

// SECURITY: Hard-blocked files - NEVER include these
pub const IGNORED_FILES: &[&str] = &[
    // System
    ".DS_Store",
    // Secrets & credentials
    ".env",
    ".env.local",
    ".env.production",
    ".env.development",
    "id_rsa",
    "id_rsa.pub",
    "id_ed25519",
    "id_ed25519.pub",
    "id_dsa",
    "*.pem",
    "*.key",
    "*.p12",
    "*.pfx",
    "secrets.yaml",
    "secrets.yml",
    "secrets.json",
    ".npmrc",
    ".pypirc",
    "credentials",
    "credentials.json",
    "service-account.json",
    // Output files
    "context.xml",
    "context.json",
    "context.md",
    "codecartographer_map.xml",
    "codecartographer_map.md",
    "codecartographer_map.json",
    ".codecartographer_memory.json",
    ".codecartographer_memory.json",
    // CodeCartographer runtime state files
    ".codecartographer_cache.json",
    ".codecartographer_cache.json",
    ".codecartographer_watch_state.json",
    ".codecartographer_history.json",
];

// Patterns for extension-based blocking
pub const BLOCKED_EXTENSIONS: &[&str] = &["pem", "key", "p12", "pfx", "jks", "keystore"];

pub const BLOCKED_PATTERNS: &[&str] = &[
    "id_rsa",
    "id_dsa",
    "id_ed25519",
    "id_ecdsa",
    "aws_access",
    "aws_secret",
    "credentials",
];

// =============================================================================
// .codecartographerignore support
// =============================================================================

/// A compiled pattern from .codecartographerignore
pub struct CodeCartographerIgnorePattern {
    pub negate: bool,
    filename_only: bool,
    /// Pattern ended in `/` — matches directories only, never a plain file of
    /// that name (gitignore semantics).
    dir_only: bool,
    regex: regex::Regex,
}

impl CodeCartographerIgnorePattern {
    fn parse(line: &str) -> Option<Self> {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            return None;
        }
        let (negate, pattern) = if let Some(rest) = line.strip_prefix('!') {
            (true, rest)
        } else {
            (false, line)
        };
        // Trailing '/' → directory-only rule (`.build/`, `dist/`).
        let dir_only = pattern.ends_with('/');
        // No '/' in pattern (ignoring trailing slash) → match filename only
        let filename_only = !pattern.trim_end_matches('/').contains('/');
        let pattern = pattern.trim_end_matches('/');
        // A leading '/' anchors to the repo root; the rest is matched relatively.
        let pattern = pattern.strip_prefix('/').unwrap_or(pattern);
        let regex = glob_to_regex(pattern)?;
        Some(Self {
            negate,
            filename_only,
            dir_only,
            regex,
        })
    }

    /// Match a repo-relative *file* path against this pattern.
    ///
    /// gitignore excludes a directory's whole subtree, so a rule has to be
    /// tested against every ancestor of the path — not just the path itself.
    /// Without that, `.build/` only ever matched a file literally named
    /// `.build` and let the entire build tree through.
    fn matches(&self, rel_path: &str) -> bool {
        if self.filename_only {
            // A pattern with no '/' applies at any depth: to the entry itself
            // or to any directory above it.
            let mut comps: Vec<&str> = rel_path.split('/').collect();
            if self.dir_only {
                comps.pop(); // directory-only: never match the leaf file
            }
            comps.iter().any(|c| self.regex.is_match(c))
        } else {
            // Anchored pattern: the full path, or any ancestor directory.
            if !self.dir_only && self.regex.is_match(rel_path) {
                return true;
            }
            let mut idx = 0;
            while let Some(pos) = rel_path[idx..].find('/') {
                let end = idx + pos;
                if self.regex.is_match(&rel_path[..end]) {
                    return true;
                }
                idx = end + 1;
            }
            false
        }
    }

    /// Match a repo-relative *directory* path. Unlike `matches`, a `dir_only`
    /// rule matches the directory itself here — that is what lets a negation
    /// (`!.claude/skills/`) rescue a directory its parent rule excluded.
    fn matches_dir(&self, rel_dir: &str) -> bool {
        if self.filename_only {
            return rel_dir.split('/').any(|c| self.regex.is_match(c));
        }
        if self.regex.is_match(rel_dir) {
            return true;
        }
        let mut idx = 0;
        while let Some(pos) = rel_dir[idx..].find('/') {
            let end = idx + pos;
            if self.regex.is_match(&rel_dir[..end]) {
                return true;
            }
            idx = end + 1;
        }
        false
    }
}

/// Length of the bracket expression starting at `chars[0] == '['`, including
/// both brackets, or None if it is never closed. A leading '!'/'^' and a ']' in
/// first position are part of the class, not its terminator.
fn class_len(chars: &[char]) -> Option<usize> {
    let mut i = 1;
    if i < chars.len() && (chars[i] == '!' || chars[i] == '^') {
        i += 1;
    }
    if i < chars.len() && chars[i] == ']' {
        i += 1;
    }
    while i < chars.len() {
        if chars[i] == ']' {
            return Some(i + 1);
        }
        i += 1;
    }
    None
}

fn glob_to_regex(pattern: &str) -> Option<regex::Regex> {
    let mut out = String::from("^");
    let chars: Vec<char> = pattern.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        match chars[i] {
            '*' if i + 1 < chars.len() && chars[i + 1] == '*' => {
                out.push_str(".*");
                i += 2;
                if i < chars.len() && chars[i] == '/' {
                    i += 1;
                }
            }
            '*' => {
                out.push_str("[^/]*");
                i += 1;
            }
            '?' => {
                out.push_str("[^/]");
                i += 1;
            }
            // gitignore supports bracket expressions — [0-9], [a-z], [!x] to
            // negate. Escaping the brackets turned every one of them into
            // literal text, so a rule like `*_[0-9][0-9].json` could never match.
            '[' => match class_len(&chars[i..]) {
                Some(len) => {
                    out.push('[');
                    let end = i + len - 1; // index of the closing ']'
                    let mut j = i + 1;
                    if chars[j] == '!' || chars[j] == '^' {
                        out.push('^');
                        j += 1;
                    }
                    if j < end && chars[j] == ']' {
                        out.push_str("\\]"); // ']' first in a class is a literal
                        j += 1;
                    }
                    while j < end {
                        // '-' passes through so ranges keep working; '\' and '['
                        // are escaped so they cannot open a nested class.
                        match chars[j] {
                            '\\' => out.push_str("\\\\"),
                            '[' => out.push_str("\\["),
                            c => out.push(c),
                        }
                        j += 1;
                    }
                    out.push(']');
                    i += len;
                }
                // Unterminated '[' — treat it as a literal, like git does.
                None => {
                    out.push_str("\\[");
                    i += 1;
                }
            },
            '.' | '+' | '^' | '$' | '{' | '}' | '(' | ')' | '|' | ']' | '\\' => {
                out.push('\\');
                out.push(chars[i]);
                i += 1;
            }
            c => {
                out.push(c);
                i += 1;
            }
        }
    }
    out.push('$');
    regex::Regex::new(&out).ok()
}

fn load_ignore_file(path: &Path) -> Vec<CodeCartographerIgnorePattern> {
    fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .filter_map(CodeCartographerIgnorePattern::parse)
        .collect()
}

/// Ignore rules resolved the way git resolves them: a `.gitignore` applies to
/// its own directory's subtree, with its patterns relative to that directory.
///
/// Reading only the root file misses most exclusions in multi-package repos —
/// a nested `DMP/.gitignore` containing `/results/` left 18k benchmark-output
/// JSONs in the index because the root file's `/results/` is anchored at the
/// repo root and never applied there.
///
/// Layers are loaded lazily per directory and cached; most directories have no
/// ignore file, which caches as an empty layer.
pub struct IgnoreIndex {
    root: PathBuf,
    layers: Mutex<HashMap<String, Arc<Vec<CodeCartographerIgnorePattern>>>>,
    /// Repository-wide excludes that live outside the tree: the global
    /// `core.excludesFile` and `$GIT_COMMON_DIR/info/exclude`.
    repo: RepoExcludes,
}

/// Excludes git reads from outside the working tree. Their patterns are
/// relative to the repository's top level, which can sit above the scan root.
#[derive(Default)]
struct RepoExcludes {
    /// Scan root relative to the git top level, with a trailing '/' (or "").
    prefix: String,
    /// `core.excludesFile` first, then `info/exclude`: lowest precedence first,
    /// since the last matching rule wins.
    patterns: Vec<CodeCartographerIgnorePattern>,
}

impl RepoExcludes {
    fn load(root: &Path) -> Self {
        let Some((toplevel, gitdir)) = find_git_dir(root) else {
            return Self::default();
        };
        let prefix = root
            .strip_prefix(&toplevel)
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .unwrap_or_default();
        let prefix = if prefix.is_empty() { prefix } else { format!("{prefix}/") };

        let mut patterns = Vec::new();
        if let Some(global) = global_excludes_file(&toplevel) {
            patterns.extend(load_ignore_file(&global));
        }
        patterns.extend(load_ignore_file(&common_dir(&gitdir).join("info").join("exclude")));
        Self { prefix, patterns }
    }
}

/// The repository containing `root`: (top level of the working tree, git dir).
/// Handles a `.git` *file* (`gitdir: <path>`), which is what linked worktrees
/// and submodules have.
fn find_git_dir(root: &Path) -> Option<(PathBuf, PathBuf)> {
    for dir in root.ancestors() {
        let dot_git = dir.join(".git");
        let Ok(meta) = fs::metadata(&dot_git) else {
            continue;
        };
        if meta.is_dir() {
            return Some((dir.to_path_buf(), dot_git));
        }
        let text = fs::read_to_string(&dot_git).ok()?;
        let target = text.lines().find_map(|l| l.trim().strip_prefix("gitdir:"))?.trim();
        return Some((dir.to_path_buf(), dir.join(target)));
    }
    None
}

/// `$GIT_COMMON_DIR`: a linked worktree's git dir names the main repository's
/// git dir in its `commondir` file; `info/exclude` lives there.
fn common_dir(gitdir: &Path) -> PathBuf {
    match fs::read_to_string(gitdir.join("commondir")) {
        Ok(rel) if !rel.trim().is_empty() => gitdir.join(rel.trim()),
        _ => gitdir.to_path_buf(),
    }
}

/// `core.excludesFile` as git resolves it (repo, global and system config,
/// `~` expanded), falling back to git's default `$XDG_CONFIG_HOME/git/ignore`
/// or `~/.config/git/ignore`.
fn global_excludes_file(toplevel: &Path) -> Option<PathBuf> {
    let configured = std::process::Command::new("git")
        .arg("-C")
        .arg(toplevel)
        .args(["config", "--path", "--get", "core.excludesFile"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty());
    if let Some(p) = configured {
        let p = PathBuf::from(p);
        return Some(if p.is_absolute() { p } else { toplevel.join(p) });
    }
    match std::env::var_os("XDG_CONFIG_HOME").filter(|v| !v.is_empty()) {
        Some(xdg) => Some(PathBuf::from(xdg).join("git").join("ignore")),
        None => std::env::var_os("HOME")
            .map(|h| PathBuf::from(h).join(".config").join("git").join("ignore")),
    }
}

impl IgnoreIndex {
    pub fn new(root: &Path) -> Self {
        Self {
            root: root.to_path_buf(),
            layers: Mutex::new(HashMap::new()),
            repo: RepoExcludes::load(root),
        }
    }

    /// Patterns declared in `dir_rel` ("" = repo root).
    fn layer(&self, dir_rel: &str) -> Arc<Vec<CodeCartographerIgnorePattern>> {
        if let Some(hit) = self.layers.lock().unwrap().get(dir_rel) {
            return Arc::clone(hit);
        }
        let dir = if dir_rel.is_empty() {
            self.root.clone()
        } else {
            self.root.join(dir_rel)
        };
        // .codecartographerignore is appended last so its rules (including
        // negations) take precedence over .gitignore at the same level.
        let mut pats = load_ignore_file(&dir.join(".gitignore"));
        pats.extend(load_ignore_file(&dir.join(".codecartographerignore")));
        let layer = Arc::new(pats);
        self.layers
            .lock()
            .unwrap()
            .insert(dir_rel.to_string(), Arc::clone(&layer));
        layer
    }

    /// Evaluate `rel` (a repo-relative path, '/'-separated) against every
    /// ancestor directory's rules, outermost first — last match wins, so a
    /// deeper negation can re-include what an outer rule excluded.
    fn is_ignored(&self, rel: &str, is_dir: bool) -> bool {
        // Repo-wide excludes have the lowest precedence; any .gitignore rule
        // below can override them.
        let mut ignored = !self.repo.patterns.is_empty()
            && eval_layer(&format!("{}{rel}", self.repo.prefix), is_dir, &self.repo.patterns)
                .unwrap_or(false);
        let mut dir_end = 0; // "" = root layer
        loop {
            let dir_rel = &rel[..dir_end];
            let sub = if dir_end == 0 {
                rel
            } else {
                &rel[dir_end + 1..]
            };
            if let Some(verdict) = eval_layer(sub, is_dir, &self.layer(dir_rel)) {
                ignored = verdict;
            }
            // Advance to the next ancestor directory, if any.
            let next = if dir_end == 0 { 0 } else { dir_end + 1 };
            match rel[next..].find('/') {
                Some(pos) => dir_end = next + pos,
                None => return ignored,
            }
        }
    }
}

/// Last-match-wins verdict for one directory's rules. `None` means no rule in
/// this layer applied, which is what lets an outer layer's verdict stand.
fn eval_layer(
    rel: &str,
    is_dir: bool,
    patterns: &[CodeCartographerIgnorePattern],
) -> Option<bool> {
    let mut verdict = None;
    for p in patterns {
        let hit = if is_dir {
            p.matches_dir(rel)
        } else {
            p.matches(rel)
        };
        if hit {
            verdict = Some(!p.negate);
        }
    }
    verdict
}

#[cfg(test)]
fn is_codecartographer_ignored(rel_path: &str, patterns: &[CodeCartographerIgnorePattern]) -> bool {
    eval_layer(rel_path, false, patterns).unwrap_or(false)
}

#[cfg(test)]
fn is_dir_ignored(rel_dir: &str, patterns: &[CodeCartographerIgnorePattern]) -> bool {
    eval_layer(rel_dir, true, patterns).unwrap_or(false)
}

// =============================================================================
// Source-file classification
// =============================================================================

/// Extensions that indicate analysable source code.  Files whose extension is
/// not in this list (e.g. `.md`, `.toml`, `LICENSE`, `Makefile`) are excluded
/// from structural analysis commands (hotspots, dead, simulate, health, check).
#[allow(dead_code)]
pub const SOURCE_EXTENSIONS: &[&str] = &[
    "rs", "py", "js", "mjs", "cjs", "ts", "tsx", "jsx",
    "go", "java", "kt", "kts", "swift", "c", "cc", "cpp", "cxx",
    "h", "hpp", "cs", "rb", "php", "scala", "clj", "cljs",
    "elm", "hs", "ml", "mli", "ex", "exs", "lua", "r", "dart",
    "vue", "svelte", "zig", "nim", "cr",
];

#[allow(dead_code)]
pub fn is_source_file(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|ext| {
            let lower = ext.to_lowercase();
            SOURCE_EXTENSIONS.contains(&lower.as_str())
        })
        .unwrap_or(false)
}

// =============================================================================

/// Legacy function for backward compatibility
#[allow(dead_code)]
pub fn scan_files(root: &Path) -> Result<Vec<PathBuf>> {
    let result = scan_files_with_noise_tracking(root)?;
    Ok(result.files)
}

/// Scan files with noise tracking - returns both clean files and ignored noise
pub fn scan_files_with_noise_tracking(root: &Path) -> Result<ScanResult> {
    let ignored_dirs: HashSet<&str> = IGNORED_DIRS.iter().copied().collect();
    let ignored_files: HashSet<&str> = IGNORED_FILES.iter().copied().collect();
    // Ignore rules are resolved per directory, git-style: a nested .gitignore
    // applies to its own subtree with its patterns relative to that directory.
    let ignore_index = IgnoreIndex::new(root);

    let mut result = ScanResult::default();

    let all_entries: Vec<_> = WalkDir::new(root)
        .into_iter()
        .filter_entry(|e| !should_skip_dir(e, &ignored_dirs, root, &ignore_index))
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .collect();

    for entry in all_entries {
        let path = entry.path();

        // Check security blocks first (these are never included, not even reported)
        if is_blocked_file(path, &ignored_files) {
            continue;
        }

        // Check .gitignore + .codecartographerignore patterns (silently excluded)
        let rel = path
            .strip_prefix(root)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");
        if ignore_index.is_ignored(&rel, false) {
            continue;
        }

        // Check noise patterns
        if let Some(ignored) = check_noise_file(path, root) {
            result.ignored_noise.push(ignored);
            continue;
        }

        result.files.push(entry.into_path());
    }

    result.files.sort();
    result
        .ignored_noise
        .sort_by(|a, b| b.estimated_tokens.cmp(&a.estimated_tokens));
    Ok(result)
}

/// Check if a file is noise and should be ignored (but reported)
fn check_noise_file(path: &Path, root: &Path) -> Option<IgnoredFile> {
    let filename = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    let rel_path = path
        .strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/");

    // Check lock files
    if NOISE_LOCK_FILES.contains(&filename) {
        let tokens = estimate_file_tokens(path);
        return Some(IgnoredFile {
            path: rel_path,
            reason: NoiseReason::LockFile,
            estimated_tokens: tokens,
        });
    }

    // Check extension-based noise
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        let ext_lower = ext.to_lowercase();

        // Binary images - always ignore
        if NOISE_BINARY_EXTENSIONS.contains(&ext_lower.as_str()) {
            return Some(IgnoredFile {
                path: rel_path,
                reason: NoiseReason::BinaryImage,
                estimated_tokens: 0,
            });
        }

        // Log files
        if NOISE_LOG_EXTENSIONS.contains(&ext_lower.as_str()) {
            let tokens = estimate_file_tokens(path);
            return Some(IgnoredFile {
                path: rel_path,
                reason: NoiseReason::LogFile,
                estimated_tokens: tokens,
            });
        }

        // Source maps
        if NOISE_MAP_EXTENSIONS.contains(&ext_lower.as_str()) {
            let tokens = estimate_file_tokens(path);
            return Some(IgnoredFile {
                path: rel_path,
                reason: NoiseReason::SourceMap,
                estimated_tokens: tokens,
            });
        }

        // Large SVGs (> 2KB)
        if ext_lower == "svg" {
            if let Ok(metadata) = fs::metadata(path) {
                let size = metadata.len();
                if size > SVG_SIZE_THRESHOLD {
                    let tokens = estimate_file_tokens(path);
                    return Some(IgnoredFile {
                        path: rel_path,
                        reason: NoiseReason::LargeSvg(size),
                        estimated_tokens: tokens,
                    });
                }
            }
        }
    }

    // Check minified JS
    if filename.ends_with(NOISE_MINIFIED_JS_SUFFIX) {
        let tokens = estimate_file_tokens(path);
        return Some(IgnoredFile {
            path: rel_path,
            reason: NoiseReason::MinifiedJs,
            estimated_tokens: tokens,
        });
    }

    // Check minified CSS
    if filename.ends_with(NOISE_MINIFIED_CSS_SUFFIX) {
        let tokens = estimate_file_tokens(path);
        return Some(IgnoredFile {
            path: rel_path,
            reason: NoiseReason::MinifiedCss,
            estimated_tokens: tokens,
        });
    }

    None
}

/// Estimate tokens for a file (rough: ~4 chars per token)
fn estimate_file_tokens(path: &Path) -> usize {
    fs::metadata(path)
        .map(|m| (m.len() as usize) / 4)
        .unwrap_or(0)
}

fn is_blocked_file(path: &Path, ignored_files: &HashSet<&str>) -> bool {
    let filename = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

    // Direct filename match
    if ignored_files.contains(filename) {
        return true;
    }

    // Extension-based blocking
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        if BLOCKED_EXTENSIONS.contains(&ext) {
            return true;
        }
    }

    // Pattern-based blocking (contains check)
    let lower = filename.to_lowercase();
    for pattern in BLOCKED_PATTERNS {
        if lower.contains(pattern) {
            return true;
        }
    }

    false
}

fn should_skip_dir(
    entry: &walkdir::DirEntry,
    ignored: &HashSet<&str>,
    root: &Path,
    ignore_index: &IgnoreIndex,
) -> bool {
    if entry
        .file_name()
        .to_str()
        .map(|s| ignored.contains(s))
        .unwrap_or(false)
    {
        return true;
    }
    // Prune ignored directories before descending — a gitignored build tree can
    // be multi-GB, and walking it costs the same whether or not we keep the
    // files. Same caveat as git: a rule that excludes a directory cannot be
    // undone by a negation deeper inside it.
    if entry.file_type().is_dir() {
        let path = entry.path();
        let rel = path
            .strip_prefix(root)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");
        if !rel.is_empty() && ignore_index.is_ignored(&rel, true) {
            return true;
        }
    }
    false
}

pub fn is_ignored_path(path: &Path) -> bool {
    path.components().any(|c| {
        c.as_os_str()
            .to_str()
            .map(|s| IGNORED_DIRS.contains(&s) || IGNORED_FILES.contains(&s))
            .unwrap_or(false)
    })
}

#[cfg(test)]
mod ignore_pattern_tests {
    use super::*;

    fn compile(lines: &[&str]) -> Vec<CodeCartographerIgnorePattern> {
        lines
            .iter()
            .filter_map(|l| CodeCartographerIgnorePattern::parse(l))
            .collect()
    }

    /// A `dir/` rule must exclude the whole subtree, not just a file of that
    /// name. This is the bug that let SwiftPM's `.build/` (multi-GB: generated
    /// manifests, a `checkouts/` copy of every dependency, plus an
    /// `index-build/` duplicate) into the index.
    #[test]
    fn dir_pattern_excludes_directory_contents() {
        let pats = compile(&[".build/", "dist/", "coverage/"]);
        for p in [
            ".build/plugin-tools.yaml",
            ".build/checkouts/swift-atomics/Tests/AtomicsFolding.swift",
            "KlippsterWasm/.build/debug.yaml",
            "dist/bundle.js",
            "coverage/lcov-report/index.html",
        ] {
            assert!(is_codecartographer_ignored(p, &pats), "not ignored: {}", p);
        }
    }

    /// Directory-only rules must not swallow a plain file of the same name.
    #[test]
    fn dir_pattern_does_not_match_a_plain_file() {
        let pats = compile(&["dist/"]);
        assert!(!is_codecartographer_ignored("dist", &pats));
        assert!(!is_codecartographer_ignored("src/dist", &pats));
    }

    #[test]
    fn bare_name_applies_at_any_depth() {
        let pats = compile(&["node_modules", "*.log"]);
        assert!(is_codecartographer_ignored("a/b/node_modules/x/y.js", &pats));
        assert!(is_codecartographer_ignored("logs/app.log", &pats));
        assert!(!is_codecartographer_ignored("src/main.rs", &pats));
    }

    /// A leading '/' anchors to the repo root.
    #[test]
    fn leading_slash_anchors_to_root() {
        let pats = compile(&["/build/"]);
        assert!(is_codecartographer_ignored("build/out.o", &pats));
        assert!(!is_codecartographer_ignored("sub/build/out.o", &pats));
    }

    /// The `.claude/*` + `!.claude/skills/` idiom must keep skills — at both the
    /// file level and the directory-pruning level, or the walk drops them before
    /// the file check ever runs.
    #[test]
    fn negation_rescues_subdirectory() {
        let pats = compile(&[".claude/*", "!.claude/skills/"]);
        assert!(is_codecartographer_ignored(".claude/settings.json", &pats));
        assert!(!is_codecartographer_ignored(".claude/skills/foo.md", &pats));

        assert!(is_dir_ignored(".claude/other", &pats));
        assert!(!is_dir_ignored(".claude/skills", &pats));
    }

    #[test]
    fn dir_pruning_matches_file_level_decisions() {
        let pats = compile(&[".build/", "target/"]);
        assert!(is_dir_ignored(".build", &pats));
        assert!(is_dir_ignored("KlippsterWasm/.build", &pats));
        assert!(!is_dir_ignored("src", &pats));
    }
}

#[cfg(test)]
mod nested_ignore_tests {
    use super::*;

    fn write(path: &Path, body: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, body).unwrap();
    }

    fn tmpdir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("cc-nested-ignore-{}-{}", tag, std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    /// A nested .gitignore applies to its own subtree, with patterns relative to
    /// that directory. The root file's `/results/` is anchored at the repo root
    /// and must NOT reach into `DMP/`; `DMP/.gitignore`'s `/results/` must.
    #[test]
    fn nested_gitignore_applies_to_its_own_subtree() {
        let root = tmpdir("subtree");
        write(&root.join(".gitignore"), "/results/\n");
        write(&root.join("DMP/.gitignore"), "/results/\n");

        let idx = IgnoreIndex::new(&root);
        assert!(idx.is_ignored("results/out.json", false));
        assert!(idx.is_ignored("DMP/results/run_1/seed_50.json", false));
        assert!(idx.is_ignored("DMP/results", true));
        // Root-anchored rule must not leak into the nested package.
        assert!(!idx.is_ignored("DMP/src/main.rs", false));
        assert!(!idx.is_ignored("DMP/results.rs", false));

        let _ = fs::remove_dir_all(&root);
    }

    /// Deeper rules are evaluated after outer ones, so a nested negation wins.
    #[test]
    fn nested_negation_overrides_outer_rule() {
        let root = tmpdir("negation");
        write(&root.join(".gitignore"), "*.json\n");
        write(&root.join("fixtures/.gitignore"), "!keep.json\n");

        let idx = IgnoreIndex::new(&root);
        assert!(idx.is_ignored("data/x.json", false));
        assert!(!idx.is_ignored("fixtures/keep.json", false));
        assert!(idx.is_ignored("fixtures/other.json", false));

        let _ = fs::remove_dir_all(&root);
    }

    /// The end-to-end path: a nested build tree must not reach the file list.
    #[test]
    fn scan_excludes_nested_ignored_tree() {
        let root = tmpdir("scan");
        write(&root.join(".gitignore"), "");
        write(&root.join("src/main.rs"), "fn main() {}\n");
        write(&root.join("pkg/.gitignore"), "/generated/\n");
        write(&root.join("pkg/lib.rs"), "pub fn f() {}\n");
        write(&root.join("pkg/generated/huge.rs"), "// generated\n");
        write(&root.join("pkg/generated/deep/more.rs"), "// generated\n");

        let files = scan_files_with_noise_tracking(&root).unwrap().files;
        let rels: Vec<String> = files
            .iter()
            .map(|p| {
                p.strip_prefix(&root)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/")
            })
            .collect();

        assert!(rels.iter().any(|r| r == "src/main.rs"), "got {:?}", rels);
        assert!(rels.iter().any(|r| r == "pkg/lib.rs"), "got {:?}", rels);
        assert!(
            !rels.iter().any(|r| r.starts_with("pkg/generated/")),
            "nested ignored tree leaked: {:?}",
            rels
        );

        let _ = fs::remove_dir_all(&root);
    }
}

#[cfg(test)]
mod glob_class_tests {
    use super::*;

    fn m(pattern: &str, path: &str) -> bool {
        CodeCartographerIgnorePattern::parse(pattern)
            .expect("pattern failed to compile")
            .matches(path)
    }

    /// The rule that let two 8.6 MB benchmark JSONs into the index: bracket
    /// expressions were escaped into literals, so this could never match.
    #[test]
    fn timestamped_artifacts() {
        let p = "docs/benchmark_results/**/*_[0-9][0-9][0-9][0-9][0-9][0-9][0-9][0-9][0-9][0-9].json";
        assert!(m(p, "docs/benchmark_results/v5.2.0/nonlinear_probe_1783337559.json"));
        assert!(!m(p, "docs/benchmark_results/v5.2.0/summary.json"));
    }

    #[test]
    fn ranges_and_negation() {
        assert!(m("tmp[0-9].log", "tmp7.log"));
        assert!(!m("tmp[0-9].log", "tmpx.log"));
        assert!(m("[a-c]*.rs", "b_mod.rs"));
        assert!(m("file[!0-9].txt", "filea.txt"));
        assert!(!m("file[!0-9].txt", "file4.txt"));
    }

    /// A '[' that never closes is a literal, the way git treats it.
    #[test]
    fn unterminated_bracket_is_literal() {
        assert!(m("weird[name.txt", "weird[name.txt"));
    }

    /// Wildcards must keep their existing meaning: '*' stays within one path
    /// segment, '**' crosses separators.
    #[test]
    fn wildcards_unchanged() {
        assert!(m("*.log", "app.log"));
        assert!(!m("src/*.rs", "src/a/b.rs"));
        assert!(m("src/**/*.rs", "src/a/b.rs"));
        assert!(m("f?o.txt", "foo.txt"));
    }
}

#[cfg(test)]
mod repo_exclude_tests {
    use super::*;
    use std::process::Command;

    fn write(path: &Path, body: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, body).unwrap();
    }

    fn tmpdir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("cc-repo-exclude-{}-{}", tag, std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d.canonicalize().unwrap()
    }

    fn git(dir: &Path, args: &[&str]) {
        let ok = Command::new("git").arg("-C").arg(dir).args(args).status().unwrap().success();
        assert!(ok, "git {args:?}");
    }

    fn scanned(root: &Path) -> Vec<String> {
        scan_files_with_noise_tracking(root)
            .unwrap()
            .files
            .iter()
            .map(|p| p.strip_prefix(root).unwrap().to_string_lossy().replace('\\', "/"))
            .collect()
    }

    /// The CKB case: `.claude/worktrees/` (two full repo copies) is excluded
    /// only through `.git/info/exclude`. Without reading it the scan walked
    /// both copies and MapProject took >10 min instead of 2.3 s.
    #[test]
    fn info_exclude_and_core_excludes_file_apply() {
        let root = tmpdir("info");
        git(&root, &["init", "-q"]);
        let global = root.join("..").join(format!("cc-global-ignore-{}", std::process::id()));
        write(&global, "*.scratch\n");
        // Repo-local config wins over the user's real global setting, so the
        // test does not depend on the machine's ~/.gitconfig.
        git(&root, &["config", "core.excludesFile", global.to_str().unwrap()]);
        write(&root.join(".git/info/exclude"), "# local\n.claude/worktrees/\n/only-root.rs\n");
        write(&root.join("src/main.rs"), "fn main() {}\n");
        write(&root.join(".claude/worktrees/copy/src/main.rs"), "fn main() {}\n");
        write(&root.join("only-root.rs"), "\n");
        write(&root.join("src/only-root.rs"), "\n");
        write(&root.join("src/notes.scratch"), "\n");
        // A .gitignore negation overrides the lower-precedence repo excludes.
        write(&root.join("keep/.gitignore"), "!keep.scratch\n");
        write(&root.join("keep/keep.scratch"), "\n");

        let rels = scanned(&root);
        assert!(rels.contains(&"src/main.rs".to_string()), "{rels:?}");
        assert!(!rels.iter().any(|r| r.starts_with(".claude/")), "info/exclude ignored: {rels:?}");
        assert!(!rels.contains(&"only-root.rs".to_string()), "{rels:?}");
        assert!(rels.contains(&"src/only-root.rs".to_string()), "anchor is root-relative: {rels:?}");
        assert!(!rels.contains(&"src/notes.scratch".to_string()), "core.excludesFile ignored: {rels:?}");
        assert!(rels.contains(&"keep/keep.scratch".to_string()), "{rels:?}");

        let _ = fs::remove_file(&global);
        let _ = fs::remove_dir_all(&root);
    }

    /// Scanning a subdirectory: info/exclude patterns stay relative to the
    /// repository top level.
    #[test]
    fn info_exclude_applies_below_the_top_level() {
        let root = tmpdir("sub");
        git(&root, &["init", "-q"]);
        write(&root.join(".git/info/exclude"), "/pkg/gen/\n");
        write(&root.join("pkg/gen/a.rs"), "\n");
        write(&root.join("pkg/lib.rs"), "\n");
        let rels = scanned(&root.join("pkg"));
        assert_eq!(rels, vec!["lib.rs".to_string()]);
        let _ = fs::remove_dir_all(&root);
    }

    /// A linked worktree has a `.git` file; info/exclude comes from the main
    /// repository's git dir via `commondir`.
    #[test]
    fn worktree_reads_common_dir_exclude() {
        let main = tmpdir("wt-main");
        git(&main, &["init", "-q"]);
        write(&main.join(".git/info/exclude"), "secret-out/\n");
        let wt_gitdir = main.join(".git/worktrees/wt");
        write(&wt_gitdir.join("commondir"), "../..\n");
        let wt = tmpdir("wt");
        write(&wt.join(".git"), &format!("gitdir: {}\n", wt_gitdir.display()));
        write(&wt.join("src/a.rs"), "\n");
        write(&wt.join("secret-out/b.rs"), "\n");
        let rels = scanned(&wt);
        assert_eq!(rels, vec!["src/a.rs".to_string()]);
        let _ = fs::remove_dir_all(&wt);
        let _ = fs::remove_dir_all(&main);
    }
}
