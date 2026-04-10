//! Content search and file discovery — grep-like text/regex search + glob find.
//!
//! Reuses the existing file scanner (`.cartographerignore`, noise filter, security
//! block) unless `no_ignore` is set, in which case raw `walkdir` is used.

use crate::scanner::{is_ignored_path, scan_files_with_noise_tracking};
use regex::{Regex, RegexBuilder};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

// ---------------------------------------------------------------------------
// Search options
// ---------------------------------------------------------------------------

/// Options for a content search request.
#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SearchOptions {
    /// Treat `pattern` as a literal string (escape regex metacharacters).
    #[serde(default)]
    pub literal: bool,
    /// Case-sensitive matching — default `true`.
    #[serde(default = "default_true")]
    pub case_sensitive: bool,
    /// Symmetric context lines before and after each match (like `grep -C`).
    /// Ignored when `before_context` or `after_context` is nonzero.
    #[serde(default)]
    pub context_lines: usize,
    /// Lines of context before each match (like `grep -B`).
    #[serde(default)]
    pub before_context: usize,
    /// Lines of context after each match (like `grep -A`).
    #[serde(default)]
    pub after_context: usize,
    /// Cap on returned matches (0 = unlimited). Default 100.
    #[serde(default = "default_max")]
    pub max_results: usize,
    /// Include only files matching this glob (e.g. `"*.rs"` or `"src/**/*.ts"`).
    #[serde(default)]
    pub file_glob: Option<String>,
    /// Exclude files matching this glob.
    #[serde(default)]
    pub exclude_glob: Option<String>,
    /// Additional patterns OR'd with the primary pattern (like `grep -e`).
    #[serde(default)]
    pub extra_patterns: Vec<String>,
    /// Invert match — return lines that do NOT match (like `grep -v`).
    #[serde(default)]
    pub invert_match: bool,
    /// Whole-word matching — wraps pattern in `\b…\b` (like `grep -w`).
    #[serde(default)]
    pub word_regexp: bool,
    /// Print only the matched portion of each line (like `grep -o`).
    #[serde(default)]
    pub only_matching: bool,
    /// Return only file paths that contain matches (like `grep -l`).
    #[serde(default)]
    pub files_with_matches: bool,
    /// Return only file paths that contain NO matches (like `grep --files-without-match`).
    #[serde(default)]
    pub files_without_match: bool,
    /// Return per-file match counts instead of match lines (like `grep -c`).
    #[serde(default)]
    pub count_only: bool,
    /// Bypass noise/vendor/generated-file filter — search all text files.
    #[serde(default)]
    pub no_ignore: bool,
    /// Restrict search to this repo-relative subdirectory (e.g. `"src/api"`).
    #[serde(default)]
    pub search_path: Option<String>,
}

fn default_true() -> bool { true }
fn default_max() -> usize { 100 }

impl Default for SearchOptions {
    fn default() -> Self {
        Self {
            literal: false,
            case_sensitive: true,
            context_lines: 0,
            before_context: 0,
            after_context: 0,
            max_results: 100,
            file_glob: None,
            exclude_glob: None,
            extra_patterns: vec![],
            invert_match: false,
            word_regexp: false,
            only_matching: false,
            files_with_matches: false,
            files_without_match: false,
            count_only: false,
            no_ignore: false,
            search_path: None,
        }
    }
}

// ---------------------------------------------------------------------------
// Find options
// ---------------------------------------------------------------------------

/// Options for a file-find request.
#[derive(Debug, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct FindOptions {
    /// Return only files modified within this many seconds of now (e.g. 86400 = last 24 h).
    #[serde(default)]
    pub modified_since_secs: Option<u64>,
    /// Return only files with mtime newer than this repo-relative file's mtime.
    #[serde(default)]
    pub newer_than: Option<String>,
    /// Minimum file size in bytes (inclusive).
    #[serde(default)]
    pub min_size_bytes: Option<u64>,
    /// Maximum file size in bytes (inclusive).
    #[serde(default)]
    pub max_size_bytes: Option<u64>,
    /// Maximum directory depth (0 = root files only, 1 = one level deep, …).
    #[serde(default)]
    pub max_depth: Option<usize>,
    /// Bypass noise/vendor/generated-file filter — find in all files.
    #[serde(default)]
    pub no_ignore: bool,
}

// ---------------------------------------------------------------------------
// Result types — search
// ---------------------------------------------------------------------------

/// A single context line (before or after a match).
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextLine {
    pub line_number: usize,
    pub line: String,
}

