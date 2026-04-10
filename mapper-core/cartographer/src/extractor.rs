//! Tree-sitter based skeleton extraction — Tier 2 (confidence = 60).
//!
//! Replaces the regex heuristics in `mapper.rs` for supported languages.
//! Falls back gracefully: callers receive `None` for unsupported extensions
//! and use the regex path instead.
//!
//! Each language is an optional Cargo feature:
//!   lang-rust, lang-go, lang-python, lang-typescript, lang-javascript
//!
//! Build without any tree-sitter grammar:
//!   cargo build --no-default-features
//!
//! Build for a single language only:
//!   cargo build --no-default-features --features lang-rust

use crate::mapper::{Signature, SymbolKind};
use std::path::Path;

// tree-sitter core types are only needed when at least one grammar is compiled in.
#[cfg(any(
    feature = "lang-rust",
    feature = "lang-go",
    feature = "lang-python",
    feature = "lang-typescript",
    feature = "lang-javascript",
))]
use tree_sitter::{Language, Node, Parser};

/// Confidence score assigned to tree-sitter extracted symbols (LIP Tier 2).
#[cfg(any(
    feature = "lang-rust",
    feature = "lang-go",
    feature = "lang-python",
    feature = "lang-typescript",
    feature = "lang-javascript",
))]
const CONFIDENCE_TS: u8 = 60;

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

/// Attempt tree-sitter extraction for `path` / `source`.
///
/// Returns `Some(signatures)` with `confidence = 60` for supported languages,
/// or `None` when the grammar is not compiled in or the extension is unsupported
/// (caller falls back to Tier 1 regex).
pub fn ts_extract(path: &Path, source: &str) -> Option<Vec<Signature>> {
    let ext = path.extension()?.to_str()?.to_lowercase();
    match ext.as_str() {
        #[cfg(feature = "lang-rust")]
        "rs" => Some(extract_rust(source, path)),

        #[cfg(feature = "lang-go")]
        "go" => Some(extract_go(source, path)),

        #[cfg(feature = "lang-python")]
        "py" => Some(extract_python(source, path)),

        #[cfg(feature = "lang-typescript")]
        "ts" => Some(extract_typescript(source, path, false)),

        #[cfg(feature = "lang-typescript")]
        "tsx" => Some(extract_typescript(source, path, true)),

        #[cfg(feature = "lang-javascript")]
        "js" | "jsx" | "mjs" | "cjs" => Some(extract_javascript(source, path)),

        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Shared helpers — only compiled when at least one grammar is active
// ---------------------------------------------------------------------------

#[cfg(any(
    feature = "lang-rust",
    feature = "lang-go",
    feature = "lang-python",
    feature = "lang-typescript",
    feature = "lang-javascript",
))]
fn node_text<'a>(node: &Node, src: &'a [u8]) -> &'a str {
    node.utf8_text(src).unwrap_or("")
}

/// Extract the signature text of a node up to (but not including) its body.
/// Works for any language where the body is a `block` or `statement_block` child.
#[cfg(any(
    feature = "lang-rust",
    feature = "lang-go",
    feature = "lang-python",
    feature = "lang-typescript",
    feature = "lang-javascript",
))]
fn sig_up_to_block(node: &Node, src: &[u8]) -> String {
    let body_start = {
        let mut cur = node.walk();
        let children: Vec<_> = node.children(&mut cur).collect();
        children.iter()
            .find(|c| matches!(c.kind(),
                "block" | "statement_block" | "class_body" | "declaration_list" |
                "enum_body" | "interface_body" | "object_type"
            ))
            .map(|c| c.start_byte())
            .unwrap_or(node.end_byte())
    };
    let raw = std::str::from_utf8(&src[node.start_byte()..body_start]).unwrap_or("");
    let collapsed: String = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    collapsed.trim_end_matches(|c: char| c == '{' || c.is_whitespace()).to_string()
}

