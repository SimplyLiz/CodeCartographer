//! FFI contract tests: every `codecartographer_*` function CKB calls must emit
//! the JSON keys CKB's `internal/cartographer/types.go` decodes. Go's
//! `encoding/json` silently leaves a field at its zero value when the key is
//! missing, so a snake_case key is a wrong answer, not an error.

use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

use crate::*;

// ---------------------------------------------------------------------------
// Fixture
// ---------------------------------------------------------------------------

pub(crate) struct Fixture {
    pub root: PathBuf,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

pub(crate) fn temp_dir(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "carto-{tag}-{}-{nanos}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    // Canonical form: macOS temp dirs live behind the /var -> /private/var link.
    dir.canonicalize().unwrap()
}

fn git(root: &Path, args: &[&str]) {
    let out = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .expect("git");
    assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
}

fn write(root: &Path, rel: &str, content: &str) {
    let p = root.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, content).unwrap();
}

/// A small TypeScript project in a git repo: `ui/view` and `db/my_model`
/// import each other (a cycle and a layer violation), `config` and `util`
/// co-change without an import edge (hidden coupling). `db/my_model.ts` has
/// an underscore so the tests can check that path-valued data and path-keyed
/// maps are never camelCased.
pub(crate) fn fixture() -> Fixture {
    let root = temp_dir("ffi");
    git(&root, &["init", "-q"]);
    git(&root, &["config", "user.email", "t@example.com"]);
    git(&root, &["config", "user.name", "Tester"]);
    git(&root, &["config", "commit.gpgsign", "false"]);

    write(&root, "src/ui/view.ts",
        "import { load } from '../db/my_model';\nexport function render(): string { return load(); }\n");
    write(&root, "src/db/my_model.ts",
        "import { render } from '../ui/view';\nexport function load(): string { return 'x'; }\nexport function unused(): number { return 1; }\n");
    write(&root, "src/config.ts", "export const port = 1;\n");
    write(&root, "src/util.ts", "export function helper(): number { return 2; }\n");
    write(&root, "layers.toml",
        "[layers]\nui = [\"src/ui\"]\ndb = [\"src/db\"]\n\n[allowed_flows]\nui -> db\n");
    git(&root, &["add", "-A"]);
    git(&root, &["commit", "-q", "-m", "initial"]);

    for i in 0..3 {
        write(&root, "src/ui/view.ts", &format!(
            "import {{ load }} from '../db/my_model';\nexport function render(): string {{ return load(); }}\nexport function v{i}(): number {{ return {i}; }}\n"));
        write(&root, "src/db/my_model.ts", &format!(
            "import {{ render }} from '../ui/view';\nexport function load(): string {{ return 'x'; }}\nexport function unused(): number {{ return 1; }}\nexport function m{i}(): number {{ return {i}; }}\n"));
        write(&root, "src/config.ts", &format!("export const port = {i};\n"));
        write(&root, "src/util.ts", &format!("export function helper(): number {{ return {i}; }}\n"));
        git(&root, &["add", "-A"]);
        git(&root, &["commit", "-q", "-m", &format!("change {i}")]);
    }
    Fixture { root }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn cs(s: &str) -> CString {
    CString::new(s).unwrap()
}

/// Decode an FFI response, free it, assert `ok`, return `data`.
fn call(ptr: *mut c_char) -> Value {
    assert!(!ptr.is_null());
    let text = unsafe { CStr::from_ptr(ptr) }.to_str().unwrap().to_string();
    unsafe { codecartographer_free_string(ptr) };
    let v: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(v["ok"], true, "FFI error: {text}");
    v["data"].clone()
}

/// Every object key below `v` must be camelCase (no underscore). Objects
/// under a key in `data_maps` are data maps (paths, extensions) and skipped.
fn assert_camel(v: &Value, data_maps: &[&str], at: &str) {
    match v {
        Value::Object(m) => {
            for (k, child) in m {
                assert!(!k.contains('_'), "snake_case key {at}.{k}");
                if data_maps.contains(&k.as_str()) {
                    continue;
                }
                assert_camel(child, data_maps, &format!("{at}.{k}"));
            }
        }
        Value::Array(a) => a.iter().for_each(|c| assert_camel(c, data_maps, &format!("{at}[]"))),
        _ => {}
    }
}

fn assert_keys(v: &Value, keys: &[&str], at: &str) {
    let m = v.as_object().unwrap_or_else(|| panic!("{at} is not an object: {v}"));
    for k in keys {
        assert!(m.contains_key(*k), "{at} lacks key {k}: {v}");
    }
}

fn first<'a>(v: &'a Value, at: &str) -> &'a Value {
    v.as_array()
        .and_then(|a| a.first())
        .unwrap_or_else(|| panic!("{at} is empty: {v}"))
}

const GO_CONTEXT_HEALTH: &[&str] = &[
    "token_count", "char_count", "window_size", "utilization_pct",
    "metrics", "score", "grade", "warnings", "recommendations",
];
const GO_METRICS: &[&str] = &[
    "signal_density", "compression_density", "position_health",
    "entity_density", "utilization_headroom", "dedup_ratio",
];

