//! `answer` — question-driven evidence chain for AI context injection.
//!
//! Given a natural-language question, assembles the minimum set of semantic
//! units that together answer it, ordered so they read like an explanation:
//! entry points first, then types, then internals. Each item is numbered and
//! annotated with its connection to adjacent items (`[calls #2]`, `[type used by #1]`).
//!
//! Pipeline:
//!   1. BM25 search across all files → ranked candidate files
//!   2. For each candidate file, score its public symbols against query terms
//!   3. Select the top-scoring symbols across all files, capped by item budget
//!   4. Order: structs/types before functions, entry points before internals
//!   5. Annotate inter-item connections via import and call-graph edges
//!   6. Decide body vs sig-only per item: show body for the single "core logic" item
//!   7. Render as numbered evidence chain

use std::collections::HashMap;
use std::path::Path;

use crate::formatter::estimate_tokens;
use crate::mapper::{MappedFile, Signature, SymbolKind};
use crate::search::{bm25_search, BM25Options};

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct AnswerOptions {
    /// Maximum tokens in the output. Default 8000.
    pub budget: usize,
    /// Maximum evidence items. Default 6.
    pub max_items: usize,
    /// Show body for the top-scoring item. Default true.
    pub show_top_body: bool,
}

impl Default for AnswerOptions {
    fn default() -> Self {
        Self {
            budget: 8000,
            max_items: 6,
            show_top_body: true,
        }
    }
}

#[derive(Debug, Clone)]
pub struct EvidenceItem {
    /// 1-based index in the final chain.
    pub index: usize,
    pub name: String,
    pub file: String,
    pub line: u32,
    pub kind: SymbolKind,
    pub sig: String,
    /// Function body — only populated for the "core logic" item.
    pub body: Option<String>,
    /// How this item relates to adjacent items in the chain.
    pub connection: Option<String>,
    /// Role label: "entry point", "core logic", "type", "caller", etc.
    pub role_note: Option<String>,
}

#[derive(Debug)]
pub struct AnswerResult {
    pub query: String,
    pub items: Vec<EvidenceItem>,
    pub tokens_used: usize,
    pub budget_hit: bool,
    /// Files searched during BM25 phase.
    pub files_searched: usize,
}

// ---------------------------------------------------------------------------
// build_answer
// ---------------------------------------------------------------------------

pub fn build_answer(
    root_path: &Path,
    mapped: &[MappedFile],
    query: &str,
    opts: &AnswerOptions,
) -> AnswerResult {
    // --- 1. BM25 search ---
    let bm25_opts = BM25Options {
        max_results: 30,
        ..Default::default()
    };
    let bm25 = match bm25_search(root_path, query, &bm25_opts) {
        Ok(r) => r,
        Err(_) => {
            return AnswerResult {
                query: query.to_string(),
                items: vec![],
                tokens_used: 0,
                budget_hit: false,
                files_searched: 0,
            };
        }
    };

    // Unique file count from BM25 results.
    let files_searched: usize = {
        let mut seen = std::collections::HashSet::new();
        for m in &bm25.matches { seen.insert(m.path.clone()); }
        seen.len()
    };

    // Collect unique candidate files ranked by BM25 score.
    let mut file_scores: HashMap<String, f64> = HashMap::new();
    for m in &bm25.matches {
        let entry = file_scores.entry(m.path.clone()).or_default();
        *entry += 1.0; // increment per match; BM25 already weights by relevance
    }

    let mut ranked_files: Vec<(String, f64)> = file_scores.into_iter().collect();
    ranked_files.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    ranked_files.truncate(10);

    // --- 2. Score symbols in candidate files ---
    let query_terms = tokenize_query(query);
    let mut scored: Vec<ScoredSymbol> = vec![];

    for (file, file_score) in &ranked_files {
        let mf = match mapped.iter().find(|m| m.path == *file) {
            Some(m) => m,
            None => continue,
        };
        for sig in &mf.signatures {
            let sym_score = score_symbol(sig, &query_terms, *file_score);
            if sym_score > 0.0 {
                scored.push(ScoredSymbol {
                    file: file.clone(),
                    sig: sig.clone(),
                    score: sym_score,
                });
            }
        }
    }

    // Sort by score descending.
    scored.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    scored.truncate(opts.max_items * 2); // keep extras for ordering pass

    // --- 3. Order for readability ---
    let ordered = order_for_reading(scored, opts.max_items);

    // --- 4. Build evidence items with connections ---
    let mut items: Vec<EvidenceItem> = build_evidence_items(root_path, mapped, &ordered, opts);

    // --- 5. Apply token budget ---
    let rendered = render_answer(&AnswerResult {
        query: query.to_string(),
        items: items.clone(),
        tokens_used: 0,
        budget_hit: false,
        files_searched,
    });
    let tokens = estimate_tokens(&rendered);

    let budget_hit = tokens > opts.budget;
    if budget_hit {
        // Trim bodies first, then items from the tail.
        for item in &mut items {
            item.body = None;
        }
        while items.len() > 1 {
            let t = estimate_tokens(&render_items(&items, query));
            if t <= opts.budget {
                break;
            }
            items.pop();
        }
    }

    let final_tokens = estimate_tokens(&render_items(&items, query));

    AnswerResult {
        query: query.to_string(),
        items,
        tokens_used: final_tokens,
        budget_hit,
        files_searched,
    }
}