/// One matching line with optional surrounding context.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContentMatch {
    /// Repo-relative path, forward-slash separated.
    pub path: String,
    pub line_number: usize,
    /// Full line text. Empty string when `only_matching` is active.
    pub line: String,
    /// Populated only when `only_matching` is true — the matched portion(s).
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub matched_texts: Vec<String>,
    /// Byte `[start, end]` offsets of every match on this line.
    /// Useful for highlight rendering without a second regex pass.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub match_ranges: Vec<[usize; 2]>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub before_context: Vec<ContextLine>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub after_context: Vec<ContextLine>,
}

/// Per-file match count (populated by `count_only` mode).
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileCount {
    pub path: String,
    pub count: usize,
}

/// Aggregated result returned by [`search_content`].
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResult {
    /// Matching lines — empty in `files_with_matches`, `files_without_match`, and `count_only` modes.
    pub matches: Vec<ContentMatch>,
    /// Total matches / files / counts returned (≤ `max_results` if truncated).
    pub total_matches: usize,
    /// Files actually read and searched.
    pub files_searched: usize,
    /// `true` when capped at `max_results`.
    pub truncated: bool,
    /// Populated when `files_with_matches` is set.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub files_with_matches: Vec<String>,
    /// Populated when `files_without_match` is set.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub files_without_match: Vec<String>,
    /// Populated when `count_only` is set.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub file_counts: Vec<FileCount>,
}

// ---------------------------------------------------------------------------
// Result types — find
// ---------------------------------------------------------------------------

/// One matching file returned by [`find_files`].
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FindFile {
    /// Repo-relative path, forward-slash separated.
    pub path: String,
    /// Detected language (e.g. "Rust", "Go", "Python"), if known.
    pub language: Option<String>,
    /// File size in bytes.
    pub size_bytes: u64,
    /// ISO-8601 last modification timestamp (UTC), if available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modified: Option<String>,
}

/// Aggregated result returned by [`find_files`].
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FindResult {
    pub files: Vec<FindFile>,
    pub total_matches: usize,
    pub truncated: bool,
}

// ---------------------------------------------------------------------------
// Core: search_content
// ---------------------------------------------------------------------------

