//! Git history analysis — co-change coupling, churn, and semantic diff helpers.
//! All functions fail gracefully (empty results) when git is unavailable or the
//! directory is not a repository.
//!
//! Bot commits and formatting-only commits are filtered by default because they
//! inflate churn and coupling metrics without representing real work.
//! (Research: ~74% of "hotspot" commits in practice come from bots or formatters.)

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
    /// count / (min(churn_a, churn_b) + COUPLING_PRIOR): Tornhill's coupling
    /// ratio, shrunk toward 0 for pairs with little support. See
    /// [`coupling_score`].
    pub coupling_score: f64,
}

// ---------------------------------------------------------------------------
// Noise-filter helpers
// ---------------------------------------------------------------------------

/// Returns true for known bot/automation author name patterns.
fn is_bot_author(author: &str) -> bool {
    let lower = author.to_lowercase();
    lower.contains("[bot]")
        || lower.contains("dependabot")
        || lower.contains("renovate")
        || lower.contains("github-actions")
        || lower.contains("snyk-bot")
        || lower.contains("greenkeeper")
        || lower.contains("semantic-release")
        || lower.contains("auto-merge")
        || lower.contains("release-bot")
        || lower.contains("ci-bot")
}

/// Returns true for commit subjects that look like formatting/lint-only passes.
fn is_formatting_subject(subject: &str) -> bool {
    let lower = subject.to_lowercase();
    // Common formatting commit patterns
    let patterns = [
        "apply prettier",
        "run prettier",
        "prettier format",
        "format code",
        "fix formatting",
        "auto format",
        "lint fix",
        "eslint fix",
        "fix lint",
        "apply lint",
        "rustfmt",
        "cargo fmt",
        "gofmt",
        "black format",
        "isort",
        "trailing whitespace",
        "fix whitespace",
        "whitespace fix",
        "normalize line endings",
        "editorconfig",
    ];
    patterns.iter().any(|p| lower.contains(p))
}

// ---------------------------------------------------------------------------
// Parse the log format we use for both churn and cochange:
//   --format=%x1f%an%x1f%s
//
// Each commit emits one line: \x1f<author>\x1f<subject>
// followed by the (--name-only) file list, followed by a blank line.
// A line is a commit header if it starts with \x1f.
// ---------------------------------------------------------------------------

struct CommitHeader {
    skip: bool, // bot author or formatting subject
}

fn parse_header(line: &str) -> Option<CommitHeader> {
    if !line.starts_with('\x1f') {
        return None;
    }
    let parts: Vec<&str> = line.splitn(3, '\x1f').collect();
    // parts[0] = "" (before first \x1f), parts[1] = author, parts[2] = subject
    let author = parts.get(1).copied().unwrap_or("").trim();
    let subject = parts.get(2).copied().unwrap_or("").trim();
    let skip = is_bot_author(author) || is_formatting_subject(subject);
    Some(CommitHeader { skip })
}

/// True for repo paths that only add noise to churn/coupling metrics: lockfiles,
/// images/binaries/PDFs and other non-text assets, minified bundles, logs,
/// sourcemaps, and anything the scanner already ignores (`.DS_Store`, vendored
/// dirs, secrets). Source, config, and docs are kept — they carry real signal.
fn is_noise_path(path: &str) -> bool {
    use crate::scanner;
    let p = Path::new(path);
    if scanner::is_ignored_path(p) {
        return true;
    }
    let base = path.rsplit('/').next().unwrap_or(path);
    if scanner::NOISE_LOCK_FILES.contains(&base) {
        return true;
    }
    let ext = path.rsplit('.').next().unwrap_or("").to_lowercase();
    let ext = ext.as_str();
    if scanner::NOISE_BINARY_EXTENSIONS.contains(&ext)
        || scanner::NOISE_LOG_EXTENSIONS.contains(&ext)
        || scanner::NOISE_MAP_EXTENSIONS.contains(&ext)
        || matches!(
            ext,
            "pdf" | "lock" | "zip" | "gz" | "tar" | "tgz" | "svg" | "woff" | "woff2"
                | "ttf" | "eot" | "otf" | "mp4" | "mov" | "webm" | "wasm" | "class" | "o"
        )
    {
        return true;
    }
    path.ends_with(scanner::NOISE_MINIFIED_JS_SUFFIX)
        || path.ends_with(scanner::NOISE_MINIFIED_CSS_SUFFIX)
}

