//! Mapper module - Extracts skeleton signatures from source files
//! Mode A: --map provides "Satellite Vision" without function bodies

use regex::Regex;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Level of detail for skeleton extraction
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DetailLevel {
    Minimal,
    Standard,
    Extended,
}

/// A signature with metadata for CKB integration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Signature {
    pub raw: String,
    pub ckb_id: Option<String>,
    pub symbol_name: Option<String>,
    pub signature_type: Option<String>,
}

impl Signature {
    pub fn from_raw(raw: String) -> Self {
        let symbol_name = extract_symbol_name(&raw);
        let ckb_id = generate_ckb_id(&raw);

        Self {
            raw,
            ckb_id: Some(ckb_id),
            symbol_name,
            signature_type: None,
        }
    }

    pub fn with_type(mut self, sig_type: String) -> Self {
        self.signature_type = Some(sig_type);
        self
    }
}

fn extract_symbol_name(raw: &str) -> Option<String> {
    let patterns = [
        r"fn\s+(\w+)",
        r"def\s+(\w+)",
        r"function\s+(\w+)",
        r"class\s+(\w+)",
        r"interface\s+(\w+)",
        r"type\s+(\w+)",
        r"struct\s+(\w+)",
        r"enum\s+(\w+)",
    ];

    for pattern in &patterns {
        if let Ok(re) = Regex::new(pattern) {
            if let Some(caps) = re.captures(raw) {
                return caps.get(1).map(|m| m.as_str().to_string());
            }
        }
    }
    None
}

fn generate_ckb_id(raw: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::new();
    raw.hash(&mut hasher);
    format!("sym_{:x}", hasher.finish())[..12].to_string()
}

/// Represents a mapped file with only signatures (no implementation details)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MappedFile {
    pub path: String,
    pub imports: Vec<String>,
    pub signatures: Vec<Signature>,
    pub docstrings: Option<Vec<String>>,
    pub parameters: Option<Vec<String>>,
    pub return_types: Option<Vec<String>>,
}

impl MappedFile {
    pub fn new(path: String, imports: Vec<String>, signatures: Vec<Signature>) -> Self {
        Self {
            path,
            imports,
            signatures,
            docstrings: None,
            parameters: None,
            return_types: None,
        }
    }

    pub fn from_minimal(path: String, imports: Vec<String>) -> Self {
        Self {
            path,
            imports,
            signatures: Vec::new(),
            docstrings: None,
            parameters: None,
            return_types: None,
        }
    }

    pub fn with_signatures(mut self, signatures: Vec<Signature>) -> Self {
        self.signatures = signatures;
        self
    }

    pub fn with_docstrings(mut self, docstrings: Vec<String>) -> Self {
        self.docstrings = Some(docstrings);
        self
    }

    pub fn with_parameters(mut self, parameters: Vec<String>) -> Self {
        self.parameters = Some(parameters);
        self
    }

    pub fn with_return_types(mut self, return_types: Vec<String>) -> Self {
        self.return_types = Some(return_types);
        self
    }

    pub fn format(&self) -> String {
        let mut out = String::new();

        // Imports section
        if !self.imports.is_empty() {
            for imp in &self.imports {
                out.push_str(imp);
                out.push('\n');
            }
            out.push('\n');
        }

        // Signatures section
        for sig in &self.signatures {
            out.push_str(&sig.raw);
            out.push_str(" // ...\n");
        }

        out
    }