/// Search for `pattern` across all non-noise, non-ignored files under `root`.
///
/// Pass `opts` to control matching mode, context, file filters, and output shape.
pub fn search_content(
    root: &Path,
    pattern: &str,
    opts: &SearchOptions,
) -> Result<SearchResult, String> {
    if pattern.is_empty() && opts.extra_patterns.is_empty() {
        return Err("pattern must not be empty".into());
    }

    // Build regexes — primary + all -e extras, OR'd at match time
    let mut all_res: Vec<Regex> = Vec::new();
    if !pattern.is_empty() {
        all_res.push(build_re(pattern, opts.literal, opts.word_regexp, opts.case_sensitive)?);
    }
    for ep in &opts.extra_patterns {
        if !ep.is_empty() {
            all_res.push(build_re(ep, opts.literal, opts.word_regexp, opts.case_sensitive)?);
        }
    }
    if all_res.is_empty() {
        return Err("no non-empty patterns provided".into());
    }

    // Glob filters
    let include_filter: Option<Regex> = opts.file_glob.as_deref().and_then(build_glob_regex);
    let exclude_filter: Option<Regex> = opts.exclude_glob.as_deref().and_then(build_glob_regex);

    // Effective per-side context
    let before_ctx = if opts.before_context > 0 || opts.after_context > 0 {
        opts.before_context
    } else {
        opts.context_lines
    };
    let after_ctx = if opts.before_context > 0 || opts.after_context > 0 {
        opts.after_context
    } else {
        opts.context_lines
    };

    let cap = if opts.max_results == 0 { usize::MAX } else { opts.max_results };

    let file_list = enumerate_files(root, opts.no_ignore)?;

    let mut matches: Vec<ContentMatch> = Vec::new();
    let mut files_with_m: Vec<String> = Vec::new();
    let mut files_without_m: Vec<String> = Vec::new();
    let mut file_counts: Vec<FileCount> = Vec::new();
    let mut files_searched: usize = 0;
    let mut truncated = false;

    'files: for abs_path in &file_list {
        let rel = rel_path(root, abs_path);

        // search_path prefix filter
        if let Some(ref sp) = opts.search_path {
            let sp = sp.trim_end_matches('/');
            if !rel.starts_with(&format!("{}/", sp)) && rel != sp {
                continue;
            }
        }

        // include/exclude glob
        if let Some(ref gre) = include_filter {
            if !gre.is_match(&rel) { continue; }
        }
        if let Some(ref gre) = exclude_filter {
            if gre.is_match(&rel) { continue; }
        }

        let content = match std::fs::read_to_string(abs_path) {
            Ok(c) => c,
            Err(_) => continue, // binary or unreadable — skip silently
        };

        files_searched += 1;
        let lines: Vec<&str> = content.lines().collect();

        // ── count_only mode ──────────────────────────────────────────────────
        if opts.count_only {
            let count = lines.iter().filter(|&&l| line_matches(&all_res, l, opts.invert_match)).count();
            file_counts.push(FileCount { path: rel, count });
            if file_counts.len() >= cap {
                truncated = true;
                break 'files;
            }
            continue;
        }

        // ── files_with_matches / files_without_match mode ────────────────────
        if opts.files_with_matches || opts.files_without_match {
            let has = lines.iter().any(|&l| line_matches(&all_res, l, opts.invert_match));
            if opts.files_with_matches && has {
                files_with_m.push(rel.clone());
                if files_with_m.len() >= cap { truncated = true; break 'files; }
            }
            if opts.files_without_match && !has {
                files_without_m.push(rel);
                if files_without_m.len() >= cap { truncated = true; break 'files; }
            }
            continue;
        }

        // ── normal match mode ────────────────────────────────────────────────
        for (idx, &line) in lines.iter().enumerate() {
            if !line_matches(&all_res, line, opts.invert_match) {
                continue;
            }

            // Collect all match spans once — used for both only_matching text and ranges.
            let spans: Vec<_> = all_res.iter()
                .flat_map(|re| re.find_iter(line))
                .collect();

            let matched_texts: Vec<String> = if opts.only_matching {
                spans.iter().map(|m| m.as_str().to_string()).collect()
            } else {
                vec![]
            };

            let match_ranges: Vec<[usize; 2]> = spans.iter()
                .map(|m| [m.start(), m.end()])
                .collect();

            matches.push(ContentMatch {
                path: rel.clone(),
                line_number: idx + 1,
                line: if opts.only_matching { String::new() } else { line.to_string() },
                matched_texts,
                match_ranges,
                before_context: context_slice(&lines, idx, before_ctx, true),
                after_context: context_slice(&lines, idx, after_ctx, false),
            });

            if matches.len() >= cap {
                truncated = true;
                break 'files;
            }
        }
    }

    let total_matches = if opts.count_only {
        file_counts.iter().map(|fc| fc.count).sum()
    } else if opts.files_with_matches {
        files_with_m.len()
    } else if opts.files_without_match {
        files_without_m.len()
    } else {
        matches.len()
    };

    Ok(SearchResult {
        matches,
        total_matches,
        files_searched,
        truncated,
        files_with_matches: files_with_m,
        files_without_match: files_without_m,
        file_counts,
    })
}

// ---------------------------------------------------------------------------
// Core: find_files
// ---------------------------------------------------------------------------

/// Find files whose repo-relative path matches a glob pattern.
///
/// `pattern` supports `*`, `**`, and `?`. Patterns without `/` are matched
/// against the filename only. Noise and ignored files are excluded unless
/// `opts.no_ignore` is set.
pub fn find_files(
    root: &Path,
    pattern: &str,
    limit: usize,
    opts: &FindOptions,
) -> Result<FindResult, String> {
    if pattern.is_empty() {
        return Err("pattern must not be empty".into());
    }

    let glob_re = build_glob_regex(pattern)
        .ok_or_else(|| format!("invalid glob: {}", pattern))?;

    let cap = if limit == 0 { usize::MAX } else { limit };

    // Resolve newer_than mtime threshold
    let newer_than_time: Option<SystemTime> = opts.newer_than.as_ref().and_then(|np| {
        std::fs::metadata(root.join(np)).and_then(|m| m.modified()).ok()
    });

    let modified_since_threshold: Option<SystemTime> = opts
        .modified_since_secs
        .map(|s| SystemTime::now() - Duration::from_secs(s));

    let file_list = enumerate_files(root, opts.no_ignore)?;

    let mut files: Vec<FindFile> = Vec::new();
    let mut truncated = false;

    for abs_path in &file_list {
        let rel = rel_path(root, abs_path);

        // depth filter (count '/' in repo-relative path)
        if let Some(max_d) = opts.max_depth {
            if rel.matches('/').count() > max_d { continue; }
        }

        if !glob_re.is_match(&rel) { continue; }

        let meta = match std::fs::metadata(abs_path) {
            Ok(m) => m,
            Err(_) => continue,
        };
        let size = meta.len();

        if let Some(min) = opts.min_size_bytes { if size < min { continue; } }
        if let Some(max) = opts.max_size_bytes { if size > max { continue; } }

        let mtime = meta.modified().ok();

        if let Some(threshold) = modified_since_threshold {
            match mtime { Some(t) if t >= threshold => {}, _ => continue }
        }
        if let Some(newer) = newer_than_time {
            match mtime { Some(t) if t > newer => {}, _ => continue }
        }

        let modified = mtime.map(|t| {
            let secs = t.duration_since(SystemTime::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
            format_unix_ts(secs)
        });

        files.push(FindFile {
            language: detect_language(&rel),
            path: rel,
            size_bytes: size,
            modified,
        });

        if files.len() >= cap { truncated = true; break; }
    }

    let total_matches = files.len();
    Ok(FindResult { files, total_matches, truncated })
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn line_matches(regexes: &[Regex], line: &str, invert: bool) -> bool {
    let hit = regexes.iter().any(|re| re.is_match(line));
    if invert { !hit } else { hit }
}

fn build_re(pattern: &str, literal: bool, word: bool, case_sensitive: bool) -> Result<Regex, String> {
    let mut pat = if literal { regex::escape(pattern) } else { pattern.to_string() };
    if word { pat = format!(r"\b{}\b", pat); }
    RegexBuilder::new(&pat)
        .case_insensitive(!case_sensitive)
        .build()
        .map_err(|e| format!("invalid pattern {:?}: {}", pattern, e))
}

/// Walk files respecting the noise filter (default) or raw walkdir (no_ignore).
fn enumerate_files(root: &Path, no_ignore: bool) -> Result<Vec<PathBuf>, String> {
    if no_ignore {
        use walkdir::WalkDir;
        let mut files = Vec::new();
        for entry in WalkDir::new(root).follow_links(false).into_iter().filter_map(|e| e.ok()) {
            if entry.file_type().is_file() {
                files.push(entry.into_path());
            }
        }
        Ok(files)
    } else {
        let scan = scan_files_with_noise_tracking(root).map_err(|e| e.to_string())?;
        let files = scan.files.into_iter().filter(|p| !is_ignored_path(p)).collect();
        Ok(files)
    }
}

fn rel_path(root: &Path, abs: &Path) -> String {
    abs.strip_prefix(root)
        .unwrap_or(abs)
        .to_string_lossy()
        .replace('\\', "/")
}

fn context_slice(lines: &[&str], match_idx: usize, n: usize, before: bool) -> Vec<ContextLine> {
    if n == 0 { return vec![]; }
    if before {
        let start = match_idx.saturating_sub(n);
        lines[start..match_idx]
            .iter()
            .enumerate()
            .map(|(j, l)| ContextLine { line_number: start + j + 1, line: l.to_string() })
            .collect()
    } else {
        let end = (match_idx + 1 + n).min(lines.len());
        lines[match_idx + 1..end]
            .iter()
            .enumerate()
            .map(|(j, l)| ContextLine { line_number: match_idx + 2 + j, line: l.to_string() })
            .collect()
    }
}

/// Convert a glob pattern to an anchored regex for matching repo-relative paths.
///
/// - `*.rs`        → match filename anywhere in tree  (`(^|/)[^/]*\.rs$`)
/// - `src/**/*.ts` → anchor to path root              (`^src/.*[^/]*\.ts$`)
pub fn build_glob_regex(pattern: &str) -> Option<Regex> {
    let has_sep = pattern.contains('/');
    let mut out = if has_sep { String::from("^") } else { String::from("(^|/)") };
    let chars: Vec<char> = pattern.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        match chars[i] {
            '*' if i + 1 < chars.len() && chars[i + 1] == '*' => {
                out.push_str(".*");
                i += 2;
                if i < chars.len() && chars[i] == '/' { i += 1; }
            }
            '*' => { out.push_str("[^/]*"); i += 1; }
            '?' => { out.push_str("[^/]"); i += 1; }
            c @ ('.' | '+' | '^' | '$' | '{' | '}' | '(' | ')' | '|' | '[' | ']' | '\\') => {
                out.push('\\'); out.push(c); i += 1;
            }
            c => { out.push(c); i += 1; }
        }
    }
    out.push('$');
    Regex::new(&out).ok()
}

/// Detect language from file extension.
pub fn detect_language(path: &str) -> Option<String> {
    let ext = path.rsplit('.').next()?;
    let lang = match ext {
        "rs"                          => "Rust",
        "go"                          => "Go",
        "py"                          => "Python",
        "ts" | "tsx"                  => "TypeScript",
        "js" | "jsx" | "mjs" | "cjs" => "JavaScript",
        "java"                        => "Java",
        "kt" | "kts"                  => "Kotlin",
        "swift"                       => "Swift",
        "c" | "h"                     => "C",
        "cpp" | "cc" | "cxx" | "hpp" => "C++",
        "cs"                          => "C#",
        "rb"                          => "Ruby",
        "php"                         => "PHP",
        "dart"                        => "Dart",
        "scala"                       => "Scala",
        "ex" | "exs"                  => "Elixir",
        "hs"                          => "Haskell",
        "ml" | "mli"                  => "OCaml",
        "clj" | "cljs"                => "Clojure",
        "sh" | "bash" | "zsh"         => "Shell",
        "lua"                         => "Lua",
        "r" | "R"                     => "R",
        "jl"                          => "Julia",
        "sql"                         => "SQL",
        "toml"                        => "TOML",
        "json"                        => "JSON",
        "yaml" | "yml"                => "YAML",
        "md"                          => "Markdown",
        "html" | "htm"                => "HTML",
        "css" | "scss" | "less"       => "CSS",
        "xml"                         => "XML",
        "tf"                          => "Terraform",
        "proto"                       => "Protobuf",
        "graphql" | "gql"             => "GraphQL",
        _                             => return None,
    };
    Some(lang.to_string())
}

/// Format a Unix timestamp as `YYYY-MM-DDTHH:MM:SSZ` without pulling in chrono.
fn format_unix_ts(secs: u64) -> String {
    let s = secs % 60;
    let m = (secs / 60) % 60;
    let h = (secs / 3600) % 24;
    let days = secs / 86400;
    let (y, mo, d) = days_to_ymd(days);
    format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z", y, mo, d, h, m, s)
}

/// Gregorian calendar: days since Unix epoch → (year, month, day).
fn days_to_ymd(mut days: u64) -> (u64, u64, u64) {
    days += 719468;
    let era = days / 146097;
    let doe = days % 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y   = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp  = (5 * doy + 2) / 153;
    let d   = doy - (153 * mp + 2) / 5 + 1;
    let mo  = if mp < 10 { mp + 3 } else { mp - 9 };
    let y   = if mo <= 2 { y + 1 } else { y };
    (y, mo, d)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glob_filename_only() {
        let re = build_glob_regex("*.rs").unwrap();
        assert!(re.is_match("src/api.rs"));
        assert!(re.is_match("api.rs"));
        assert!(!re.is_match("src/api.ts"));
    }

    #[test]
    fn glob_path_anchored() {
        let re = build_glob_regex("src/**/*.ts").unwrap();
        assert!(re.is_match("src/components/button.ts"));
        assert!(re.is_match("src/button.ts"));
        assert!(!re.is_match("lib/button.ts"));
    }

    #[test]
    fn glob_exact_dir() {
        let re = build_glob_regex("src/*.rs").unwrap();
        assert!(re.is_match("src/api.rs"));
        assert!(!re.is_match("src/sub/api.rs"));
    }

    #[test]
    fn word_regexp_wraps() {
        let re = build_re("fn", false, true, true).unwrap();
        assert!(re.is_match("pub fn foo()"));
        assert!(!re.is_match("foo_fn_bar")); // not word-boundary
    }

    #[test]
    fn extra_patterns_or() {
        let res = vec![
            build_re("TODO", false, false, false).unwrap(),
            build_re("FIXME", false, false, false).unwrap(),
        ];
        assert!(line_matches(&res, "// TODO: refactor", false));
        assert!(line_matches(&res, "// FIXME: broken", false));
        assert!(!line_matches(&res, "// just a comment", false));
    }

    #[test]
    fn invert_match() {
        let res = vec![build_re("test", false, false, false).unwrap()];
        assert!(!line_matches(&res, "fn test_foo()", true));
        assert!(line_matches(&res, "fn production()", true));
    }

    #[test]
    fn format_unix_ts_known() {
        // 2024-01-15T00:00:00Z = 1705276800
        let s = format_unix_ts(1705276800);
        assert_eq!(s, "2024-01-15T00:00:00Z");
    }
}
