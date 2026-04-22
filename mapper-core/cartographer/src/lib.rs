// Add constant at top
const MAX_SKELETON_TOKENS: usize = 200_000; // ~150 KB typical

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

    // GUARD: Estimate tokens and truncate if necessary
    let mut total_tokens = 0usize;
    let mut total_sigs = 0usize;
    let mut files: Vec<serde_json::Value> = Vec::new();

    for (_, f) in &mapped_files {
        // Rough token estimate: 15 per signature + 50 per file
        let file_tokens = f.signatures.len() * 15 + 50;

        if total_tokens + file_tokens > MAX_SKELETON_TOKENS {
            eprintln!(
                "[CARTOGRAPHER] Skeleton map truncated at {} tokens (limit: {})",
                total_tokens, MAX_SKELETON_TOKENS
            );
            break;
        }

        total_tokens += file_tokens;
        total_sigs += f.signatures.len();

        let sigs: Vec<_> = f.signatures.iter().map(|s| &s.raw).collect();
        match detail_level {
            mapper::DetailLevel::Minimal => {
                files.push(serde_json::json!({
                    "path": f.path,
                    "signatures": sigs,
                }));
            }
            mapper::DetailLevel::Standard => {
                files.push(serde_json::json!({
                    "path": f.path,
                    "imports": f.imports,
                    "signatures": sigs,
                }));
            }
            mapper::DetailLevel::Extended => {
                files.push(serde_json::json!({
                    "path": f.path,
                    "imports": f.imports,
                    "signatures": sigs,
                    "docstrings": f.docstrings,
                    "returnTypes": f.return_types,
                }));
            }
        }
    }

    let estimated_tokens = total_sigs * 15 + mapped_files.len() * 5;

    let data = serde_json::json!({
        "files": files,
        "totalFiles": mapped_files.len(),
        "totalSignatures": total_sigs,
        "estimatedTokens": estimated_tokens,
        "detailLevel": format!("{detail_level:?}"),
        "truncated": files.len() < mapped_files.len(),
    });

    result_to_json_ptr::<serde_json::Value>(Ok(data))
}
