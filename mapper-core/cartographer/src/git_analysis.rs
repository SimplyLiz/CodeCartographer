//! Git history analysis — co-change coupling, churn, and semantic diff helpers.
//! All functions fail gracefully (empty results) when git is unavailable or the
//! directory is not a repository.

use std::collections::HashMap;
use std::path::Path;
use std::process::Command;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CoChangePair {
    pub file_a: String,
    pub file_b: String,
    /// Number of commits where both files changed together.
    pub count: usize,
    /// count / min(churn_a, churn_b) — 1.0 means they always change together.
    pub coupling_score: f64,
}

// ---------------------------------------------------------------------------
// git_churn
// ---------------------------------------------------------------------------

/// Return the number of commits that touched each file over the last `limit`
/// commits, relative paths from the repo root.
///
/// Returns an empty map if git is unavailable or the directory is not a repo.
pub fn git_churn(root: &Path, limit: usize) -> HashMap<String, usize> {
    let output = Command::new("git")
        .args([
            "-C",
            &root.to_string_lossy(),
            "log",
            &format!("-n {}", limit),
            "--name-only",
            "--format=",  // empty format — only file names, blank-line-separated
        ])
        .output();

    let output = match output {
        Ok(o) if o.status.success() => o,
        _ => return HashMap::new(),
    };

    let text = String::from_utf8_lossy(&output.stdout);
    let mut churn: HashMap<String, usize> = HashMap::new();

    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        *churn.entry(line.to_string()).or_insert(0) += 1;
    }

    churn
}

// ---------------------------------------------------------------------------
// git_cochange
// ---------------------------------------------------------------------------

/// Analyse the last `limit` commits and return file pairs that changed together,
/// sorted descending by coupling_score.
///
/// Uses Adam Tornhill's coupling formula:
///   coupling = co_changes / min(churn_a, churn_b)
pub fn git_cochange(root: &Path, limit: usize) -> Vec<CoChangePair> {
    // Collect one entry per commit: list of changed files.
    let output = Command::new("git")
        .args([
            "-C",
            &root.to_string_lossy(),
            "log",
            &format!("-n {}", limit),
            "--name-only",
            "--format=%x00",  // NUL byte as commit separator
        ])
        .output();

    let output = match output {
        Ok(o) if o.status.success() => o,
        _ => return vec![],
    };

    let text = String::from_utf8_lossy(&output.stdout);

    // Build per-commit file sets.
    let mut commits: Vec<Vec<String>> = Vec::new();
    let mut current: Vec<String> = Vec::new();

    for line in text.lines() {
        let line = line.trim();
        if line == "\x00" || line.is_empty() {
            if !current.is_empty() {
                commits.push(std::mem::take(&mut current));
            }
        } else {
            current.push(line.to_string());
        }
    }
    if !current.is_empty() {
        commits.push(current);
    }

    // Build churn map.
    let mut churn: HashMap<String, usize> = HashMap::new();
    for files in &commits {
        for f in files {
            *churn.entry(f.clone()).or_insert(0) += 1;
        }
    }

    // Count co-changes for every pair.
    let mut pair_counts: HashMap<(String, String), usize> = HashMap::new();
    for files in &commits {
        if files.len() < 2 {
            continue;
        }
        for i in 0..files.len() {
            for j in (i + 1)..files.len() {
                let (a, b) = if files[i] <= files[j] {
                    (files[i].clone(), files[j].clone())
                } else {
                    (files[j].clone(), files[i].clone())
                };
                *pair_counts.entry((a, b)).or_insert(0) += 1;
            }
        }
    }

    // Convert to CoChangePair with coupling score.
    let mut pairs: Vec<CoChangePair> = pair_counts
        .into_iter()
        .map(|((a, b), count)| {
            let ca = *churn.get(&a).unwrap_or(&1);
            let cb = *churn.get(&b).unwrap_or(&1);
            let min_churn = ca.min(cb) as f64;
            let coupling_score = if min_churn > 0.0 {
                (count as f64 / min_churn).min(1.0)
            } else {
                0.0
            };
            CoChangePair {
                file_a: a,
                file_b: b,
                count,
                coupling_score,
            }
        })
        .collect();

    pairs.sort_by(|a, b| {
        b.coupling_score
            .partial_cmp(&a.coupling_score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    pairs
}

// ---------------------------------------------------------------------------
// git_show_file
// ---------------------------------------------------------------------------

/// Return the contents of `path` at `commit`, or None if unavailable.
pub fn git_show_file(root: &Path, commit: &str, path: &str) -> Option<String> {
    let spec = format!("{}:{}", commit, path);
    let output = Command::new("git")
        .args(["-C", &root.to_string_lossy(), "show", &spec])
        .output()
        .ok()?;

    if output.status.success() {
        String::from_utf8(output.stdout).ok()
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// git_diff_files
// ---------------------------------------------------------------------------

/// Return files that changed between `c1` and `c2`, with their status:
/// `'A'` = added, `'M'` = modified, `'D'` = deleted.
pub fn git_diff_files(root: &Path, c1: &str, c2: &str) -> Vec<(String, char)> {
    let output = Command::new("git")
        .args([
            "-C",
            &root.to_string_lossy(),
            "diff",
            "--name-status",
            c1,
            c2,
        ])
        .output();

    let output = match output {
        Ok(o) if o.status.success() => o,
        _ => return vec![],
    };

    let text = String::from_utf8_lossy(&output.stdout);
    let mut result = Vec::new();

    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let parts: Vec<&str> = line.splitn(2, '\t').collect();
        if parts.len() != 2 {
            continue;
        }
        let status = parts[0].chars().next().unwrap_or('M');
        let file = parts[1].to_string();
        result.push((file, status));
    }

    result
}
