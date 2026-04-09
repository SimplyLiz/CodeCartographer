# Cartographer — Architecture Overview

## What is Cartographer?

Cartographer is a code intelligence tool that builds a **semantic map** of your codebase — not full source, but the shape (public APIs, imports, signatures, dependency graph) — and exposes it via CLI, MCP server, and a C FFI consumed by CKB.

**v1.6.0 feature set:**
- Skeleton extraction (regex-based, 8+ languages, 90%+ token savings)
- Full dependency graph with role classification, bridge detection, cycle detection, layer violation checking
- Git history analysis: churn, temporal coupling (co-change), hotspot scoring — bot and formatting commits filtered
- Dead code detection (in-degree=0 nodes that are not entry points)
- Unreferenced public export detection (import-token heuristic)
- Semantic diff (function-level diff between any two commits)
- Diagram export (Mermaid / Graphviz DOT)
- `llms.txt` and `CLAUDE.md` generation for AI context
- `cartographer check` — CI gate, exits non-zero on cycles or layer violations
- Personalized PageRank skeleton (`cartographer context --focus <file> --budget N`)
- MCP server (JSON-RPC 2.0 stdio) for Claude and other LLM integrations
- Context compression via ContextCompressionEngine
- C FFI (`libcartographer.a`) consumed by CKB via CGo

---

## Core Pipeline

```
scan_files_with_noise_tracking()
        │
        ▼
extract_skeleton()  ──── regex-based per-language
        │
        ▼
ApiState.rebuild_graph()
        │
        ├── import resolution → petgraph edges
        ├── Tarjan SCC → cycle detection
        ├── Brandes betweenness → bridge detection
        ├── role classification (entry/core/utility/leaf/dead/bridge)
        └── layer violation checking
        │
        ▼ (optional)
enrich_with_git()   ──── churn × signature_count → hotspot scores
                          co-change pairs (temporal coupling)
```

---

## Module Map

| Module | Responsibility |
|--------|---------------|
| `scanner.rs` | File discovery, noise filtering (.gitignore, .cartographerignore, binary files) |
| `mapper.rs` | Language-specific skeleton extraction; `Signature`, `MappedFile` types |
| `api.rs` | `ApiState`, `rebuild_graph`, `ProjectGraphResponse`; all graph analysis |
| `git_analysis.rs` | `git_churn`, `git_cochange`, `git_show_file`, `git_diff_files` via subprocess |
| `layers.rs` | Architectural layer config (`layers.toml`), violation detection |
| `mcp.rs` | MCP server: JSON-RPC 2.0 stdio transport, 8 tools |
| `memory.rs` | Versioned local memory, incremental hash-based sync |
| `formatter.rs` | Output formatting (XML, Markdown, JSON) |
| `global_config.rs` | `~/.config/cartographer/config.toml` — API key, default target |
| `main.rs` | CLI (`clap`), 20+ commands, 7 git/analysis mode functions |
| `lib.rs` | C FFI (`extern "C"`, `#[no_mangle]`), 8 functions consumed by CKB |

---

## CLI Commands

```bash
# Context generation
cartographer source          # Full skeleton map (default)
cartographer map             # One-shot map (no cloud sync)
cartographer watch           # Live watch + auto-update

# Cloud sync
cartographer init --cloud    # Provision UltraContext project
cartographer push            # Push local map to cloud
cartographer pull            # Pull latest version

# Architectural analysis
cartographer health          # Health score, cycles, bridges, god modules
cartographer dead            # Dead code candidates
cartographer hotspots        # Churn × complexity (top N files)
cartographer cochange        # Temporally coupled file pairs (hidden coupling)
cartographer semidiff HEAD~1 # Function-level diff between two commits
cartographer diagram         # Export graph as Mermaid or DOT
cartographer check           # CI gate: non-zero exit on cycles/violations
cartographer context --focus src/api.rs --budget 8000  # Ranked skeleton
cartographer symbols --unreferenced  # Unreferenced public exports

# AI context generation
cartographer llmstxt         # Generate llms.txt index
cartographer claudemd        # Generate CLAUDE.md architecture guide

# MCP server
cartographer serve           # Start JSON-RPC 2.0 stdio server

# Dependency inspection
cartographer deps <target>   # Show dependencies of a module as JSON
cartographer simulate        # Predict impact of a hypothetical change

# Configuration
cartographer status          # Show tracked files and sync state
cartographer config          # Get/set global config (API key, target)
```