// ---------------------------------------------------------------------------
// Symbol scoring
// ---------------------------------------------------------------------------

struct ScoredSymbol {
    file: String,
    sig: Signature,
    score: f64,
}

fn tokenize_query(query: &str) -> Vec<String> {
    query
        .split(|c: char| !c.is_alphanumeric() && c != '_')
        .filter(|t| t.len() >= 3)
        .map(|t| t.to_lowercase())
        .collect()
}

fn score_symbol(sig: &Signature, query_terms: &[String], file_score: f64) -> f64 {
    let name = sig
        .symbol_name
        .as_deref()
        .unwrap_or("")
        .to_lowercase();
    let raw = sig.raw.to_lowercase();
    let doc = sig
        .doc_comment
        .as_deref()
        .unwrap_or("")
        .to_lowercase();

    let mut score = 0.0;

    for term in query_terms {
        // Strong match: term appears in the symbol name itself.
        if name.contains(term.as_str()) {
            score += 3.0;
        }
        // Medium: term in the raw signature (parameters, return types).
        if raw.contains(term.as_str()) {
            score += 1.5;
        }
        // Weak: term in doc comment.
        if doc.contains(term.as_str()) {
            score += 0.5;
        }
    }

    // Boost public symbols over private ones — they're more likely to be the
    // interface an AI needs to understand.
    if sig.raw.trim_start().starts_with("pub") {
        score *= 1.4;
    }

    // Boost functions and methods over fields and constants — they're more
    // explanatory.
    match sig.kind {
        SymbolKind::Function | SymbolKind::Method | SymbolKind::Constructor => score *= 1.2,
        SymbolKind::Struct | SymbolKind::Class | SymbolKind::Interface => score *= 1.1,
        _ => {}
    }

    // Incorporate the BM25 file-level relevance.
    score * (1.0 + file_score * 0.1)
}

// ---------------------------------------------------------------------------
// Ordering for readability
// ---------------------------------------------------------------------------

