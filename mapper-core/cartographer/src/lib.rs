//! C-FFI interface for Cartographer — consumed by CKB via CGo.
//!
//! Every function uses `extern "C"`, takes/returns `*const c_char` (C strings),
//! and never panics across the FFI boundary. Errors are returned as JSON error objects.
//!
//! Memory contract:
//!   - Input strings are borrowed (caller owns them).
//!   - Output strings are allocated by Rust and MUST be freed by the caller
//!     via `cartographer_free_string()`.

use std::collections::HashMap;
use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::path::{Path, PathBuf};

mod api;
mod git_analysis;
mod layers;
mod mapper;
mod scanner;

use api::ApiState;
use mapper::{extract_skeleton, MappedFile};
use scanner::{is_ignored_path, scan_files_with_noise_tracking};

// ---------------------------------------------------------------------------
// Memory management
// ---------------------------------------------------------------------------

/// Free a string returned by any `cartographer_*` function.
///
/// # Safety
/// `ptr` must be a valid pointer returned by a Cartographer FFI function,
/// and must not have been freed already.
#[no_mangle]
pub unsafe extern "C" fn cartographer_free_string(ptr: *mut c_char) {
    if ptr.is_null() {
        return;
    }
    drop(CString::from_raw(ptr));
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn c_str_to_path(s: *const c_char) -> Result<PathBuf, String> {
    if s.is_null() {
        return Err("null path".into());
    }
    let cstr = unsafe { CStr::from_ptr(s) };
    let rust_str = cstr.to_str().map_err(|e| e.to_string())?;
    Ok(PathBuf::from(rust_str))
}

fn result_to_json_ptr<T: serde::Serialize>(result: Result<T, String>) -> *mut c_char {
    let json = match result {
        Ok(value) => serde_json::json!({ "ok": true, "data": value }),
        Err(e) => serde_json::json!({ "ok": false, "error": e }),
    };
    let s = serde_json::to_string(&json)
        .unwrap_or_else(|_| r#"{"ok":false,"error":"serialization failed"}"#.to_string());
    match CString::new(s) {
        Ok(cs) => cs.into_raw(),
        Err(_) => {
            let fallback = CString::new(r#"{"ok":false,"error":"invalid utf8"}"#).unwrap();
            fallback.into_raw()
        }
    }
}

fn build_mapped_files(root: &Path) -> Result<HashMap<String, MappedFile>, String> {
    let scan_result = scan_files_with_noise_tracking(root).map_err(|e| e.to_string())?;

    let result = scan_result
        .files
        .iter()
        .filter(|p| !is_ignored_path(p))
        .filter_map(|p| {
            let content = std::fs::read_to_string(p).ok()?;
            let mapped = extract_skeleton(p, &content);
            let rel = p
                .strip_prefix(root)
                .unwrap_or(p)
                .to_string_lossy()
                .replace('\\', "/");
            Some((rel, mapped))
        })
        .collect();

    Ok(result)
}

// ---------------------------------------------------------------------------
// FFI: Map Project
// ---------------------------------------------------------------------------

/// Scan a project directory and return the full project graph as JSON.
///
/// Input:  `path` — absolute path to project root (C string)
/// Output: JSON string (must be freed with `cartographer_free_string`)
///
/// Response shape:
/// ```json
/// {
///   "ok": true,
///   "data": {
///     "nodes": [...],
///     "edges": [...],
///     "cycles": [...],
///     "godModules": [...],
///     "layerViolations": [...],
///     "metadata": { ... }
///   }
/// }
/// ```
#[no_mangle]
pub extern "C" fn cartographer_map_project(path: *const c_char) -> *mut c_char {
    let path = match c_str_to_path(path) {
        Ok(p) => p,
        Err(e) => return result_to_json_ptr::<serde_json::Value>(Err(e)),
    };

    let mapped_files = match build_mapped_files(&path) {
        Ok(m) => m,
        Err(e) => return result_to_json_ptr::<serde_json::Value>(Err(e)),
    };
    let state = ApiState::new(path.clone());
    {
        let mut files = state.mapped_files.lock().unwrap();
        *files = mapped_files;
    }

    let result = state.rebuild_graph();
    result_to_json_ptr(result)
}

// ---------------------------------------------------------------------------
// FFI: Health Score
// ---------------------------------------------------------------------------

/// Return the architectural health score for a project.
///
/// Response shape:
/// ```json
/// {
///   "ok": true,
///   "data": {
///     "healthScore": 72.5,
///     "totalFiles": 150,
///     "totalEdges": 320,
///     "bridgeCount": 3,
///     "cycleCount": 1,
///     "godModuleCount": 0,
///     "layerViolationCount": 2
///   }
/// }
/// ```
#[no_mangle]
pub extern "C" fn cartographer_health(path: *const c_char) -> *mut c_char {
    let path = match c_str_to_path(path) {
        Ok(p) => p,
        Err(e) => return result_to_json_ptr::<serde_json::Value>(Err(e)),
    };

    let mapped_files = match build_mapped_files(&path) {
        Ok(m) => m,
        Err(e) => return result_to_json_ptr::<serde_json::Value>(Err(e)),
    };
    let state = ApiState::new(path.clone());
    {
        let mut files = state.mapped_files.lock().unwrap();
        *files = mapped_files;
    }

    let graph = match state.rebuild_graph() {
        Ok(g) => g,
        Err(e) => return result_to_json_ptr::<serde_json::Value>(Err(e)),
    };

    let data = serde_json::json!({
        "healthScore": graph.metadata.health_score,
        "totalFiles": graph.metadata.total_files,
        "totalEdges": graph.metadata.total_edges,
        "bridgeCount": graph.metadata.bridge_count,
        "cycleCount": graph.metadata.cycle_count,
        "godModuleCount": graph.metadata.god_module_count,
        "layerViolationCount": graph.metadata.layer_violation_count,
    });

    result_to_json_ptr::<serde_json::Value>(Ok(data))
}

// ---------------------------------------------------------------------------
// FFI: Layer Violations
// ---------------------------------------------------------------------------

/// Check a project against a `layers.toml` config file.
///
/// Inputs:
///   `path`        — project root
///   `layers_path` — path to layers.toml (C string, may be null for defaults)
///
/// Response shape:
/// ```json
/// {
///   "ok": true,
///   "data": {
///     "violations": [
///       {
///         "sourcePath": "src/ui/button.ts",
///         "targetPath": "src/db/model.ts",
///         "sourceLayer": "ui",
///         "targetLayer": "db",
///         "violationType": "skip_call",
///         "severity": "HIGH"
///       }
///     ],
///     "violationCount": 1
///   }
/// }
/// ```
#[no_mangle]
pub extern "C" fn cartographer_check_layers(
    path: *const c_char,
    layers_path: *const c_char,
) -> *mut c_char {
    let path = match c_str_to_path(path) {
        Ok(p) => p,
        Err(e) => return result_to_json_ptr::<serde_json::Value>(Err(e)),
    };

    let config = if !layers_path.is_null() {
        let lp = match c_str_to_path(layers_path) {
            Ok(p) => p,
            Err(e) => return result_to_json_ptr::<serde_json::Value>(Err(e)),
        };
        match layers::LayerConfig::from_file(&lp) {
            Ok(c) => c,
            Err(e) => return result_to_json_ptr::<serde_json::Value>(Err(e)),
        }
    } else {
        layers::LayerConfig::default()
    };

    let mapped_files = match build_mapped_files(&path) {
        Ok(m) => m,
        Err(e) => return result_to_json_ptr::<serde_json::Value>(Err(e)),
    };
    let state = ApiState::new(path.clone());
    {
        let mut files = state.mapped_files.lock().unwrap();
        *files = mapped_files;
    }

    let graph = match state.rebuild_graph() {
        Ok(g) => g,
        Err(e) => return result_to_json_ptr::<serde_json::Value>(Err(e)),
    };

    let edge_tuples: Vec<(String, String)> = graph
        .edges
        .iter()
        .map(|e| (e.source.clone(), e.target.clone()))
        .collect();

    let violations = layers::detect_layer_violations(&edge_tuples, &config);

    let data = serde_json::json!({
        "violations": violations,
        "violationCount": violations.len(),
    });

    result_to_json_ptr::<serde_json::Value>(Ok(data))
}

// ---------------------------------------------------------------------------
// FFI: Simulate Change
// ---------------------------------------------------------------------------

/// Predict the architectural impact of changing a module.
///
/// Inputs:
///   `path`              — project root
///   `module_id`         — module path (relative to root)
///   `new_signature`     — optional new signature (may be null)
///   `remove_signature`  — optional signature to remove (may be null)
///
/// Response shape:
/// ```json
/// {
///   "ok": true,
///   "data": {
///     "targetModule": "src/auth/user.rs",
///     "predictedImpact": {
///       "affectedModules": ["src/api/handler.rs", "src/main.rs"],
///       "callersCount": 5,
///       "calleesCount": 2,
///       "willCreateCycle": false,
///       "layerViolations": [],
///       "riskLevel": "MEDIUM",
///       "healthImpact": -2.0
///     }
///   }
/// }
/// ```
#[no_mangle]
pub extern "C" fn cartographer_simulate_change(
    path: *const c_char,
    module_id: *const c_char,
    new_signature: *const c_char,
    remove_signature: *const c_char,
) -> *mut c_char {
    let path = match c_str_to_path(path) {
        Ok(p) => p,
        Err(e) => return result_to_json_ptr::<serde_json::Value>(Err(e)),
    };

    if module_id.is_null() {
        return result_to_json_ptr::<serde_json::Value>(Err("null module_id".into()));
    }

    let mod_id = unsafe {
        match CStr::from_ptr(module_id).to_str() {
            Ok(s) => s.to_string(),
            Err(e) => return result_to_json_ptr::<serde_json::Value>(Err(e.to_string())),
        }
    };

    let new_sig = if !new_signature.is_null() {
        let s = unsafe {
            match CStr::from_ptr(new_signature).to_str() {
                Ok(s) => s,
                Err(e) => return result_to_json_ptr::<serde_json::Value>(Err(e.to_string())),
            }
        };
        Some(s)
    } else {
        None
    };

    let rem_sig = if !remove_signature.is_null() {
        let s = unsafe {
            match CStr::from_ptr(remove_signature).to_str() {
                Ok(s) => s,
                Err(e) => return result_to_json_ptr::<serde_json::Value>(Err(e.to_string())),
            }
        };
        Some(s)
    } else {
        None
    };

    let mapped_files = match build_mapped_files(&path) {
        Ok(m) => m,
        Err(e) => return result_to_json_ptr::<serde_json::Value>(Err(e)),
    };
    let state = ApiState::new(path.clone());
    {
        let mut files = state.mapped_files.lock().unwrap();
        *files = mapped_files;
    }

    let result = state.simulate_change(&mod_id, new_sig, rem_sig);

    result_to_json_ptr(result)
}

// ---------------------------------------------------------------------------
// FFI: Skeleton Map (token-optimized)
// ---------------------------------------------------------------------------

/// Return a compressed skeleton map of the project for LLM context injection.
///
/// Input:
///   `path`        — project root
///   `detail`      — "minimal", "standard", or "extended" (may be null → standard)
///
/// Response shape:
/// ```json
/// {
///   "ok": true,
///   "data": {
///     "files": [
///       {
///         "path": "src/auth/user.rs",
///         "imports": ["std::collections::HashMap"],
///         "signatures": ["pub fn authenticate(...) -> User"]
///       }
///     ],
///     "totalFiles": 150,
///     "totalSignatures": 2300,
///     "estimatedTokens": 4500
///   }
/// }
/// ```
#[no_mangle]
pub extern "C" fn cartographer_skeleton_map(
    path: *const c_char,
    detail: *const c_char,
) -> *mut c_char {
    let path = match c_str_to_path(path) {
        Ok(p) => p,
        Err(e) => return result_to_json_ptr::<serde_json::Value>(Err(e)),
    };

    let detail_level = if !detail.is_null() {
        let d = unsafe { CStr::from_ptr(detail).to_str().unwrap_or("standard") };
        match d {
            "minimal" => mapper::DetailLevel::Minimal,
            "extended" => mapper::DetailLevel::Extended,
            _ => mapper::DetailLevel::Standard,
        }
    } else {
        mapper::DetailLevel::Standard
    };

    let mapped_files = match build_mapped_files(&path) {
        Ok(m) => m,
        Err(e) => return result_to_json_ptr::<serde_json::Value>(Err(e)),
    };

    let mut total_sigs = 0;
    let files: Vec<serde_json::Value> = mapped_files
        .values()
        .map(|f| {
            total_sigs += f.signatures.len();
            let sigs: Vec<_> = f.signatures.iter().map(|s| &s.raw).collect();
            match detail_level {
                mapper::DetailLevel::Minimal => serde_json::json!({
                    "path": f.path,
                    "signatures": sigs,
                }),
                mapper::DetailLevel::Standard => serde_json::json!({
                    "path": f.path,
                    "imports": f.imports,
                    "signatures": sigs,
                }),
                mapper::DetailLevel::Extended => serde_json::json!({
                    "path": f.path,
                    "imports": f.imports,
                    "signatures": sigs,
                    "docstrings": f.docstrings,
                    "returnTypes": f.return_types,
                }),
            }
        })
        .collect();

    let estimated_tokens = total_sigs * 15 + mapped_files.len() * 5;

    let data = serde_json::json!({
        "files": files,
        "totalFiles": mapped_files.len(),
        "totalSignatures": total_sigs,
        "estimatedTokens": estimated_tokens,
        "detailLevel": format!("{detail_level:?}"),
    });

    result_to_json_ptr::<serde_json::Value>(Ok(data))
}

// ---------------------------------------------------------------------------
// FFI: Module Context (single module with dependencies)
// ---------------------------------------------------------------------------

/// Get skeleton context for a single module with optional dependency depth.
///
/// Inputs:
///   `path`      — project root
///   `module_id` — relative file path
///   `depth`     — dependency traversal depth (0 = none)
///
/// Response shape:
/// ```json
/// {
///   "ok": true,
///   "data": {
///     "module": { "path": "...", "imports": [...], "signatures": [...] },
///     "dependencies": [
///       { "moduleId": "...", "path": "...", "signatureCount": 12 }
///     ]
///   }
/// }
/// ```
#[no_mangle]
pub extern "C" fn cartographer_module_context(
    path: *const c_char,
    module_id: *const c_char,
    depth: u32,
) -> *mut c_char {
    let path = match c_str_to_path(path) {
        Ok(p) => p,
        Err(e) => return result_to_json_ptr::<serde_json::Value>(Err(e)),
    };

    if module_id.is_null() {
        return result_to_json_ptr::<serde_json::Value>(Err("null module_id".into()));
    }

    let mod_id = unsafe {
        match CStr::from_ptr(module_id).to_str() {
            Ok(s) => s.to_string(),
            Err(e) => return result_to_json_ptr::<serde_json::Value>(Err(e.to_string())),
        }
    };

    let mapped_files = match build_mapped_files(&path) {
        Ok(m) => m,
        Err(e) => return result_to_json_ptr::<serde_json::Value>(Err(e)),
    };
    let state = ApiState::new(path.clone());
    {
        let mut files = state.mapped_files.lock().unwrap();
        *files = mapped_files;
    }

    if let Err(e) = state.rebuild_graph() {
        return result_to_json_ptr::<serde_json::Value>(Err(e));
    }

    let module = state
        .mapped_files
        .lock()
        .unwrap()
        .get(&mod_id)
        .cloned()
        .ok_or_else(|| format!("Module not found: {}", mod_id));

    let module = match module {
        Ok(m) => m,
        Err(e) => return result_to_json_ptr::<serde_json::Value>(Err(e)),
    };

    let deps = match state.get_dependencies_internal(&mod_id, depth) {
        Ok(d) => d,
        Err(e) => return result_to_json_ptr::<serde_json::Value>(Err(e)),
    };

    let data = serde_json::json!({
        "module": {
            "path": module.path,
            "imports": module.imports,
            "signatures": module.signatures.iter().map(|s| &s.raw).collect::<Vec<_>>(),
        },
        "dependencies": deps,
    });

    result_to_json_ptr::<serde_json::Value>(Ok(data))
}

// ---------------------------------------------------------------------------
// FFI: Version
// ---------------------------------------------------------------------------

/// Return the Cartographer library version string (e.g. "9.0.0").
///
/// Output: raw C string — must be freed with `cartographer_free_string`.
#[no_mangle]
pub extern "C" fn cartographer_version() -> *mut c_char {
    let version = env!("CARGO_PKG_VERSION");
    match CString::new(version) {
        Ok(cs) => cs.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}

// ---------------------------------------------------------------------------
// FFI: Git Churn
// ---------------------------------------------------------------------------

/// Return per-file commit counts over the last `limit` commits.
///
/// Inputs:
///   `path`  — project root (C string)
///   `limit` — number of commits to analyse (0 → 500)
///
/// Response shape:
/// ```json
/// {
///   "ok": true,
///   "data": {
///     "src/api.rs": 42,
///     "src/main.rs": 18
///   }
/// }
/// ```
/// Returns an empty object when the directory is not a git repo.
#[no_mangle]
pub extern "C" fn cartographer_git_churn(path: *const c_char, limit: u32) -> *mut c_char {
    let path = match c_str_to_path(path) {
        Ok(p) => p,
        Err(e) => return result_to_json_ptr::<serde_json::Value>(Err(e)),
    };
    let limit = if limit == 0 { 500 } else { limit as usize };
    let churn = git_analysis::git_churn(&path, limit);
    result_to_json_ptr::<std::collections::HashMap<String, usize>>(Ok(churn))
}

// ---------------------------------------------------------------------------
// FFI: Git Co-change
// ---------------------------------------------------------------------------

/// Return temporally coupled file pairs from the last `limit` commits.
///
/// Inputs:
///   `path`      — project root (C string)
///   `limit`     — number of commits to analyse (0 → 500)
///   `min_count` — minimum co-change count to include (0 → 2)
///
/// Response shape:
/// ```json
/// {
///   "ok": true,
///   "data": [
///     {
///       "fileA": "src/api.rs",
///       "fileB": "src/main.rs",
///       "count": 12,
///       "couplingScore": 0.92
///     }
///   ]
/// }
/// ```
/// Returns an empty array when the directory is not a git repo.
#[no_mangle]
pub extern "C" fn cartographer_git_cochange(
    path: *const c_char,
    limit: u32,
    min_count: u32,
) -> *mut c_char {
    let path = match c_str_to_path(path) {
        Ok(p) => p,
        Err(e) => return result_to_json_ptr::<serde_json::Value>(Err(e)),
    };
    let limit = if limit == 0 { 500 } else { limit as usize };
    let min_count = if min_count == 0 { 2 } else { min_count as usize };

    let pairs: Vec<serde_json::Value> = git_analysis::git_cochange(&path, limit)
        .into_iter()
        .filter(|p| p.count >= min_count)
        .map(|p| {
            serde_json::json!({
                "fileA": p.file_a,
                "fileB": p.file_b,
                "count": p.count,
                "couplingScore": p.coupling_score,
            })
        })
        .collect();

    result_to_json_ptr::<Vec<serde_json::Value>>(Ok(pairs))
}

// ---------------------------------------------------------------------------
// FFI: Semantic Diff
// ---------------------------------------------------------------------------

/// Return a function-level diff between two commits.
///
/// Inputs:
///   `path`    — project root (C string)
///   `commit1` — base commit (C string)
///   `commit2` — target commit (C string; use "HEAD" for latest)
///
/// Response shape:
/// ```json
/// {
///   "ok": true,
///   "data": [
///     {
///       "path": "src/api.rs",
///       "status": "modified",
///       "added": ["pub fn new_handler(...)"],
///       "removed": ["fn old_helper(...)"]
///     },
///     {
///       "path": "src/old.rs",
///       "status": "deleted",
///       "added": [],
///       "removed": ["pub fn foo()", "pub fn bar()"]
///     }
///   ]
/// }
/// ```
#[no_mangle]
pub extern "C" fn cartographer_semidiff(
    path: *const c_char,
    commit1: *const c_char,
    commit2: *const c_char,
) -> *mut c_char {
    let path = match c_str_to_path(path) {
        Ok(p) => p,
        Err(e) => return result_to_json_ptr::<serde_json::Value>(Err(e)),
    };

    let c1 = if commit1.is_null() {
        return result_to_json_ptr::<serde_json::Value>(Err("null commit1".into()));
    } else {
        unsafe {
            match CStr::from_ptr(commit1).to_str() {
                Ok(s) => s.to_string(),
                Err(e) => return result_to_json_ptr::<serde_json::Value>(Err(e.to_string())),
            }
        }
    };

    let c2 = if commit2.is_null() {
        "HEAD".to_string()
    } else {
        unsafe {
            match CStr::from_ptr(commit2).to_str() {
                Ok(s) => s.to_string(),
                Err(e) => return result_to_json_ptr::<serde_json::Value>(Err(e.to_string())),
            }
        }
    };

    let changed = git_analysis::git_diff_files(&path, &c1, &c2);

    let diff: Vec<serde_json::Value> = changed
        .iter()
        .map(|(file_path, status)| {
            let status_str = match status {
                'A' => "added",
                'D' => "deleted",
                _ => "modified",
            };
            let fake_path = std::path::Path::new(file_path);

            let before_sigs: Vec<String> = if *status != 'A' {
                git_analysis::git_show_file(&path, &c1, file_path)
                    .map(|content| {
                        let mf = mapper::extract_skeleton(fake_path, &content);
                        mf.signatures.into_iter().map(|s| s.raw).collect()
                    })
                    .unwrap_or_default()
            } else {
                vec![]
            };

            let after_sigs: Vec<String> = if *status != 'D' {
                git_analysis::git_show_file(&path, &c2, file_path)
                    .map(|content| {
                        let mf = mapper::extract_skeleton(fake_path, &content);
                        mf.signatures.into_iter().map(|s| s.raw).collect()
                    })
                    .unwrap_or_default()
            } else {
                vec![]
            };

            let before_set: std::collections::HashSet<&str> =
                before_sigs.iter().map(|s| s.as_str()).collect();
            let after_set: std::collections::HashSet<&str> =
                after_sigs.iter().map(|s| s.as_str()).collect();

            let added: Vec<&str> = after_sigs
                .iter()
                .filter(|s| !before_set.contains(s.as_str()))
                .map(|s| s.as_str())
                .collect();
            let removed: Vec<&str> = before_sigs
                .iter()
                .filter(|s| !after_set.contains(s.as_str()))
                .map(|s| s.as_str())
                .collect();

            serde_json::json!({
                "path": file_path,
                "status": status_str,
                "added": added,
                "removed": removed,
            })
        })
        .collect();

    result_to_json_ptr::<Vec<serde_json::Value>>(Ok(diff))
}

// ---------------------------------------------------------------------------
// FFI: Hidden Coupling
// ---------------------------------------------------------------------------

/// Return file pairs that co-change frequently but have NO import edge between
/// them — i.e. implicit/hidden coupling that is invisible in the static graph.
///
/// Inputs:
///   `path`      — project root
///   `limit`     — commits to analyse (0 → 500)
///   `min_count` — minimum co-change count to include (0 → 2)
///
/// Response shape: same as `cartographer_git_cochange` (array of CoChangePair).
/// Returns an empty array when the directory is not a git repo.
#[no_mangle]
pub extern "C" fn cartographer_hidden_coupling(
    path: *const c_char,
    limit: u32,
    min_count: u32,
) -> *mut c_char {
    let path = match c_str_to_path(path) {
        Ok(p) => p,
        Err(e) => return result_to_json_ptr::<serde_json::Value>(Err(e)),
    };
    let limit = if limit == 0 { 500 } else { limit as usize };
    let min_count = if min_count == 0 { 2 } else { min_count as usize };

    // Build the static import-edge set from the dependency graph.
    let scan_result = match scan_files_with_noise_tracking(&path) {
        Ok(r) => r,
        Err(e) => return result_to_json_ptr::<serde_json::Value>(Err(e.to_string())),
    };
    let mapped: std::collections::HashMap<String, MappedFile> = scan_result
        .files
        .iter()
        .filter(|p| !is_ignored_path(p))
        .filter_map(|p| {
            let content = std::fs::read_to_string(p).ok()?;
            let mapped = extract_skeleton(p, &content);
            let rel = p
                .strip_prefix(&path)
                .unwrap_or(p)
                .to_string_lossy()
                .replace('\\', "/");
            Some((rel, mapped))
        })
        .collect();

    let state = ApiState::new(path.clone());
    {
        let mut files = state.mapped_files.lock().unwrap();
        *files = mapped;
    }

    // Normalise: store both (a,b) and (b,a) so lookup is direction-agnostic.
    let import_edges: std::collections::HashSet<(String, String)> =
        match state.rebuild_graph() {
            Ok(graph) => graph
                .edges
                .iter()
                .flat_map(|e| {
                    [
                        (e.source.clone(), e.target.clone()),
                        (e.target.clone(), e.source.clone()),
                    ]
                })
                .collect(),
            Err(_) => std::collections::HashSet::new(),
        };

    // Keep only pairs with no import edge — those are the hidden coupling.
    let pairs: Vec<serde_json::Value> = git_analysis::git_cochange(&path, limit)
        .into_iter()
        .filter(|p| p.count >= min_count)
        .filter(|p| !import_edges.contains(&(p.file_a.clone(), p.file_b.clone())))
        .map(|p| {
            serde_json::json!({
                "fileA": p.file_a,
                "fileB": p.file_b,
                "count": p.count,
                "couplingScore": p.coupling_score,
            })
        })
        .collect();

    result_to_json_ptr::<Vec<serde_json::Value>>(Ok(pairs))
}

// ---------------------------------------------------------------------------
// FFI: Ranked Skeleton (personalized PageRank, token-budget-aware)
// ---------------------------------------------------------------------------

/// Return a token-budget-aware ranked skeleton using personalized PageRank.
///
/// Inputs:
///   `path`       — project root (C string)
///   `focus_json` — JSON array of focus file paths for personalization (C string, may be null/empty)
///   `budget`     — max tokens to include (0 = unlimited)
///
/// Response shape:
/// ```json
/// {
///   "ok": true,
///   "data": [
///     {
///       "path": "src/api.rs",
///       "moduleId": "src/api.rs",
///       "rank": 0.0842,
///       "signatureCount": 45,
///       "estimatedTokens": 680,
///       "role": "core",
///       "signatures": ["pub fn rebuild_graph(...) -> ...", "..."]
///     }
///   ]
/// }
/// ```
#[no_mangle]
pub extern "C" fn cartographer_ranked_skeleton(
    path: *const c_char,
    focus_json: *const c_char,
    budget: u32,
) -> *mut c_char {
    let path = match c_str_to_path(path) {
        Ok(p) => p,
        Err(e) => return result_to_json_ptr::<serde_json::Value>(Err(e)),
    };

    let focus: Vec<String> = if !focus_json.is_null() {
        let s = unsafe {
            match CStr::from_ptr(focus_json).to_str() {
                Ok(s) => s,
                Err(_) => "",
            }
        };
        serde_json::from_str(s).unwrap_or_default()
    } else {
        vec![]
    };

    let mapped_files = match build_mapped_files(&path) {
        Ok(m) => m,
        Err(e) => return result_to_json_ptr::<serde_json::Value>(Err(e)),
    };
    let state = ApiState::new(path.clone());
    {
        let mut files = state.mapped_files.lock().unwrap();
        *files = mapped_files;
    }

    if let Err(e) = state.rebuild_graph() {
        return result_to_json_ptr::<serde_json::Value>(Err(e));
    }

    let ranked = match state.ranked_skeleton(&focus, budget as usize) {
        Ok(r) => r,
        Err(e) => return result_to_json_ptr::<serde_json::Value>(Err(e)),
    };

    let data: Vec<serde_json::Value> = ranked
        .into_iter()
        .map(|f| serde_json::json!({
            "path": f.path,
            "moduleId": f.module_id,
            "rank": f.rank,
            "signatureCount": f.signature_count,
            "estimatedTokens": f.estimated_tokens,
            "role": f.role,
            "signatures": f.signatures,
        }))
        .collect();

    result_to_json_ptr::<Vec<serde_json::Value>>(Ok(data))
}

// ---------------------------------------------------------------------------
// FFI: Unreferenced Symbols
// ---------------------------------------------------------------------------

/// Return public symbols that appear unreferenced across the project (heuristic).
///
/// Input:  `path` — project root (C string)
///
/// Response shape:
/// ```json
/// {
///   "ok": true,
///   "data": {
///     "totalCount": 12,
///     "files": [
///       {
///         "path": "src/utils.rs",
///         "symbols": ["pub fn unused_helper(...)", "pub const OLD_VALUE: ..."]
///       }
///     ]
///   }
/// }
/// ```
#[no_mangle]
pub extern "C" fn cartographer_unreferenced_symbols(path: *const c_char) -> *mut c_char {
    let path = match c_str_to_path(path) {
        Ok(p) => p,
        Err(e) => return result_to_json_ptr::<serde_json::Value>(Err(e)),
    };

    let mapped_files = match build_mapped_files(&path) {
        Ok(m) => m,
        Err(e) => return result_to_json_ptr::<serde_json::Value>(Err(e)),
    };
    let state = ApiState::new(path.clone());
    {
        let mut files = state.mapped_files.lock().unwrap();
        *files = mapped_files;
    }

    let graph = match state.rebuild_graph() {
        Ok(g) => g,
        Err(e) => return result_to_json_ptr::<serde_json::Value>(Err(e)),
    };

    let mut total_count = 0usize;
    let files: Vec<serde_json::Value> = graph
        .nodes
        .iter()
        .filter_map(|n| {
            let exports = n.unreferenced_exports.as_ref()?;
            if exports.is_empty() {
                return None;
            }
            total_count += exports.len();
            Some(serde_json::json!({
                "path": n.path,
                "symbols": exports,
            }))
        })
        .collect();

    let data = serde_json::json!({
        "totalCount": total_count,
        "files": files,
    });

    result_to_json_ptr::<serde_json::Value>(Ok(data))
}