---

## C FFI (lib.rs)

Compiled as `libcartographer.a` (staticlib) + `rlib`. CKB loads via CGo.

**Memory contract:** all output strings are heap-allocated by Rust and **must** be freed by the caller via `cartographer_free_string(ptr)`. Input strings are borrowed (caller owns them). No panics across the FFI boundary — all errors returned as `{"ok":false,"error":"..."}`.

| Function | Inputs | Returns |
|----------|--------|---------|
| `cartographer_free_string(ptr)` | `*mut c_char` | — |
| `cartographer_version()` | — | version string (e.g. `"1.5.0"`) |
| `cartographer_map_project(path)` | path | `ProjectGraphResponse` JSON |
| `cartographer_health(path)` | path | health score + counts JSON |
| `cartographer_check_layers(path, layers_path)` | path, optional layers.toml | violations JSON |
| `cartographer_simulate_change(path, module_id, new_sig, rem_sig)` | path, module, optional sigs | impact JSON |
| `cartographer_skeleton_map(path, detail)` | path, "minimal"/"standard"/"extended" | skeleton JSON |
| `cartographer_module_context(path, module_id, depth)` | path, module, depth | module + deps JSON |
| `cartographer_git_churn(path, limit)` | path, commit limit (0=500) | `{ "file": count }` JSON |
| `cartographer_git_cochange(path, limit, min_count)` | path, limit, min co-changes | `[{fileA,fileB,count,couplingScore}]` JSON |
| `cartographer_semidiff(path, commit1, commit2)` | path, two commit refs | `[{path,status,added[],removed[]}]` JSON |
| `cartographer_ranked_skeleton(path, focus_json, budget)` | path, focus files JSON array, token budget (0=unlimited) | `[{path,moduleId,rank,signatureCount,estimatedTokens,role,signatures}]` JSON |
| `cartographer_unreferenced_symbols(path)` | path | `{totalCount, files:[{path,symbols}]}` JSON |

---

## MCP Tools

Exposed via `cartographer serve` (JSON-RPC 2.0 stdio):

| Tool | Purpose |
|------|---------|
| `map_project` | Full project graph |
| `get_dependencies` | Dependency tree for a module |
| `get_dependents` | Reverse dependencies |
| `get_health` | Health score |
| `get_cycles` | Circular dependency list |
| `get_symbol_context` | Signatures matching a symbol name |
| `get_blast_radius` | Combined deps+dependents up to depth |
| `check_layer_violations` | Architectural layer check |

---

## CKB Integration

Cartographer and CKB operate at complementary levels:

| Aspect | Cartographer | CKB |
|--------|-------------|-----|
| Level | File/module | Symbol |
| Method | Regex skeleton | AST + SCIP index |
| Speed | Fast (milliseconds) | Deep (seconds) |
| Git signals | Churn, co-change, semidiff | — |
| Use case | Quick map, LLM context, hotspots | Deep analysis, refactoring |

**CKB consumes Cartographer via FFI:**
1. `cartographer_map_project()` → graph for navigation and blast-radius pre-filtering
2. `cartographer_git_churn()` + `cartographer_git_cochange()` → hotspot prioritization
3. `cartographer_semidiff()` → semantic context for `reviewPR` / `summarizeDiff`
4. `cartographer_version()` → compatibility gating before any call

---

## Context Compression (CCE)

`compressor.py` integrates [ContextCompressionEngine](https://github.com/SimplyLiz/ContextCompressionEngine) via `tools/cce_bridge.mjs`:

```bash
# Compress a conversation to fit 8k tokens
python compressor.py --messages chat.json --token-budget 8000

# Add cartographer context + compress
python compressor.py src/api.rs --messages chat.json --token-budget 8000
```

CCE preserves code blocks verbatim, summarises prose, and is fully reversible via a verbatim store. `launch.py` auto-builds CCE during installation.