/// Order symbols so types come before functions that use them, and entry
/// points come before internal helpers. The goal is to read like a guided tour.
fn order_for_reading(mut scored: Vec<ScoredSymbol>, max: usize) -> Vec<ScoredSymbol> {
    // Partition into: types first, then functions/methods, then rest.
    // Within each partition, keep score order.
    let mut types: Vec<ScoredSymbol> = vec![];
    let mut fns: Vec<ScoredSymbol> = vec![];
    let mut rest: Vec<ScoredSymbol> = vec![];

    for s in scored.drain(..) {
        match s.sig.kind {
            SymbolKind::Struct
            | SymbolKind::Class
            | SymbolKind::Interface
            | SymbolKind::Enum
            | SymbolKind::TypeAlias => types.push(s),
            SymbolKind::Function | SymbolKind::Method | SymbolKind::Constructor => fns.push(s),
            _ => rest.push(s),
        }
    }

    // Interleave: a type next to the function that introduces it reads better
    // than all types first. Strategy: highest-scored function goes first, then
    // its parameter types, then next function, etc. For simplicity, just
    // sort types by score and interleave every-other.
    let mut result: Vec<ScoredSymbol> = vec![];
    let mut fi = fns.into_iter().peekable();
    let mut ti = types.into_iter().peekable();

    // Lead with the top function (most likely the "core" item).
    if let Some(f) = fi.next() {
        result.push(f);
    }
    // Then alternate type → function → type → function…
    loop {
        let had_type = if let Some(t) = ti.next() {
            result.push(t);
            true
        } else {
            false
        };
        let had_fn = if let Some(f) = fi.next() {
            result.push(f);
            true
        } else {
            false
        };
        if !had_type && !had_fn {
            break;
        }
        if result.len() >= max * 2 {
            break;
        }
    }
    result.extend(rest);
    result.truncate(max);
    result
}

// ---------------------------------------------------------------------------
// Evidence item construction
// ---------------------------------------------------------------------------

fn build_evidence_items(
    root_path: &Path,
    mapped: &[MappedFile],
    ordered: &[ScoredSymbol],
    opts: &AnswerOptions,
) -> Vec<EvidenceItem> {
    let mut items: Vec<EvidenceItem> = ordered
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let role_note = role_note_for(&s.sig, i == 0);
            let body = if i == 0 && opts.show_top_body {
                read_function_body(root_path, &s.file, s.sig.line_start, 30)
            } else {
                None
            };
            EvidenceItem {
                index: i + 1,
                name: s
                    .sig
                    .symbol_name
                    .clone()
                    .or_else(|| s.sig.qualified_name.clone())
                    .unwrap_or_default(),
                file: s.file.clone(),
                line: s.sig.line_start as u32 + 1,
                kind: s.sig.kind,
                sig: s.sig.raw.clone(),
                body,
                connection: None, // filled in next pass
                role_note,
            }
        })
        .collect();

    // Annotate inter-item connections.
    annotate_connections(&mut items, mapped);

    items
}

fn role_note_for(sig: &Signature, is_top: bool) -> Option<String> {
    let raw = sig.raw.trim();
    if is_top {
        return Some("core logic".to_string());
    }
    match sig.kind {
        SymbolKind::Struct | SymbolKind::Class => Some("type".to_string()),
        SymbolKind::Enum => Some("enum".to_string()),
        SymbolKind::Interface => Some("interface/trait".to_string()),
        SymbolKind::Function | SymbolKind::Method => {
            if raw.starts_with("pub") {
                Some("entry point".to_string())
            } else {
                Some("helper".to_string())
            }
        }
        _ => None,
    }
}