// ---------------------------------------------------------------------------
// Tests — one per FFI function CKB calls
// ---------------------------------------------------------------------------

#[test]
fn ffi_contract_all_functions() {
    let fx = fixture();
    let root = cs(fx.root.to_str().unwrap());
    let p = root.as_ptr();

    // map_project
    let d = call(codecartographer_map_project(p));
    assert_camel(&d, &["languages"], "map_project");
    assert_keys(&d, &["nodes", "edges", "cycles", "godModules", "layerViolations", "metadata"], "map_project");
    assert_keys(&d["metadata"], &[
        "totalFiles", "totalEdges", "languages", "generatedAt", "bridgeCount", "cycleCount",
        "godModuleCount", "healthScore", "layerViolationCount", "architecturalDrift",
    ], "metadata");
    assert_keys(first(&d["nodes"], "nodes"), &[
        "moduleId", "path", "language", "signatureCount", "isBridge", "bridgeScore", "degree", "riskLevel",
    ], "node");
    assert_keys(first(&d["edges"], "edges"), &["source", "target", "edgeType", "resolution"], "edge");
    assert_keys(first(&d["cycles"], "cycles"), &["nodes", "pivotNode", "severity"], "cycle");
    assert!(d["metadata"]["healthScore"].is_number());
    assert!(
        d["nodes"].as_array().unwrap().iter().any(|n| n["path"] == "src/db/my_model.ts"),
        "path values must stay verbatim"
    );

    // health
    let d = call(codecartographer_health(p));
    assert_camel(&d, &[], "health");
    assert_keys(&d, &[
        "healthScore", "totalFiles", "totalEdges", "bridgeCount", "cycleCount",
        "godModuleCount", "layerViolationCount",
    ], "health");

    // check_layers
    let layers = cs(fx.root.join("layers.toml").to_str().unwrap());
    let d = call(codecartographer_check_layers(p, layers.as_ptr()));
    assert_camel(&d, &[], "check_layers");
    assert_keys(&d, &["violations", "violationCount"], "check_layers");
    assert_keys(first(&d["violations"], "violations"), &[
        "sourcePath", "targetPath", "sourceLayer", "targetLayer", "violationType", "severity",
    ], "violation");

    // simulate_change
    let module = cs("src/db/my_model.ts");
    let sig = cs("export function added(): void");
    let d = call(codecartographer_simulate_change(p, module.as_ptr(), sig.as_ptr(), std::ptr::null()));
    assert_camel(&d, &[], "simulate_change");
    assert_keys(&d, &["targetModule", "predictedImpact"], "simulate_change");
    assert_keys(&d["predictedImpact"], &[
        "affectedModules", "callersCount", "calleesCount", "willCreateCycle",
        "layerViolations", "riskLevel", "healthImpact",
    ], "predictedImpact");

    // skeleton_map
    let d = call(codecartographer_skeleton_map(p, std::ptr::null()));
    assert_camel(&d, &[], "skeleton_map");
    assert_keys(&d, &["files", "totalFiles", "totalSignatures", "estimatedTokens", "detailLevel"], "skeleton_map");
    assert_keys(first(&d["files"], "files"), &["path", "imports", "signatures"], "skeleton file");

    // module_context
    let view = cs("src/ui/view.ts");
    let d = call(codecartographer_module_context(p, view.as_ptr(), 1));
    assert_camel(&d, &[], "module_context");
    assert_keys(&d, &["module", "dependencies"], "module_context");
    assert_keys(&d["module"], &["path", "imports", "signatures"], "module");
    assert_keys(first(&d["dependencies"], "dependencies"), &["moduleId", "path", "signatureCount"], "dependency");

    // git_churn: a path-keyed map — keys are data and must stay verbatim.
    let d = call(codecartographer_git_churn(p, 0));
    assert!(d.as_object().unwrap().contains_key("src/db/my_model.ts"), "churn keys renamed: {d}");
    assert!(d.as_object().unwrap().values().all(Value::is_number));

    // git_cochange
    let d = call(codecartographer_git_cochange(p, 0, 1));
    assert_camel(&d, &[], "git_cochange");
    assert_keys(first(&d, "git_cochange"), &["fileA", "fileB", "count", "couplingScore"], "pair");

    // hidden_coupling
    let d = call(codecartographer_hidden_coupling(p, 0, 1));
    assert_camel(&d, &[], "hidden_coupling");
    assert_keys(first(&d, "hidden_coupling"), &["fileA", "fileB", "count", "couplingScore"], "pair");

    // semidiff
    let c1 = cs("HEAD~1");
    let d = call(codecartographer_semidiff(p, c1.as_ptr(), std::ptr::null()));
    assert_camel(&d, &[], "semidiff");
    assert_keys(first(&d, "semidiff"), &["path", "status", "added", "removed"], "semidiff file");

    // ranked_skeleton: CKB decodes an object with "files".
    let focus = cs(r#"["src/ui/view.ts"]"#);
    let d = call(codecartographer_ranked_skeleton(p, focus.as_ptr(), 0));
    assert_camel(&d, &[], "ranked_skeleton");
    assert_keys(first(&d["files"], "ranked files"), &[
        "path", "moduleId", "rank", "signatureCount", "estimatedTokens", "role", "signatures",
    ], "ranked file");

    // unreferenced_symbols
    let d = call(codecartographer_unreferenced_symbols(p));
    assert_camel(&d, &[], "unreferenced_symbols");
    assert_keys(&d, &["totalCount", "files"], "unreferenced_symbols");

    // query_context: camelCase envelope, snake_case health (CKB's tags).
    let q = cs("load");
    let d = call(codecartographer_query_context(p, q.as_ptr(), std::ptr::null()));
    assert_keys(&d, &["context", "filesUsed", "focusFiles", "totalTokens", "health"], "query_context");
    assert_keys(&d["health"], GO_CONTEXT_HEALTH, "query_context.health");
    assert_keys(&d["health"]["metrics"], GO_METRICS, "query_context.health.metrics");
    let mut envelope = d.clone();
    envelope.as_object_mut().unwrap().remove("health");
    assert_camel(&envelope, &[], "query_context");

    // shotgun_surgery
    let d = call(codecartographer_shotgun_surgery(p, 0, 1));
    assert_camel(&d, &[], "shotgun_surgery");
    assert_keys(first(&d, "shotgun_surgery"), &[
        "file", "partnerCount", "totalCochanges", "entropy", "dispersionScore",
    ], "shotgun entry");

    // evolution
    let d = call(codecartographer_evolution(p, 0));
    assert_camel(&d, &[], "evolution");
    assert_keys(&d, &["snapshots", "healthTrend", "debtIndicators", "recommendations"], "evolution");
    assert_keys(first(&d["snapshots"], "snapshots"), &["timestamp", "healthScore"], "snapshot");

    // blast_radius
    let d = call(codecartographer_blast_radius(p, view.as_ptr(), 0));
    assert_camel(&d, &[], "blast_radius");
    assert_keys(&d, &["moduleId", "related"], "blast_radius");
    assert_keys(first(&d["related"], "related"), &["moduleId", "path", "relationship"], "related");

    // context_health: stays snake_case — CKB's ContextHealthReport tags are snake_case.
    let content = cs("fn main() {}\npub fn helper(x: u32) -> u32 { x }\n");
    let d = call(codecartographer_context_health(content.as_ptr(), std::ptr::null()));
    assert_keys(&d, GO_CONTEXT_HEALTH, "context_health");
    assert_keys(&d["metrics"], GO_METRICS, "context_health.metrics");

    // render_architecture
    let fmt = cs("mermaid");
    let d = call(codecartographer_render_architecture(p, fmt.as_ptr(), std::ptr::null(), 0, 0));
    assert_camel(&d, &[], "render_architecture");
    assert_keys(&d, &["diagram", "truncated", "format", "nodeCount"], "render_architecture");
}

/// God modules need a hub the small fixture doesn't have; check the struct's
/// FFI encoding directly.
#[test]
fn ffi_god_module_encoding() {
    let g = api::GodModuleInfo {
        module_id: "a".into(),
        path: "a".into(),
        degree: 40,
        cohesion_score: 0.1,
        severity: "HIGH".into(),
    };
    let v = ffi_json::camel(&g);
    assert_keys(&v, &["moduleId", "path", "degree", "cohesionScore", "severity"], "godModule");
    assert_camel(&v, &[], "godModule");
}

/// Timing probe, not a test: `CARTO_PROBE_ROOT=/path cargo test --release --lib
/// probe_timing -- --ignored --nocapture`. Prints the scan size and the wall
/// time of a sequence of FFI calls the way CKB issues them.
#[test]
#[ignore]
fn probe_timing() {
    let Some(root) = std::env::var_os("CARTO_PROBE_ROOT") else {
        return;
    };
    let root = PathBuf::from(root);
    let t = std::time::Instant::now();
    let n = crate::scanner::scan_files_with_noise_tracking(&root).unwrap().files.len();
    eprintln!("scan: {n} files in {:?}", t.elapsed());
    if std::env::var_os("CARTO_PROBE_SCAN_ONLY").is_some() {
        return;
    }
    let c = cs(root.to_str().unwrap());
    let p = c.as_ptr();
    let timed = |name: &str, f: &dyn Fn() -> *mut c_char| {
        let t = std::time::Instant::now();
        let d = call(f());
        eprintln!("{name}: {:?} ({} bytes)", t.elapsed(), d.to_string().len());
    };
    timed("map_project #1", &|| codecartographer_map_project(p));
    timed("map_project #2", &|| codecartographer_map_project(p));
    timed("health", &|| codecartographer_health(p));
    let target = cs("internal/query/engine.go");
    timed("blast_radius", &|| codecartographer_blast_radius(p, target.as_ptr(), 0));
    timed("simulate_change", &|| {
        codecartographer_simulate_change(p, target.as_ptr(), std::ptr::null(), std::ptr::null())
    });
    timed("hidden_coupling", &|| codecartographer_hidden_coupling(p, 0, 0));
    timed("git_cochange", &|| codecartographer_git_cochange(p, 0, 0));
    timed("git_churn", &|| codecartographer_git_churn(p, 0));
}
