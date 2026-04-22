// Add constants at the top
const MAX_PATTERN_LENGTH: usize = 2_000;
const MAX_REGEX_SIZE_LIMIT: usize = 50 * 1024 * 1024; // 50 MB
const REGEX_TIMEOUT_MILLIS: u64 = 5_000; // 5 seconds per file

fn build_re(pattern: &str, literal: bool, word: bool, case_sensitive: bool) -> Result<Regex, String> {
    // GUARD 1: Pattern length
    if pattern.len() > MAX_PATTERN_LENGTH {
        return Err(format!(
            "Pattern too long: {} bytes (max {})",
            pattern.len(),
            MAX_PATTERN_LENGTH
        ));
    }

    let mut pat = if literal { regex::escape(pattern) } else { pattern.to_string() };
    if word { pat = format!(r"\b{}\b", pat); }

    // GUARD 2: Use RegexBuilder with size limits
    RegexBuilder::new(&pat)
        .case_insensitive(!case_sensitive)
        .size_limit(MAX_REGEX_SIZE_LIMIT) // Prevent catastrophic backtracking
        .dfa_size_limit(MAX_REGEX_SIZE_LIMIT)
        .build()
        .map_err(|e| {
            eprintln!("[CARTOGRAPHER] Regex compilation failed: {}", e);
            format!("Invalid pattern: {}", e)
        })
}

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

    // ... rest of function ...
    // When matching lines, apply a timeout per file:

    let per_file: Vec<FileResult> = file_list
        .par_iter()
        .filter_map(|abs_path| {
            let rel = rel_path(root, abs_path);

            // ... existing filters ...

            let content = std::fs::read_to_string(abs_path).ok()?;
            let lines: Vec<&str> = content.lines().collect();

            // GUARD: Timeout per file (simple wall-clock check)
            let start = std::time::Instant::now();

            // ... process lines ...
            let mut file_matches: Vec<ContentMatch> = Vec::new();
            for (idx, &line) in lines.iter().enumerate() {
                // Check timeout every N lines
                if idx % 100 == 0 && start.elapsed().as_millis() > REGEX_TIMEOUT_MILLIS as u128 {
                    eprintln!(
                        "[CARTOGRAPHER] Regex timeout on file {}: {}ms exceeded",
                        rel,
                        REGEX_TIMEOUT_MILLIS
                    );
                    return Some(FileResult::Searched); // Give up on this file
                }

                if !line_matches(&all_res, line, opts.invert_match) { continue; }

                // ... rest of match processing ...
            }

            // ... continue ...
        })
        .collect();

    // ... rest of function ...
}
