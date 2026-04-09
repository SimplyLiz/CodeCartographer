# Cartographer — Implementation Status

All planned features are complete as of v1.6.0.

---

## ✅ Completed

### Core Infrastructure
- [x] Regex skeleton extraction — JS/TS, Rust, Python, Go, Java/Kotlin/Scala, C/C++, Ruby, PHP
- [x] `DetailLevel` enum (Minimal / Standard / Extended) with docstrings, return types, parameters
- [x] Symbol-level context retrieval (`get_symbol_context` MCP tool)
- [x] "Blast radius" context (`get_blast_radius` MCP tool: callers + callees up to depth)
- [x] Versioned local memory with hash-based incremental sync
- [x] Background file watching with debounce (`notify`)
- [x] Cloud sync (push/pull to UltraContext)
- [x] Agent management (Cursor, Copilot, Claude)

### Architectural Analysis
- [x] Dependency graph (petgraph) with import resolution
- [x] Cycle detection (Tarjan SCC)
- [x] Bridge detection (Brandes betweenness centrality)
- [x] God module detection
- [x] Layer violation checking (`layers.toml`)
- [x] Predictive impact simulation
- [x] Architectural health score
- [x] **Role classification** — entry / core / utility / leaf / dead / bridge / standard per node
- [x] **Dead code detection** — in-degree=0 nodes, excluding entry points and test files

### Git History Analysis
- [x] **`git_churn`** — per-file commit count over N commits
- [x] **`git_cochange`** — temporal coupling pairs (Adam Tornhill formula: `count / min(churn_a, churn_b)`)
- [x] **Hotspot scoring** — churn × signature_count, normalised 0–100
- [x] **`git_diff_files`** — file-level diff between two commits
- [x] **`git_show_file`** — file contents at a given commit
- [x] **Semantic diff** — function-level diff using skeleton extraction at two revisions
- [x] **Bot-author filtering** — commits from bots/automation excluded from all git metrics
- [x] **Formatting-commit filtering** — prettier/rustfmt/eslint-only commits excluded from all git metrics

### Output / Export
- [x] Mermaid diagram export (role-based node colouring)
- [x] Graphviz DOT export
- [x] `llms.txt` generation (entry points first, sorted by symbol count)
- [x] `CLAUDE.md` generation (health, entry points, core modules, hotspots, cycles, hidden coupling)

### Integrations
- [x] MCP server — JSON-RPC 2.0 stdio, 8 tools
- [x] C FFI (`libcartographer.a`) — 13 functions for CKB via CGo
  - `cartographer_version()` — compatibility gating
  - `cartographer_git_churn()` — hotspot prioritization for CKB
  - `cartographer_git_cochange()` — hidden coupling for CKB
  - `cartographer_semidiff()` — semantic context for `reviewPR` / `summarizeDiff`
  - `cartographer_ranked_skeleton()` — token-budget-aware context via personalized PageRank
  - `cartographer_unreferenced_symbols()` — unreferenced public export detection
- [x] CCE context compression — `compressor.py` + `tools/cce_bridge.mjs`
- [x] `launch.py` — cross-platform installer (Rust + Node + CCE)
- [x] Global config (`~/.config/cartographer/config.toml`)
- [x] Per-repo `.cartographerignore`
- [x] Webhook notifications
- [x] **`cartographer check`** — CI gate, exits non-zero on cycles or layer violations
- [x] **`cartographer context`** — ranked skeleton pruned to token budget (personalized PageRank)
- [x] **`cartographer symbols --unreferenced`** — unreferenced public export detection

---

## Deferred

| Feature | Reason |
|---------|--------|
| Tree-sitter skeleton extraction | Full rewrite of `mapper.rs` + ~15 grammar crates; separate PR |
| Hybrid BM25 + embedding search | Needs local model (bge-small via llama.cpp) + vector store |
| PKG retrieval layer | Context pruning in MCP server — builds on tree-sitter; after that lands |
