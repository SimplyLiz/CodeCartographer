//! Content search — grep-like text/regex search across project files.
//!
//! Reuses the existing file scanner so `.cartographerignore`, noise filters,
//! and security blocks are all respected automatically.

use crate::scanner::{is_ignored_path, scan_files_with_noise_tracking};
use regex::{Regex, RegexBuilder};
use serde::{Deserialize, Serialize};
use std::path::Path;

// ---------------------------------------------------------------------------
// Options
// ---------------------------------------------------------------------------

/// Options for a content search request.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchOptions {
    /// Treat `pattern` as a literal string (escape regex metacharacters).
    #[serde(default)]
    pub literal: bool,
    /// Case-sensitive matching — default `true`.
    #[serde(default = "default_true")]
    pub case_sensitive: bool,
    /// Lines of context to include before and after each match (like `grep -C`).
    #[serde(default)]
    pub context_lines: usize,
    /// Cap on returned matches. `0` = unlimited. Default 100.
    #[serde(default = "default_max")]
    pub max_results: usize,
    /// Optional glob to restrict which files are searched, e.g. `"*.rs"` or
    /// `"src/**/*.ts"`. Patterns without `/` are matched against the filename
    /// only; patterns with `/` are anchored to the repo-relative path.
    #[serde(default)]
    pub file_glob: Option<String>,
}

fn default_true() -> bool {
    true
}
fn default_max() -> usize {
    100
}

impl Default for SearchOptions {
    fn default() -> Self {
        Self {
            literal: false,
            case_sensitive: true,
            context_lines: 0,
            max_results: 100,
            file_glob: None,
        }
    }
}

// ---------------------------------------------------------------------------
// Result types
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
    pub line: String,
    pub before_context: Vec<ContextLine>,
    pub after_context: Vec<ContextLine>,
}

/// Aggregated result returned by [`search_content`].
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResult {
    pub matches: Vec<ContentMatch>,
    /// Number of matches returned (≤ `max_results` if truncated).
    pub total_matches: usize,
    /// Number of files that were actually read and searched.
    pub files_searched: usize,
    /// `true` when the result was capped at `max_results`.
    pub truncated: bool,
}

// ---------------------------------------------------------------------------
// Core search function
// ---------------------------------------------------------------------------

/// Search for `pattern` across all non-noise, non-ignored files under `root`.
pub fn search_content(
    root: &Path,
    pattern: &str,
    opts: &SearchOptions,
) -> Result<SearchResult, String> {
    if pattern.is_empty() {
        return Err("pattern must not be empty".into());
    }

    // Build regex — escape metacharacters for literal searches
    let pat = if opts.literal {
        regex::escape(pattern)
    } else {
        pattern.to_string()
    };

    let re = RegexBuilder::new(&pat)
        .case_insensitive(!opts.case_sensitive)
        .build()
        .map_err(|e| format!("invalid pattern: {}", e))?;

    // Optional file-glob filter compiled once up front
    let glob_filter: Option<Regex> = opts.file_glob.as_deref().and_then(build_glob_regex);

    // Enumerate files through the existing scanner (noise + security filters apply)
    let scan = scan_files_with_noise_tracking(root).map_err(|e| e.to_string())?;

    let cap = if opts.max_results == 0 {
        usize::MAX
    } else {
        opts.max_results
    };

    let mut results: Vec<ContentMatch> = Vec::new();
    let mut files_searched: usize = 0;
    let mut truncated = false;

    'outer: for abs_path in &scan.files {
        if is_ignored_path(abs_path) {
            continue;
        }

        // Repo-relative path with forward slashes
        let rel = abs_path
            .strip_prefix(root)
            .unwrap_or(abs_path)
            .to_string_lossy()
            .replace('\\', "/");

        if let Some(ref gre) = glob_filter {
            if !gre.is_match(&rel) {
                continue;
            }
        }

        // Read as UTF-8 — silently skip binary/unreadable files
        let content = match std::fs::read_to_string(abs_path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        files_searched += 1;

        let lines: Vec<&str> = content.lines().collect();

        for (idx, &line) in lines.iter().enumerate() {
            if !re.is_match(line) {
                continue;
            }

            results.push(ContentMatch {
                path: rel.clone(),
                line_number: idx + 1,
                line: line.to_string(),
                before_context: context_slice(&lines, idx, opts.context_lines, true),
                after_context: context_slice(&lines, idx, opts.context_lines, false),
            });

            if results.len() >= cap {
                truncated = true;
                break 'outer;
            }
        }
    }

    let total_matches = results.len();
    Ok(SearchResult {
        matches: results,
        total_matches,
        files_searched,
        truncated,
    })
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn context_slice(lines: &[&str], match_idx: usize, n: usize, before: bool) -> Vec<ContextLine> {
    if n == 0 {
        return vec![];
    }
    if before {
        let start = match_idx.saturating_sub(n);
        lines[start..match_idx]
            .iter()
            .enumerate()
            .map(|(j, l)| ContextLine {
                line_number: start + j + 1,
                line: l.to_string(),
            })
            .collect()
    } else {
        let end = (match_idx + 1 + n).min(lines.len());
        lines[match_idx + 1..end]
            .iter()
            .enumerate()
            .map(|(j, l)| ContextLine {
                line_number: match_idx + 2 + j,
                line: l.to_string(),
            })
            .collect()
    }
}

/// Convert a glob pattern to an anchored regex for matching repo-relative paths.
///
/// - `*.rs`           → match filename anywhere in tree  (`(^|/)[^/]*\.rs$`)
/// - `src/**/*.ts`    → anchor to path root              (`^src/.*[^/]*\.ts$`)
fn build_glob_regex(pattern: &str) -> Option<Regex> {
    let has_sep = pattern.contains('/');
    let mut out = if has_sep {
        String::from("^")
    } else {
        String::from("(^|/)")
    };

    let chars: Vec<char> = pattern.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        match chars[i] {
            '*' if i + 1 < chars.len() && chars[i + 1] == '*' => {
                out.push_str(".*");
                i += 2;
                if i < chars.len() && chars[i] == '/' {
                    i += 1; // consume trailing separator after **
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
            c @ ('.' | '+' | '^' | '$' | '{' | '}' | '(' | ')' | '|' | '[' | ']' | '\\') => {
                out.push('\\');
                out.push(c);
                i += 1;
            }
            c => {
                out.push(c);
                i += 1;
            }
        }
    }
    out.push('$');
    Regex::new(&out).ok()
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
        assert!(!re.is_match("src/sub/api.rs")); // single * doesn't cross /
    }
}