/// Normalise a caller-supplied commit limit: 0 (the tool default) means
/// "recent history", which we cap at 500 to bound cost — matching the tool
/// docs' documented `0 → 500`.
fn effective_limit(limit: usize) -> usize {
    if limit == 0 {
        500
    } else {
        limit
    }
}

// ---------------------------------------------------------------------------
// git_churn
// ---------------------------------------------------------------------------

/// Return the number of commits that touched each file over the last `limit`
/// commits, relative paths from the repo root.
///
/// Bot and formatting-only commits are excluded.
/// Returns an empty map if git is unavailable or the directory is not a repo.
pub fn git_churn(root: &Path, limit: usize) -> HashMap<String, usize> {
    let limit = effective_limit(limit);
    let output = Command::new("git")
        .args([
            "-C",
            &root.to_string_lossy(),
            "log",
            &format!("-n {}", limit),
            "--name-only",
            "--format=%x1f%an%x1f%s", // \x1f<author>\x1f<subject>
        ])
        .output();

    let output = match output {
        Ok(o) if o.status.success() => o,
        _ => return HashMap::new(),
    };

    let text = String::from_utf8_lossy(&output.stdout);
    let mut churn: HashMap<String, usize> = HashMap::new();
    let mut skip_current = false;

    for line in text.lines() {
        let line = line.trim();
        if let Some(header) = parse_header(line) {
            skip_current = header.skip;
            continue;
        }
        if line.is_empty() || skip_current || is_noise_path(line) {
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
/// Bot and formatting-only commits are excluded.
///
/// Score: [`coupling_score`] (Tornhill's ratio with a support prior); ties
/// broken by the raw co-change count.
pub fn git_cochange(root: &Path, limit: usize) -> Vec<CoChangePair> {
    let limit = effective_limit(limit);
    let output = Command::new("git")
        .args([
            "-C",
            &root.to_string_lossy(),
            "log",
            &format!("-n {}", limit),
            "--name-only",
            "--format=%x1f%an%x1f%s",
        ])
        .output();

    let output = match output {
        Ok(o) if o.status.success() => o,
        _ => return vec![],
    };

    let text = String::from_utf8_lossy(&output.stdout);

    // Build per-commit file sets (filtered).
    let mut commits: Vec<Vec<String>> = Vec::new();
    let mut current: Vec<String> = Vec::new();
    let mut skip_current = false;

    for line in text.lines() {
        let line = line.trim();
        if let Some(header) = parse_header(line) {
            // Flush previous commit
            if !current.is_empty() {
                commits.push(std::mem::take(&mut current));
            }
            skip_current = header.skip;
            continue;
        }
        if line.is_empty() {
            continue;
        }
        if !skip_current && !is_noise_path(line) {
            current.push(line.to_string());
        }
    }
    if !current.is_empty() {
        commits.push(current);
    }

    // Drop mechanical mega-commits (mass reformats, license-header sweeps, large
    // merges). They touch hundreds/thousands of files and carry no meaningful
    // coupling signal, yet they dominate cost: one N-file commit yields
    // N*(N-1)/2 pairs, so a few thousand-file commits explode into tens of
    // millions of pairs and the analysis never completes on a large repo.
    const MAX_FILES_PER_COMMIT: usize = 50;
    commits.retain(|files| files.len() <= MAX_FILES_PER_COMMIT);

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
            let coupling_score = coupling_score(count, ca, cb);
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
            .then(b.count.cmp(&a.count))
    });
    pairs
}

/// Pseudo-commits in which a pair is assumed *not* to have changed together.
/// See [`coupling_score`].
pub(crate) const COUPLING_PRIOR: f64 = 2.0;

/// Coupling of two files that changed together in `count` commits, out of
/// `churn_a` and `churn_b` commits each.
///
/// Tornhill's ratio `count / min(churn_a, churn_b)` says how often the less
/// active file changes with the other, but it ignores support: a file changed
/// twice, both times with X, scored 1.0 and outranked a partner that changed
/// with X in 14 of 16 commits. The ratio is now shrunk by a prior of
/// [`COUPLING_PRIOR`] commits without a co-change (the posterior mean under a
/// Beta(0, 2) prior): 2 of 2 scores 0.5, 5 of 5 0.71, 14 of 16 0.78,
/// 10 of 10 0.83. Well-supported pairs keep roughly their old value; the
/// score still reads "how reliably does the smaller file follow the other".
pub fn coupling_score(count: usize, churn_a: usize, churn_b: usize) -> f64 {
    let min_churn = churn_a.min(churn_b).max(count) as f64;
    count as f64 / (min_churn + COUPLING_PRIOR)
}

// ---------------------------------------------------------------------------
// Co-change dispersion / shotgun-surgery detection
// ---------------------------------------------------------------------------

/// Per-file co-change dispersion — how widely a file's changes scatter across
/// the codebase. High dispersion is the shotgun-surgery code smell:
/// one change triggers edits across many unrelated modules.
/// (arXiv:2504.18511 — Co-Change Graph Entropy for defect prediction)
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CoChangeDispersion {
    pub file: String,
    /// Number of distinct files this file has co-changed with.
    pub partner_count: usize,
    /// Sum of co-change counts across all partners.
    pub total_cochanges: usize,
    /// Shannon entropy: −Σ p_i·log₂(p_i). Higher = more evenly spread across partners.
    pub entropy: f64,
    /// partner_count normalised to 0–100 across all files in the project.
    pub dispersion_score: f64,
}

/// Compute co-change dispersion for every file that appears in the co-change graph.
///
/// Reuses the existing co-change pairs — no additional git call.
pub fn git_cochange_dispersion(root: &Path, limit: usize) -> Vec<CoChangeDispersion> {
    dispersion_from_pairs(&git_cochange(root, limit))
}

/// Dispersion from already computed co-change pairs (lets the FFI reuse its
/// cached pairs instead of running `git log` again).
///
/// Only source files count, as a file and as a partner. A changelog, a lock
/// file or a generated migration journal changes alongside everything, so it
/// topped the ranking and inflated every code file's partner count.
pub fn dispersion_from_pairs(pairs: &[CoChangePair]) -> Vec<CoChangeDispersion> {
    let is_code = |f: &str| crate::scanner::is_source_file(Path::new(f));

    // Build per-file partner maps: file → { partner → count }
    let mut partner_counts: HashMap<String, HashMap<String, usize>> = HashMap::new();
    for p in pairs.iter().filter(|p| is_code(&p.file_a) && is_code(&p.file_b)) {
        *partner_counts
            .entry(p.file_a.clone())
            .or_default()
            .entry(p.file_b.clone())
            .or_insert(0) += p.count;
        *partner_counts
            .entry(p.file_b.clone())
            .or_default()
            .entry(p.file_a.clone())
            .or_insert(0) += p.count;
    }

    if partner_counts.is_empty() {
        return vec![];
    }
    let max_partners = partner_counts.values().map(|m| m.len()).max().unwrap_or(1).max(1) as f64;

    let mut result: Vec<CoChangeDispersion> = partner_counts
        .into_iter()
        .map(|(file, partners)| {
            let partner_count = partners.len();
            let total_cochanges: usize = partners.values().sum();
            let total = total_cochanges as f64;

            // Shannon entropy
            let entropy = if total > 0.0 {
                partners
                    .values()
                    .filter(|&&c| c > 0)
                    .map(|&c| {
                        let p = c as f64 / total;
                        -p * p.log2()
                    })
                    .sum::<f64>()
            } else {
                0.0
            };

            let dispersion_score = (partner_count as f64 / max_partners * 100.0).round();

            CoChangeDispersion {
                file,
                partner_count,
                total_cochanges,
                entropy,
                dispersion_score,
            }
        })
        .collect();

    result.sort_by(|a, b| {
        b.dispersion_score
            .partial_cmp(&a.dispersion_score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    result
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

// ---------------------------------------------------------------------------
// git_ownership
// ---------------------------------------------------------------------------

/// Dominant author per file over the last `limit` commits. "Dominant" =
/// highest raw commit count; ties broken alphabetically. Bot authors and
/// formatting-only commits are excluded (same filters as churn/cochange).
///
/// Returns an empty map if git is unavailable or the directory is not a repo.
/// Keys are repo-relative paths matching `git log --name-only` output.
#[allow(dead_code)]
pub fn git_ownership(root: &Path, limit: usize) -> HashMap<String, String> {
    let output = Command::new("git")
        .args([
            "-C",
            &root.to_string_lossy(),
            "log",
            &format!("-n {}", limit),
            "--name-only",
            "--format=%x1f%an%x1f%s",
        ])
        .output();

    let output = match output {
        Ok(o) if o.status.success() => o,
        _ => return HashMap::new(),
    };

    let text = String::from_utf8_lossy(&output.stdout);

    // Per-file per-author commit counts. We need the raw author name here
    // (not just the skip flag) so we parse the header locally instead of
    // reusing `parse_header`.
    let mut counts: HashMap<String, HashMap<String, usize>> = HashMap::new();
    let mut current_author: Option<String> = None;
    let mut skip_current = false;

    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('\x1f') {
            let parts: Vec<&str> = line.splitn(3, '\x1f').collect();
            let author = parts.get(1).copied().unwrap_or("").trim().to_string();
            let subject = parts.get(2).copied().unwrap_or("").trim();
            skip_current = is_bot_author(&author) || is_formatting_subject(subject);
            current_author = if skip_current { None } else { Some(author) };
            continue;
        }
        if line.is_empty() || skip_current {
            continue;
        }
        if let Some(ref author) = current_author {
            *counts
                .entry(line.to_string())
                .or_default()
                .entry(author.clone())
                .or_insert(0) += 1;
        }
    }

    let mut owners: HashMap<String, String> = HashMap::with_capacity(counts.len());
    for (file, authors) in counts {
        // Pick the author with the highest count; ties → alphabetical so the
        // result is deterministic across runs.
        let dominant = authors
            .into_iter()
            .max_by(|a, b| a.1.cmp(&b.1).then_with(|| b.0.cmp(&a.0)));
        if let Some((name, _)) = dominant {
            owners.insert(file, name);
        }
    }
    owners
}

/// Return the number of distinct human authors per file (bus factor proxy).
/// A file touched by only one author has bus_factor = 1 — highest risk.
#[allow(dead_code)]
pub fn git_bus_factor(root: &Path, limit: usize) -> HashMap<String, usize> {
    let output = Command::new("git")
        .args([
            "-C",
            &root.to_string_lossy(),
            "log",
            &format!("-n {}", limit),
            "--name-only",
            "--format=%x1f%an%x1f%s",
        ])
        .output();

    let output = match output {
        Ok(o) if o.status.success() => o,
        _ => return HashMap::new(),
    };

    let text = String::from_utf8_lossy(&output.stdout);

    let mut file_authors: HashMap<String, std::collections::HashSet<String>> = HashMap::new();
    let mut current_author: Option<String> = None;
    let mut skip_current = false;

    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('\x1f') {
            let parts: Vec<&str> = line.splitn(3, '\x1f').collect();
            let author = parts.get(1).copied().unwrap_or("").trim().to_string();
            let subject = parts.get(2).copied().unwrap_or("").trim();
            skip_current = is_bot_author(&author) || is_formatting_subject(subject);
            current_author = if skip_current { None } else { Some(author) };
            continue;
        }
        if line.is_empty() || skip_current {
            continue;
        }
        if let Some(ref author) = current_author {
            file_authors
                .entry(line.to_string())
                .or_default()
                .insert(author.clone());
        }
    }

    file_authors
        .into_iter()
        .map(|(file, authors)| (file, authors.len()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dispersion_leaves_out_non_code_files() {
        let pair = |a: &str, b: &str| CoChangePair {
            file_a: a.into(),
            file_b: b.into(),
            count: 3,
            coupling_score: 0.5,
        };
        let pairs = vec![
            pair("CHANGELOG.md", "src/a.rs"),
            pair("CHANGELOG.md", "src/b.rs"),
            pair("drizzle/meta/_journal.json", "src/a.rs"),
            pair("src/a.rs", "src/b.rs"),
        ];
        let d = dispersion_from_pairs(&pairs);
        let files: Vec<&str> = d.iter().map(|e| e.file.as_str()).collect();
        assert_eq!(files.len(), 2, "{files:?}");
        assert!(files.iter().all(|f| f.starts_with("src/")), "{files:?}");
        assert!(d.iter().all(|e| e.partner_count == 1));
    }

    #[test]
    fn noise_paths_are_filtered_but_source_config_docs_kept() {
        // Noise: lockfiles, images, PDFs, minified bundles, sourcemaps, ignored.
        for p in [
            "Cargo.lock",
            "madison-desktop/Cargo.lock",
            "package-lock.json",
            "docs/marketing/pitch/cinematic-orb.png",
            "docs/lisa/Claude.pdf",
            "ui/vendor/app.min.js",
            "ui/app.min.css",
            "dist/bundle.js.map",
            ".DS_Store",
        ] {
            assert!(is_noise_path(p), "expected noise: {p}");
        }
        // Kept: real source, config, and docs carry coupling signal.
        for p in [
            "src/action.rs",
            "Cargo.toml",
            "stack/docker-compose.yml",
            "config/entity.toml",
            "README.md",
            "docs/mcc/05-roadmap.md",
        ] {
            assert!(!is_noise_path(p), "expected kept: {p}");
        }
    }

    /// The case that broke CKB's co-change list: a file changed twice, both
    /// times with X, scored 1.0 and outranked a partner with 14 co-changes.
    #[test]
    fn rare_file_does_not_outrank_strong_partner() {
        let rare = coupling_score(2, 2, 60); // 2 of its 2 commits with X
        let strong = coupling_score(14, 16, 60); // 14 of its 16 commits with X
        assert!(strong > rare, "strong {strong} <= rare {rare}");
        assert!((rare - 0.5).abs() < 1e-9);
        // Well-supported, always-together pairs stay near the top of the scale.
        assert!(coupling_score(10, 10, 40) > 0.8);
        // Monotone in support at a fixed ratio.
        assert!(coupling_score(3, 3, 9) < coupling_score(6, 6, 9));
        assert!(coupling_score(0, 5, 5) == 0.0);
    }

    #[test]
    fn git_cochange_ranks_by_supported_coupling() {
        use std::process::Command;
        let root = std::env::temp_dir().join(format!("cc-cochange-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let git = |args: &[&str]| {
            assert!(Command::new("git").arg("-C").arg(&root).args(args).status().unwrap().success());
        };
        git(&["init", "-q"]);
        git(&["config", "user.email", "t@example.com"]);
        git(&["config", "user.name", "Tester"]);
        git(&["config", "commit.gpgsign", "false"]);
        let commit = |files: &[&str], i: usize| {
            for f in files {
                std::fs::write(root.join(f), format!("{i}\n")).unwrap();
            }
            git(&["add", "-A"]);
            git(&["commit", "-q", "-m", &format!("c{i}")]);
        };
        // x.rs + strong.rs together 14 times, strong.rs alone twice,
        // x.rs + rare.rs together twice (rare.rs never changes otherwise).
        let mut i = 0;
        for _ in 0..14 { i += 1; commit(&["x.rs", "strong.rs"], i); }
        for _ in 0..2 { i += 1; commit(&["strong.rs"], i); }
        for _ in 0..2 { i += 1; commit(&["x.rs", "rare.rs"], i); }

        let pairs = git_cochange(&root, 0);
        let pos = |b: &str| {
            pairs.iter()
                .position(|p| (p.file_a == "x.rs" && p.file_b == b) || (p.file_b == "x.rs" && p.file_a == b))
                .unwrap_or_else(|| panic!("pair x.rs/{b} missing: {pairs:?}"))
        };
        assert!(pos("strong.rs") < pos("rare.rs"), "{pairs:?}");
        let strong = &pairs[pos("strong.rs")];
        assert_eq!(strong.count, 14);
        assert!((strong.coupling_score - 14.0 / 18.0).abs() < 1e-9);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn effective_limit_maps_zero_to_default() {
        assert_eq!(effective_limit(0), 500);
        assert_eq!(effective_limit(50), 50);
    }
}