/// Python variant: trim up to the colon that ends the function/class header.
#[cfg(feature = "lang-python")]
fn sig_up_to_colon(node: &Node, src: &[u8]) -> String {
    let body_start = {
        let mut cur = node.walk();
        let children: Vec<_> = node.children(&mut cur).collect();
        children.iter()
            .find(|c| c.kind() == "block")
            .map(|c| c.start_byte())
            .unwrap_or(node.end_byte())
    };
    let raw = std::str::from_utf8(&src[node.start_byte()..body_start]).unwrap_or("");
    let collapsed: String = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    collapsed.trim_end_matches(|c: char| c == ':' || c.is_whitespace()).to_string()
}

/// First non-body line of a node (for structs, enums, etc. where we skip the body).
#[cfg(any(
    feature = "lang-rust",
    feature = "lang-go",
    feature = "lang-python",
    feature = "lang-typescript",
    feature = "lang-javascript",
))]
fn first_line(node: &Node, src: &[u8]) -> String {
    let text = node_text(node, src);
    text.lines().next().unwrap_or("").split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Look backwards from `node` for a preceding doc-comment sibling.
#[cfg(any(
    feature = "lang-rust",
    feature = "lang-go",
    feature = "lang-python",
    feature = "lang-typescript",
    feature = "lang-javascript",
))]
fn preceding_doc_comment(node: &Node, src: &[u8]) -> Option<String> {
    let mut prev = node.prev_sibling()?;
    let mut lines: Vec<String> = Vec::new();
    loop {
        match prev.kind() {
            "line_comment" | "block_comment" | "comment" => {
                lines.push(node_text(&prev, src).to_string());
                match prev.prev_sibling() {
                    Some(p) => prev = p,
                    None => break,
                }
            }
            _ => break,
        }
    }
    if lines.is_empty() {
        None
    } else {
        lines.reverse();
        Some(lines.join("\n"))
    }
}

/// Build a LIP URI: `lip://local/<path>#<qualified>`.
#[cfg(any(
    feature = "lang-rust",
    feature = "lang-go",
    feature = "lang-python",
    feature = "lang-typescript",
    feature = "lang-javascript",
))]
fn lip_uri(path: &Path, qualified: &str) -> String {
    let p = path.to_string_lossy().replace('\\', "/");
    let p = p.trim_start_matches("./").trim_start_matches('/');
    format!("lip://local/{}#{}", p, qualified)
}

#[cfg(any(
    feature = "lang-rust",
    feature = "lang-go",
    feature = "lang-python",
    feature = "lang-typescript",
    feature = "lang-javascript",
))]
fn make_sig(
    raw: String,
    kind: SymbolKind,
    line: usize,
    path: &Path,
    name: &str,
    qualified: &str,
    doc: Option<String>,
) -> Signature {
    Signature {
        raw,
        ckb_id: Some(lip_uri(path, qualified)),
        symbol_name: Some(name.to_string()),
        qualified_name: Some(qualified.to_string()),
        kind,
        line_start: line,
        confidence: CONFIDENCE_TS,
        doc_comment: doc,
    }
}

#[cfg(any(feature = "lang-rust", feature = "lang-go", feature = "lang-python",
          feature = "lang-typescript", feature = "lang-javascript"))]
fn scope_qualify(scope: &[String], name: &str) -> String {
    match scope.last() {
        Some(s) if !s.is_empty() => format!("{}.{}", s, name),
        _ => name.to_string(),
    }
}

// ---------------------------------------------------------------------------
// Rust
// ---------------------------------------------------------------------------

#[cfg(feature = "lang-rust")]
fn extract_rust(source: &str, path: &Path) -> Vec<Signature> {
    let mut parser = Parser::new();
    let lang: Language = tree_sitter_rust::language();
    if parser.set_language(&lang).is_err() { return vec![]; }
    let tree = match parser.parse(source.as_bytes(), None) {
        Some(t) => t,
        None => return vec![],
    };
    let mut sigs = Vec::new();
    let mut scope: Vec<String> = Vec::new();
    walk_rust(&tree.root_node(), source.as_bytes(), path, &mut sigs, &mut scope);
    sigs
}