    pub fn to_ai_lang(&self, detail_level: DetailLevel) -> String {
        let mut out = String::new();

        out.push_str(&format!("({})\n", self.path));

        if !self.imports.is_empty() {
            let imports: Vec<String> = self
                .imports
                .iter()
                .map(|i| {
                    let parts: Vec<&str> = i.split_whitespace().collect();
                    parts
                        .last()
                        .map(|s| s.trim_matches(';'))
                        .unwrap_or(i)
                        .to_string()
                })
                .collect();
            out.push_str(&format!(" (imports: [{}])\n", imports.join(", ")));
        }

        match detail_level {
            DetailLevel::Minimal => {
                if !self.signatures.is_empty() {
                    let sigs: Vec<String> = self
                        .signatures
                        .iter()
                        .map(|s| {
                            let trimmed = s.raw.trim();
                            let without_body = trimmed.split('{').next().unwrap_or(trimmed).trim();
                            let simplified = without_body
                                .replace("pub ", "")
                                .replace("private ", "")
                                .replace("async ", "")
                                .replace("fn ", "fn ")
                                .replace("function ", "fn ")
                                .replace("def ", "fn ")
                                .replace("class ", "class ")
                                .replace("interface ", "if ");
                            simplified
                        })
                        .collect();
                    out.push_str(&format!(" (sigs: {})\n", sigs.join(", ")));
                }
            }
            DetailLevel::Standard => {
                if !self.signatures.is_empty() {
                    out.push_str(" (exports:\n");
                    for sig in &self.signatures {
                        let simplified = sig
                            .raw
                            .replace("pub ", "")
                            .replace("private ", "")
                            .replace("protected ", "");
                        out.push_str(&format!(
                            "  {} [{}]\n",
                            simplified,
                            sig.ckb_id.as_deref().unwrap_or("?")
                        ));
                    }
                    out.push_str(" )\n");
                }
            }
            DetailLevel::Extended => {
                if !self.signatures.is_empty() {
                    out.push_str(" (exports:\n");
                    for sig in &self.signatures {
                        out.push_str(&format!(
                            "  {} [{}]\n",
                            sig.raw,
                            sig.ckb_id.as_deref().unwrap_or("?")
                        ));
                    }
                    out.push_str(" )\n");
                }
                if let Some(ref docs) = self.docstrings {
                    if !docs.is_empty() {
                        out.push_str(&format!(" (doc: {})\n", docs.first().unwrap()));
                    }
                }
            }
        }

        out
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirectorySummary {
    pub path: String,
    pub file_count: usize,
    pub signature_count: usize,
    pub description: Option<String>,
    pub modules: Vec<String>,
}

/// Generate satellite view summary for a directory
pub fn summarize_directory(files: &[&MappedFile], root_path: &str) -> DirectorySummary {
    let mut file_count = 0;
    let mut signature_count = 0;
    let mut modules = Vec::new();

    for file in files {
        file_count += 1;
        signature_count += file.signatures.len();
        modules.push(file.path.clone());
    }

    let description = find_directory_description(files, root_path);

    DirectorySummary {
        path: root_path.to_string(),
        file_count,
        signature_count,
        description,
        modules,
    }
}

fn find_directory_description(files: &[&MappedFile], _root_path: &str) -> Option<String> {
    for file in files {
        let path_lower = file.path.to_lowercase();
        if path_lower.contains("readme")
            || path_lower.contains("mod.rs")
            || path_lower.contains("mod.rs")
        {
            if let Some(ref sigs) = file.docstrings {
                if !sigs.is_empty() {
                    return Some(sigs[0].clone());
                }
            }
        }
    }
    None
}

/// Extract skeleton map from file content based on language
pub fn extract_skeleton(path: &Path, content: &str) -> MappedFile {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    let rel_path = path.to_string_lossy().replace('\\', "/");

    match ext.as_str() {
        "js" | "jsx" | "ts" | "tsx" | "mjs" | "cjs" => extract_js_ts(rel_path, content),
        "rs" => extract_rust(rel_path, content),
        "py" => extract_python(rel_path, content),
        "go" => extract_go(rel_path, content),
        "java" | "kt" | "scala" => extract_java_like(rel_path, content),
        "c" | "cpp" | "cc" | "cxx" | "h" | "hpp" => extract_c_cpp(rel_path, content),
        "rb" => extract_ruby(rel_path, content),
        "php" => extract_php(rel_path, content),
        // Non-code files - return empty skeleton (no false positives from code examples in docs)
        "md" | "txt" | "json" | "yaml" | "yml" | "toml" | "xml" | "html" | "css" | "scss"
        | "less" | "svg" | "lock" => MappedFile {
            path: rel_path,
            imports: Vec::new(),
            signatures: Vec::new(),
            docstrings: None,
            parameters: None,
            return_types: None,
        },
        _ => extract_generic(rel_path, content),
    }
}

/// Extract JS/TS skeleton (imports, exports, functions, classes, interfaces, types)
fn extract_js_ts(path: String, content: &str) -> MappedFile {
    let mut imports = Vec::new();
    let mut signatures = Vec::new();

    // Import patterns
    let import_re = Regex::new(r"^(?:import\s+.+|export\s+\{[^}]+\}\s+from\s+.+|export\s+\*\s+from\s+.+|const\s+\w+\s*=\s*require\(.+\))").unwrap();

    // Signature patterns
    let sig_patterns = [
        r"^export\s+(?:default\s+)?(?:async\s+)?function\s+\w+[^{]*",
        r"^export\s+(?:default\s+)?class\s+\w+[^{]*",
        r"^export\s+(?:default\s+)?interface\s+\w+[^{]*",
        r"^export\s+(?:default\s+)?type\s+\w+\s*=",
        r"^export\s+(?:default\s+)?const\s+\w+\s*(?::\s*[^=]+)?\s*=\s*(?:async\s+)?\([^)]*\)\s*(?::\s*[^=]+)?\s*=>",
        r"^export\s+(?:default\s+)?const\s+\w+\s*:",
        r"^(?:async\s+)?function\s+\w+[^{]*",
        r"^class\s+\w+[^{]*",
        r"^interface\s+\w+[^{]*",
        r"^type\s+\w+\s*=",
        r"^const\s+\w+\s*(?::\s*[^=]+)?\s*=\s*(?:async\s+)?\([^)]*\)\s*(?::\s*[^=]+)?\s*=>",
    ];

    for line in content.lines() {
        let trimmed = line.trim();

        if import_re.is_match(trimmed) {
            imports.push(trimmed.to_string());
            continue;
        }

        for pattern in &sig_patterns {
            if let Ok(re) = Regex::new(pattern) {
                if let Some(m) = re.find(trimmed) {
                    let sig = m.as_str().trim_end_matches('{').trim();
                    signatures.push(
                        Signature::from_raw(sig.to_string()).with_type("function".to_string()),
                    );
                    break;
                }
            }
        }
    }

    MappedFile {
        path,
        imports,
        signatures,
        docstrings: None,
        parameters: None,
        return_types: None,
    }
}

/// Extract Rust skeleton (use, mod, fn, struct, enum, impl, trait)
fn extract_rust(path: String, content: &str) -> MappedFile {
    let mut imports = Vec::new();
    let mut signatures = Vec::new();

    let import_re = Regex::new(r"^(?:use\s+.+;|mod\s+\w+;)").unwrap();

    let sig_patterns = [
        r"^pub(?:\([^)]+\))?\s+(?:async\s+)?fn\s+\w+[^{]*",
        r"^(?:async\s+)?fn\s+\w+[^{]*",
        r"^pub(?:\([^)]+\))?\s+struct\s+\w+[^{;]*",
        r"^struct\s+\w+[^{;]*",
        r"^pub(?:\([^)]+\))?\s+enum\s+\w+[^{]*",
        r"^enum\s+\w+[^{]*",
        r"^pub(?:\([^)]+\))?\s+trait\s+\w+[^{]*",
        r"^trait\s+\w+[^{]*",
        r"^impl(?:<[^>]+>)?\s+\w+[^{]*",
        r"^pub(?:\([^)]+\))?\s+type\s+\w+\s*=",
        r"^pub(?:\([^)]+\))?\s+const\s+\w+\s*:",
        r"^pub(?:\([^)]+\))?\s+static\s+\w+\s*:",
    ];

    for line in content.lines() {
        let trimmed = line.trim();

        if import_re.is_match(trimmed) {
            imports.push(trimmed.to_string());
            continue;
        }

        for pattern in &sig_patterns {
            if let Ok(re) = Regex::new(pattern) {
                if let Some(m) = re.find(trimmed) {
                    let sig = m.as_str().trim_end_matches('{').trim();
                    signatures.push(
                        Signature::from_raw(sig.to_string()).with_type("function".to_string()),
                    );
                    break;
                }
            }
        }
    }

    MappedFile {
        path,
        imports,
        signatures,
        docstrings: None,
        parameters: None,
        return_types: None,
    }
}

/// Extract Python skeleton (import, from, def, class)
fn extract_python(path: String, content: &str) -> MappedFile {
    let mut imports = Vec::new();
    let mut signatures = Vec::new();

    let import_re = Regex::new(r"^(?:import\s+.+|from\s+.+\s+import\s+.+)").unwrap();

    let sig_patterns = [
        r"^(?:async\s+)?def\s+\w+\s*\([^)]*\)[^:]*:",
        r"^class\s+\w+[^:]*:",
        r"^@\w+(?:\([^)]*\))?", // Decorators
    ];

    for line in content.lines() {
        let trimmed = line.trim();

        if import_re.is_match(trimmed) {
            imports.push(trimmed.to_string());
            continue;
        }

        for pattern in &sig_patterns {
            if let Ok(re) = Regex::new(pattern) {
                if let Some(m) = re.find(trimmed) {
                    signatures.push(Signature::from_raw(m.as_str().to_string()));
                    break;
                }
            }
        }
    }

    MappedFile {
        path,
        imports,
        signatures,
        docstrings: None,
        parameters: None,
        return_types: None,
    }
}

/// Extract Go skeleton (import, func, type, struct, interface)
fn extract_go(path: String, content: &str) -> MappedFile {
    let mut imports = Vec::new();
    let mut signatures = Vec::new();

    let import_re = Regex::new(r#"^import\s+(?:\(|"[^"]+")"#).unwrap();

    let sig_patterns = [
        r"^func\s+(?:\([^)]+\)\s+)?\w+\s*\([^)]*\)[^{]*",
        r"^type\s+\w+\s+struct",
        r"^type\s+\w+\s+interface",
        r"^type\s+\w+\s+=?\s*\w+",
        r"^var\s+\w+\s+",
        r"^const\s+\w+\s+",
    ];

    for line in content.lines() {
        let trimmed = line.trim();

        if import_re.is_match(trimmed) {
            imports.push(trimmed.to_string());
            continue;
        }

        for pattern in &sig_patterns {
            if let Ok(re) = Regex::new(pattern) {
                if let Some(m) = re.find(trimmed) {
                    let sig = m.as_str().trim_end_matches('{').trim();
                    signatures.push(
                        Signature::from_raw(sig.to_string()).with_type("function".to_string()),
                    );
                    break;
                }
            }
        }
    }

    MappedFile {
        path,
        imports,
        signatures,
        docstrings: None,
        parameters: None,
        return_types: None,
    }
}

/// Extract Java/Kotlin/Scala skeleton
fn extract_java_like(path: String, content: &str) -> MappedFile {
    let mut imports = Vec::new();
    let mut signatures = Vec::new();

    let import_re = Regex::new(r"^(?:import\s+.+;|package\s+.+;)").unwrap();

    let sig_patterns = [
        r"^(?:public|private|protected)?\s*(?:static)?\s*(?:final)?\s*(?:abstract)?\s*class\s+\w+[^{]*",
        r"^(?:public|private|protected)?\s*(?:static)?\s*(?:final)?\s*interface\s+\w+[^{]*",
        r"^(?:public|private|protected)?\s*(?:static)?\s*(?:final)?\s*(?:abstract)?\s*(?:synchronized)?\s*\w+(?:<[^>]+>)?\s+\w+\s*\([^)]*\)[^{;]*",
        r"^@\w+(?:\([^)]*\))?",               // Annotations
        r"^(?:fun|suspend\s+fun)\s+\w+[^{]*", // Kotlin
        r"^(?:def|val|var)\s+\w+[^{=]*",      // Scala
    ];

    for line in content.lines() {
        let trimmed = line.trim();

        if import_re.is_match(trimmed) {
            imports.push(trimmed.to_string());
            continue;
        }

        for pattern in &sig_patterns {
            if let Ok(re) = Regex::new(pattern) {
                if let Some(m) = re.find(trimmed) {
                    let sig = m.as_str().trim_end_matches('{').trim();
                    signatures.push(
                        Signature::from_raw(sig.to_string()).with_type("function".to_string()),
                    );
                    break;
                }
            }
        }
    }

    MappedFile {
        path,
        imports,
        signatures,
        docstrings: None,
        parameters: None,
        return_types: None,
    }
}

/// Extract C/C++ skeleton
fn extract_c_cpp(path: String, content: &str) -> MappedFile {
    let mut imports = Vec::new();
    let mut signatures = Vec::new();

    let import_re = Regex::new(r#"^#include\s+[<"][^>"]+[>"]"#).unwrap();

    let sig_patterns = [
        r"^(?:static\s+)?(?:inline\s+)?(?:virtual\s+)?(?:const\s+)?(?:\w+(?:::\w+)*\s+)+\w+\s*\([^)]*\)[^{;]*",
        r"^class\s+\w+[^{;]*",
        r"^struct\s+\w+[^{;]*",
        r"^enum\s+(?:class\s+)?\w+[^{;]*",
        r"^typedef\s+.+;",
        r"^using\s+\w+\s*=",
        r"^namespace\s+\w+",
        r"^template\s*<[^>]+>",
        r"^#define\s+\w+",
    ];

    for line in content.lines() {
        let trimmed = line.trim();

        if import_re.is_match(trimmed) {
            imports.push(trimmed.to_string());
            continue;
        }

        for pattern in &sig_patterns {
            if let Ok(re) = Regex::new(pattern) {
                if let Some(m) = re.find(trimmed) {
                    let sig = m.as_str().trim_end_matches('{').trim();
                    signatures.push(
                        Signature::from_raw(sig.to_string()).with_type("function".to_string()),
                    );
                    break;
                }
            }
        }
    }

    MappedFile {
        path,
        imports,
        signatures,
        docstrings: None,
        parameters: None,
        return_types: None,
    }
}

/// Extract Ruby skeleton
fn extract_ruby(path: String, content: &str) -> MappedFile {
    let mut imports = Vec::new();
    let mut signatures = Vec::new();

    let import_re =
        Regex::new(r"^(?:require\s+.+|require_relative\s+.+|include\s+\w+|extend\s+\w+)").unwrap();

    let sig_patterns = [
        r"^def\s+(?:self\.)?\w+[^;]*",
        r"^class\s+\w+[^;]*",
        r"^module\s+\w+",
        r"^attr_(?:reader|writer|accessor)\s+.+",
    ];

    for line in content.lines() {
        let trimmed = line.trim();

        if import_re.is_match(trimmed) {
            imports.push(trimmed.to_string());
            continue;
        }

        for pattern in &sig_patterns {
            if let Ok(re) = Regex::new(pattern) {
                if let Some(m) = re.find(trimmed) {
                    signatures.push(Signature::from_raw(m.as_str().to_string()));
                    break;
                }
            }
        }
    }

    MappedFile {
        path,
        imports,
        signatures,
        docstrings: None,
        parameters: None,
        return_types: None,
    }
}

/// Extract PHP skeleton
fn extract_php(path: String, content: &str) -> MappedFile {
    let mut imports = Vec::new();
    let mut signatures = Vec::new();

    let import_re = Regex::new(
        r"^(?:use\s+.+;|namespace\s+.+;|require(?:_once)?\s+.+;|include(?:_once)?\s+.+;)",
    )
    .unwrap();

    let sig_patterns = [
        r"^(?:public|private|protected)?\s*(?:static)?\s*function\s+\w+\s*\([^)]*\)[^{]*",
        r"^(?:abstract\s+)?class\s+\w+[^{]*",
        r"^interface\s+\w+[^{]*",
        r"^trait\s+\w+",
    ];

    for line in content.lines() {
        let trimmed = line.trim();

        if import_re.is_match(trimmed) {
            imports.push(trimmed.to_string());
            continue;
        }

        for pattern in &sig_patterns {
            if let Ok(re) = Regex::new(pattern) {
                if let Some(m) = re.find(trimmed) {
                    let sig = m.as_str().trim_end_matches('{').trim();
                    signatures.push(
                        Signature::from_raw(sig.to_string()).with_type("function".to_string()),
                    );
                    break;
                }
            }
        }
    }

    MappedFile {
        path,
        imports,
        signatures,
        docstrings: None,
        parameters: None,
        return_types: None,
    }
}

/// Generic fallback - just extract obvious patterns
fn extract_generic(path: String, content: &str) -> MappedFile {
    let mut imports = Vec::new();
    let mut signatures = Vec::new();

    let import_re = Regex::new(r"^(?:import|require|include|use)\s+.+").unwrap();
    let sig_re = Regex::new(
        r"^(?:function|def|fn|func|class|struct|interface|type|enum|trait|module)\s+\w+",
    )
    .unwrap();

    for line in content.lines() {
        let trimmed = line.trim();

        if import_re.is_match(trimmed) {
            imports.push(trimmed.to_string());
        } else if let Some(m) = sig_re.find(trimmed) {
            signatures
                .push(Signature::from_raw(m.as_str().to_string()).with_type("unknown".to_string()));
        }
    }

    MappedFile {
        path,
        imports,
        signatures,
        docstrings: None,
        parameters: None,
        return_types: None,
    }
}