/// Annotate each item with how it connects to others in the chain.
/// Uses import edges and name-matching as a simple proxy for call edges.
fn annotate_connections(items: &mut Vec<EvidenceItem>, mapped: &[MappedFile]) {
    // Build a map from name → index for quick lookup.
    let name_to_idx: HashMap<String, usize> = items
        .iter()
        .enumerate()
        .map(|(i, item)| (item.name.clone(), i))
        .collect();

    for i in 0..items.len() {
        let sig_raw = items[i].sig.clone();
        let file = items[i].file.clone();

        // Check if any other item's name appears in this item's signature
        // (parameter type, return type) or if this file imports another item's file.
        let mut refs: Vec<String> = vec![];

        for (j, other) in items.iter().enumerate() {
            if i == j {
                continue;
            }
            // Type reference: other item's name appears in this item's sig.
            if sig_raw.contains(&other.name) && !other.name.is_empty() {
                match other.kind {
                    SymbolKind::Struct | SymbolKind::Class | SymbolKind::Enum
                    | SymbolKind::Interface | SymbolKind::TypeAlias => {
                        refs.push(format!("[uses type #{}]", j + 1));
                    }
                    SymbolKind::Function | SymbolKind::Method => {
                        refs.push(format!("[calls #{}]", j + 1));
                    }
                    _ => {}
                }
            }

            // Import reference: this file imports the other item's file.
            if let Some(mf) = mapped.iter().find(|m| m.path == file) {
                let other_stem = other
                    .file
                    .rsplit('/')
                    .next()
                    .unwrap_or("")
                    .trim_end_matches(".rs")
                    .trim_end_matches(".py")
                    .trim_end_matches(".go")
                    .trim_end_matches(".ts");
                if other_stem.len() >= 3
                    && mf.imports.iter().any(|imp| imp.contains(other_stem))
                    && other.file != file
                {
                    if !refs.iter().any(|r| r.contains(&(j + 1).to_string())) {
                        refs.push(format!("[imports from #{}]", j + 1));
                    }
                }
            }
        }

        if !refs.is_empty() {
            items[i].connection = Some(refs.join(", "));
        }
    }
}

// ---------------------------------------------------------------------------
// Body extraction
// ---------------------------------------------------------------------------

fn read_function_body(
    root_path: &Path,
    file: &str,
    line_start: usize,
    max_lines: usize,
) -> Option<String> {
    let abs = root_path.join(file);
    let source = std::fs::read_to_string(&abs).ok()?;
    let lines: Vec<&str> = source.lines().collect();
    if line_start >= lines.len() {
        return None;
    }

    // Find the opening brace.
    let mut brace_start = line_start;
    for (i, line) in lines[line_start..].iter().enumerate() {
        if line.contains('{') {
            brace_start = line_start + i;
            break;
        }
        if i > 8 {
            break; // give up if no brace within 8 lines
        }
    }

    // Collect body lines up to the matching closing brace or max_lines.
    let mut depth = 0i32;
    let mut body_lines: Vec<&str> = vec![];
    let mut in_body = false;

    for line in &lines[brace_start..] {
        for ch in line.chars() {
            if ch == '{' {
                depth += 1;
                in_body = true;
            } else if ch == '}' {
                depth -= 1;
            }
        }
        if in_body {
            body_lines.push(line);
        }
        if in_body && depth == 0 {
            break;
        }
        if body_lines.len() >= max_lines {
            body_lines.push("    // … (truncated)");
            break;
        }
    }

    if body_lines.is_empty() {
        None
    } else {
        Some(body_lines.join("\n"))
    }
}

// ---------------------------------------------------------------------------
// Renderer
// ---------------------------------------------------------------------------

pub fn render_answer(result: &AnswerResult) -> String {
    render_items(&result.items, &result.query)
}

fn render_items(items: &[EvidenceItem], query: &str) -> String {
    let mut out = String::with_capacity(4096);

    out.push_str(&format!("Evidence for: \"{}\"\n\n", query));

    for item in items {
        let kind_label = kind_str(item.kind);
        let short_file = short_path(&item.file);
        let role = item
            .role_note
            .as_deref()
            .map(|r| format!("  [{}]", r))
            .unwrap_or_default();
        let conn = item
            .connection
            .as_deref()
            .map(|c| format!("  {}", c))
            .unwrap_or_default();

        out.push_str(&format!(
            "{}  {}  {}  {}:{}{}{}\n",
            item.index, item.name, kind_label, short_file, item.line, role, conn
        ));
        out.push_str(&format!("   {}\n", item.sig.trim()));

        if let Some(ref body) = item.body {
            out.push_str(body);
            out.push('\n');
        }

        out.push('\n');
    }

    out
}

fn kind_str(kind: SymbolKind) -> &'static str {
    match kind {
        SymbolKind::Function | SymbolKind::Method | SymbolKind::Constructor => "fn",
        SymbolKind::Struct => "struct",
        SymbolKind::Class => "class",
        SymbolKind::Interface => "interface",
        SymbolKind::Enum => "enum",
        SymbolKind::TypeAlias => "type",
        SymbolKind::Macro => "macro",
        _ => "sym",
    }
}