#[cfg(feature = "lang-rust")]
fn walk_rust(node: &Node, src: &[u8], path: &Path, sigs: &mut Vec<Signature>, scope: &mut Vec<String>) {
    match node.kind() {
        "impl_item" => {
            let type_name = node.child_by_field_name("type")
                .map(|n| node_text(&n, src).to_string())
                .unwrap_or_default();
            let base = type_name.split('<').next().unwrap_or(&type_name).trim().to_string();
            scope.push(base);
            if let Some(body) = node.child_by_field_name("body") {
                let mut cur = body.walk();
                for child in body.children(&mut cur) {
                    walk_rust(&child, src, path, sigs, scope);
                }
            }
            scope.pop();
        }

        "trait_item" => {
            let name = node.child_by_field_name("name")
                .map(|n| node_text(&n, src).to_string())
                .unwrap_or_default();
            let raw = sig_up_to_block(node, src);
            let doc = preceding_doc_comment(node, src);
            sigs.push(make_sig(raw, SymbolKind::Interface, node.start_position().row, path, &name, &name, doc));
            scope.push(name);
            if let Some(body) = node.child_by_field_name("body") {
                let mut cur = body.walk();
                for child in body.children(&mut cur) {
                    walk_rust(&child, src, path, sigs, scope);
                }
            }
            scope.pop();
        }

        "function_item" => {
            let name = node.child_by_field_name("name")
                .map(|n| node_text(&n, src).to_string())
                .unwrap_or_default();
            let vis = {
                let mut cur = node.walk();
                let children: Vec<_> = node.children(&mut cur).collect();
                children.iter()
                    .find(|c| c.kind() == "visibility_modifier")
                    .map(|n| node_text(n, src).to_string())
            };
            let is_pub = vis.as_deref().map(|v| v.contains("pub")).unwrap_or(false);
            if scope.is_empty() && !is_pub { return; }
            let qualified = if let Some(sc) = scope.last() {
                format!("{}.{}", sc, name)
            } else {
                name.clone()
            };
            let kind = if scope.is_empty() { SymbolKind::Function } else { SymbolKind::Method };
            let raw = sig_up_to_block(node, src);
            let doc = preceding_doc_comment(node, src);
            sigs.push(make_sig(raw, kind, node.start_position().row, path, &name, &qualified, doc));
        }

        "struct_item" => {
            let name = node.child_by_field_name("name")
                .map(|n| node_text(&n, src).to_string())
                .unwrap_or_default();
            if name.is_empty() { return; }
            let raw = first_line(node, src);
            let doc = preceding_doc_comment(node, src);
            let qualified = scope_qualify(scope, &name);
            sigs.push(make_sig(raw, SymbolKind::Struct, node.start_position().row, path, &name, &qualified, doc));
        }

        "enum_item" => {
            let name = node.child_by_field_name("name")
                .map(|n| node_text(&n, src).to_string())
                .unwrap_or_default();
            if name.is_empty() { return; }
            let raw = first_line(node, src);
            let doc = preceding_doc_comment(node, src);
            let qualified = scope_qualify(scope, &name);
            sigs.push(make_sig(raw, SymbolKind::Enum, node.start_position().row, path, &name, &qualified, doc));
        }

        "type_item" => {
            let name = node.child_by_field_name("name")
                .map(|n| node_text(&n, src).to_string())
                .unwrap_or_default();
            if name.is_empty() { return; }
            let raw = node_text(node, src).split_whitespace().collect::<Vec<_>>().join(" ");
            let doc = preceding_doc_comment(node, src);
            sigs.push(make_sig(raw, SymbolKind::TypeAlias, node.start_position().row, path, &name, &name, doc));
        }

        "const_item" | "static_item" => {
            let name = node.child_by_field_name("name")
                .map(|n| node_text(&n, src).to_string())
                .unwrap_or_default();
            if name.is_empty() { return; }
            let vis = {
                let mut cur = node.walk();
                let children: Vec<_> = node.children(&mut cur).collect();
                children.iter()
                    .find(|c| c.kind() == "visibility_modifier")
                    .map(|n| node_text(n, src).to_string())
            };
            if scope.is_empty() && !vis.as_deref().map(|v| v.contains("pub")).unwrap_or(false) {
                return;
            }
            let raw = node_text(node, src).split_whitespace().collect::<Vec<_>>().join(" ");
            let doc = preceding_doc_comment(node, src);
            sigs.push(make_sig(raw, SymbolKind::Variable, node.start_position().row, path, &name, &name, doc));
        }

        "macro_definition" => {
            let name = {
                let mut cur = node.walk();
                let children: Vec<_> = node.children(&mut cur).collect();
                children.iter()
                    .find(|c| c.kind() == "identifier")
                    .map(|n| node_text(n, src).to_string())
                    .unwrap_or_default()
            };
            if name.is_empty() { return; }
            let raw = format!("macro_rules! {}", name);
            let doc = preceding_doc_comment(node, src);
            sigs.push(make_sig(raw, SymbolKind::Macro, node.start_position().row, path, &name, &name, doc));
        }

        "mod_item" => {
            let name = node.child_by_field_name("name")
                .map(|n| node_text(&n, src).to_string())
                .unwrap_or_default();
            if name.is_empty() { return; }
            let doc = preceding_doc_comment(node, src);
            let raw = format!("mod {}", name);
            sigs.push(make_sig(raw, SymbolKind::Namespace, node.start_position().row, path, &name, &name, doc));
            if let Some(body) = node.child_by_field_name("body") {
                scope.push(name);
                let mut cur = body.walk();
                for child in body.children(&mut cur) {
                    walk_rust(&child, src, path, sigs, scope);
                }
                scope.pop();
            }
        }

        _ => {
            let mut cur = node.walk();
            for child in node.children(&mut cur) {
                walk_rust(&child, src, path, sigs, scope);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Go
// ---------------------------------------------------------------------------

#[cfg(feature = "lang-go")]
fn extract_go(source: &str, path: &Path) -> Vec<Signature> {
    let mut parser = Parser::new();
    let lang: Language = tree_sitter_go::language();
    if parser.set_language(&lang).is_err() { return vec![]; }
    let tree = match parser.parse(source.as_bytes(), None) {
        Some(t) => t,
        None => return vec![],
    };
    let mut sigs = Vec::new();
    walk_go(&tree.root_node(), source.as_bytes(), path, &mut sigs);
    sigs
}

#[cfg(feature = "lang-go")]
fn walk_go(node: &Node, src: &[u8], path: &Path, sigs: &mut Vec<Signature>) {
    match node.kind() {
        "function_declaration" => {
            let name = node.child_by_field_name("name")
                .map(|n| node_text(&n, src).to_string())
                .unwrap_or_default();
            if name.is_empty() { return; }
            let raw = sig_up_to_block(node, src);
            let doc = preceding_doc_comment(node, src);
            sigs.push(make_sig(raw, SymbolKind::Function, node.start_position().row, path, &name, &name, doc));
        }

        "method_declaration" => {
            let name = node.child_by_field_name("name")
                .map(|n| node_text(&n, src).to_string())
                .unwrap_or_default();
            let receiver_type = node.child_by_field_name("receiver")
                .and_then(|r| {
                    let mut cur = r.walk();
                    let children: Vec<_> = r.children(&mut cur).collect();
                    children.iter()
                        .find(|c| matches!(c.kind(), "parameter_declaration" | "variadic_parameter_declaration"))
                        .and_then(|p| p.child_by_field_name("type"))
                        .map(|t| {
                            node_text(&t, src)
                                .trim_start_matches('*')
                                .split('<').next().unwrap_or("")
                                .trim().to_string()
                        })
                })
                .unwrap_or_default();
            let qualified = if receiver_type.is_empty() {
                name.clone()
            } else {
                format!("{}.{}", receiver_type, name)
            };
            let raw = sig_up_to_block(node, src);
            let doc = preceding_doc_comment(node, src);
            sigs.push(make_sig(raw, SymbolKind::Method, node.start_position().row, path, &name, &qualified, doc));
        }

        "type_declaration" => {
            let mut cur = node.walk();
            for child in node.children(&mut cur) {
                if child.kind() == "type_spec" {
                    let name = child.child_by_field_name("name")
                        .map(|n| node_text(&n, src).to_string())
                        .unwrap_or_default();
                    if name.is_empty() { continue; }
                    let type_node = child.child_by_field_name("type");
                    let kind = match type_node.as_ref().map(|n| n.kind()) {
                        Some("struct_type")    => SymbolKind::Struct,
                        Some("interface_type") => SymbolKind::Interface,
                        _                      => SymbolKind::TypeAlias,
                    };
                    let raw = first_line(&child, src);
                    let doc = preceding_doc_comment(&child, src);
                    sigs.push(make_sig(raw, kind, child.start_position().row, path, &name, &name, doc));
                }
            }
        }

        "const_declaration" | "var_declaration" => {
            let mut cur = node.walk();
            let top_children: Vec<_> = node.children(&mut cur).collect();
            for child in top_children {
                if matches!(child.kind(), "const_spec" | "var_spec") {
                    let name = child.child_by_field_name("name")
                        .or_else(|| {
                            let mut c = child.walk();
                            let cc: Vec<_> = child.children(&mut c).collect();
                            cc.into_iter().find(|n| n.kind() == "identifier")
                        })
                        .map(|n| node_text(&n, src).to_string())
                        .unwrap_or_default();
                    if name.is_empty() { continue; }
                    let raw = node_text(&child, src).split_whitespace().collect::<Vec<_>>().join(" ");
                    let doc = preceding_doc_comment(&child, src);
                    sigs.push(make_sig(raw, SymbolKind::Variable, child.start_position().row, path, &name, &name, doc));
                }
            }
        }

        _ => {
            let mut cur = node.walk();
            for child in node.children(&mut cur) {
                walk_go(&child, src, path, sigs);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Python
// ---------------------------------------------------------------------------

#[cfg(feature = "lang-python")]
fn extract_python(source: &str, path: &Path) -> Vec<Signature> {
    let mut parser = Parser::new();
    let lang: Language = tree_sitter_python::language();
    if parser.set_language(&lang).is_err() { return vec![]; }
    let tree = match parser.parse(source.as_bytes(), None) {
        Some(t) => t,
        None => return vec![],
    };
    let mut sigs = Vec::new();
    let mut scope: Vec<String> = Vec::new();
    walk_python(&tree.root_node(), source.as_bytes(), path, &mut sigs, &mut scope);
    sigs
}

#[cfg(feature = "lang-python")]
fn walk_python(node: &Node, src: &[u8], path: &Path, sigs: &mut Vec<Signature>, scope: &mut Vec<String>) {
    match node.kind() {
        "function_definition" => {
            let name = node.child_by_field_name("name")
                .map(|n| node_text(&n, src).to_string())
                .unwrap_or_default();
            if name.is_empty() { return; }
            if name.starts_with('_') && !name.starts_with("__") && scope.is_empty() { return; }
            let qualified = scope_qualify(scope, &name);
            let kind = if scope.is_empty() { SymbolKind::Function } else { SymbolKind::Method };
            let raw = sig_up_to_colon(node, src);
            let doc = preceding_doc_comment(node, src);
            sigs.push(make_sig(raw, kind, node.start_position().row, path, &name, &qualified, doc));
        }

        "class_definition" => {
            let name = node.child_by_field_name("name")
                .map(|n| node_text(&n, src).to_string())
                .unwrap_or_default();
            if name.is_empty() { return; }
            let raw = sig_up_to_colon(node, src);
            let doc = preceding_doc_comment(node, src);
            sigs.push(make_sig(raw, SymbolKind::Class, node.start_position().row, path, &name, &name, doc));
            scope.push(name);
            if let Some(body) = node.child_by_field_name("body") {
                let mut cur = body.walk();
                for child in body.children(&mut cur) {
                    walk_python(&child, src, path, sigs, scope);
                }
            }
            scope.pop();
        }

        "decorated_definition" => {
            let mut cur = node.walk();
            let children: Vec<Node> = node.children(&mut cur).collect();
            if let Some(def) = children.last() {
                walk_python(def, src, path, sigs, scope);
            }
        }

        "assignment" => {
            if scope.is_empty() {
                let name = node.child_by_field_name("left")
                    .map(|n| node_text(&n, src).to_string())
                    .unwrap_or_default();
                if !name.is_empty() && name.chars().all(|c| c.is_uppercase() || c == '_' || c.is_numeric()) {
                    let raw = first_line(node, src);
                    sigs.push(make_sig(raw, SymbolKind::Variable, node.start_position().row, path, &name, &name, None));
                }
            }
        }

        _ => {
            let mut cur = node.walk();
            for child in node.children(&mut cur) {
                walk_python(&child, src, path, sigs, scope);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// TypeScript / TSX
// ---------------------------------------------------------------------------

#[cfg(feature = "lang-typescript")]
fn extract_typescript(source: &str, path: &Path, is_tsx: bool) -> Vec<Signature> {
    let mut parser = Parser::new();
    let lang: Language = if is_tsx {
        tree_sitter_typescript::language_tsx()
    } else {
        tree_sitter_typescript::language_typescript()
    };
    if parser.set_language(&lang).is_err() { return vec![]; }
    let tree = match parser.parse(source.as_bytes(), None) {
        Some(t) => t,
        None => return vec![],
    };
    let mut sigs = Vec::new();
    let mut scope: Vec<String> = Vec::new();
    walk_ts(&tree.root_node(), source.as_bytes(), path, &mut sigs, &mut scope);
    sigs
}

// ---------------------------------------------------------------------------
// JavaScript (JSX / MJS / CJS)
// ---------------------------------------------------------------------------

#[cfg(feature = "lang-javascript")]
fn extract_javascript(source: &str, path: &Path) -> Vec<Signature> {
    let mut parser = Parser::new();
    let lang: Language = tree_sitter_javascript::language();
    if parser.set_language(&lang).is_err() { return vec![]; }
    let tree = match parser.parse(source.as_bytes(), None) {
        Some(t) => t,
        None => return vec![],
    };
    let mut sigs = Vec::new();
    let mut scope: Vec<String> = Vec::new();
    walk_ts(&tree.root_node(), source.as_bytes(), path, &mut sigs, &mut scope);
    sigs
}

// Shared walker for TS and JS — both grammars produce compatible node kinds.
#[cfg(any(feature = "lang-typescript", feature = "lang-javascript"))]
fn walk_ts(node: &Node, src: &[u8], path: &Path, sigs: &mut Vec<Signature>, scope: &mut Vec<String>) {
    match node.kind() {
        "function_declaration" | "function" | "generator_function_declaration" => {
            let name = node.child_by_field_name("name")
                .map(|n| node_text(&n, src).to_string())
                .unwrap_or_default();
            if name.is_empty() { return; }
            let qualified = scope_qualify(scope, &name);
            let kind = if scope.is_empty() { SymbolKind::Function } else { SymbolKind::Method };
            let raw = sig_up_to_block(node, src);
            let doc = preceding_doc_comment(node, src);
            sigs.push(make_sig(raw, kind, node.start_position().row, path, &name, &qualified, doc));
        }

        "class_declaration" | "abstract_class_declaration" | "class" => {
            let name = node.child_by_field_name("name")
                .map(|n| node_text(&n, src).to_string())
                .unwrap_or_default();
            if name.is_empty() { return; }
            let raw = sig_up_to_block(node, src);
            let doc = preceding_doc_comment(node, src);
            sigs.push(make_sig(raw, SymbolKind::Class, node.start_position().row, path, &name, &name, doc));
            scope.push(name);
            if let Some(body) = node.child_by_field_name("body") {
                let mut cur = body.walk();
                for child in body.children(&mut cur) {
                    walk_ts(&child, src, path, sigs, scope);
                }
            }
            scope.pop();
        }

        "method_definition" | "method_signature" => {
            let name = node.child_by_field_name("name")
                .map(|n| node_text(&n, src).to_string())
                .unwrap_or_default();
            if name.is_empty() || name == "constructor" { return; }
            if name.starts_with('#') || name.starts_with('[') { return; }
            let qualified = scope_qualify(scope, &name);
            let raw = sig_up_to_block(node, src);
            let doc = preceding_doc_comment(node, src);
            sigs.push(make_sig(raw, SymbolKind::Method, node.start_position().row, path, &name, &qualified, doc));
        }

        "interface_declaration" => {
            let name = node.child_by_field_name("name")
                .map(|n| node_text(&n, src).to_string())
                .unwrap_or_default();
            if name.is_empty() { return; }
            let raw = sig_up_to_block(node, src);
            let doc = preceding_doc_comment(node, src);
            sigs.push(make_sig(raw, SymbolKind::Interface, node.start_position().row, path, &name, &name, doc));
        }

        "type_alias_declaration" => {
            let name = node.child_by_field_name("name")
                .map(|n| node_text(&n, src).to_string())
                .unwrap_or_default();
            if name.is_empty() { return; }
            let raw = first_line(node, src);
            let doc = preceding_doc_comment(node, src);
            sigs.push(make_sig(raw, SymbolKind::TypeAlias, node.start_position().row, path, &name, &name, doc));
        }

        "enum_declaration" => {
            let name = node.child_by_field_name("name")
                .map(|n| node_text(&n, src).to_string())
                .unwrap_or_default();
            if name.is_empty() { return; }
            let raw = first_line(node, src);
            let doc = preceding_doc_comment(node, src);
            sigs.push(make_sig(raw, SymbolKind::Enum, node.start_position().row, path, &name, &name, doc));
        }

        "export_statement" | "export_clause" => {
            let mut cur = node.walk();
            for child in node.children(&mut cur) {
                if !matches!(child.kind(), "export" | "default" | "from" | "string" | ";" | "*" | "as") {
                    walk_ts(&child, src, path, sigs, scope);
                }
            }
        }

        "lexical_declaration" | "variable_declaration" => {
            let mut cur = node.walk();
            for decl in node.children(&mut cur) {
                if decl.kind() != "variable_declarator" { continue; }
                let name = decl.child_by_field_name("name")
                    .map(|n| node_text(&n, src).to_string())
                    .unwrap_or_default();
                if name.is_empty() { continue; }
                let value = decl.child_by_field_name("value");
                let is_fn = value.as_ref().map(|v| {
                    matches!(v.kind(), "arrow_function" | "function" | "function_expression" | "generator_function")
                }).unwrap_or(false);
                if !is_fn { continue; }
                let val = value.unwrap();
                let raw = format!("const {} = {}", name, sig_up_to_block(&val, src));
                let qualified = scope_qualify(scope, &name);
                let doc = preceding_doc_comment(node, src);
                sigs.push(make_sig(raw, SymbolKind::Function, decl.start_position().row, path, &name, &qualified, doc));
            }
        }

        _ => {
            let mut cur = node.walk();
            for child in node.children(&mut cur) {
                walk_ts(&child, src, path, sigs, scope);
            }
        }
    }
}