fn short_path(path: &str) -> &str {
    let last = match path.rfind('/') {
        None => return path,
        Some(i) => i,
    };
    match path[..last].rfind('/') {
        None | Some(0) => path,
        Some(second) => &path[second + 1..],
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mapper::{MappedFile, Signature, SymbolKind};

    fn make_sig(name: &str, kind: SymbolKind, raw: &str, line: usize) -> Signature {
        Signature {
            raw: raw.to_string(),
            ckb_id: None,
            symbol_name: Some(name.to_string()),
            qualified_name: Some(name.to_string()),
            kind,
            line_start: line,
            col_start: 0,
            line_end: line,
            col_end: raw.len(),
            confidence: 30,
            doc_comment: None,
            body: None,
            tested: false,
        }
    }

    fn make_file(path: &str, sigs: Vec<Signature>) -> MappedFile {
        MappedFile {
            path: path.to_string(),
            imports: vec![],
            signatures: sigs,
            docstrings: None,
            parameters: None,
            return_types: None,
            churn_label: None,
            inline_test_fns: vec![],
        }
    }

    #[test]
    fn score_symbol_matches_name_terms() {
        let sig = make_sig("verify_token", SymbolKind::Function, "pub fn verify_token(t: &str)", 0);
        let score = score_symbol(&sig, &["verify".to_string(), "token".to_string()], 1.0);
        assert!(score > 5.0, "expected high score for name match, got {score}");
    }

    #[test]
    fn score_symbol_zero_for_no_match() {
        let sig = make_sig("unrelated_fn", SymbolKind::Function, "fn unrelated_fn()", 0);
        let score = score_symbol(&sig, &["auth".to_string(), "login".to_string()], 1.0);
        assert_eq!(score, 0.0);
    }

    #[test]
    fn order_puts_top_function_first() {
        let scored = vec![
            ScoredSymbol {
                file: "src/auth.rs".into(),
                sig: make_sig("Claims", SymbolKind::Struct, "pub struct Claims {}", 0),
                score: 8.0,
            },
            ScoredSymbol {
                file: "src/auth.rs".into(),
                sig: make_sig("verify", SymbolKind::Function, "pub fn verify()", 10),
                score: 10.0,
            },
        ];
        let ordered = order_for_reading(scored, 4);
        assert_eq!(ordered[0].sig.symbol_name.as_deref(), Some("verify"),
            "top fn should come first");
        assert_eq!(ordered[1].sig.symbol_name.as_deref(), Some("Claims"),
            "type should follow");
    }

    #[test]
    fn render_answer_produces_numbered_items() {
        let result = AnswerResult {
            query: "how does auth work?".into(),
            items: vec![
                EvidenceItem {
                    index: 1,
                    name: "verify_token".into(),
                    file: "src/auth.rs".into(),
                    line: 42,
                    kind: SymbolKind::Function,
                    sig: "pub fn verify_token(token: &str) -> Result<Claims>".into(),
                    body: None,
                    connection: Some("[uses type #2]".into()),
                    role_note: Some("core logic".into()),
                },
                EvidenceItem {
                    index: 2,
                    name: "Claims".into(),
                    file: "src/types.rs".into(),
                    line: 14,
                    kind: SymbolKind::Struct,
                    sig: "pub struct Claims { sub: UserId, exp: u64 }".into(),
                    body: None,
                    connection: None,
                    role_note: Some("type".into()),
                },
            ],
            tokens_used: 0,
            budget_hit: false,
            files_searched: 10,
        };
        let rendered = render_answer(&result);
        assert!(rendered.contains("Evidence for:"));
        assert!(rendered.contains("1  verify_token"));
        assert!(rendered.contains("2  Claims"));
        assert!(rendered.contains("[uses type #2]"));
        assert!(rendered.contains("[core logic]"));
    }
}
