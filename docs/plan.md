# Nyx.Navigator — Feature Status

---

## Completed

### Core extraction
- [x] Regex skeleton extraction — JS/TS, Rust, Python, Go, Java/Kotlin/Scala, C/C++, Ruby, PHP
- [x] `DetailLevel` enum (Minimal / Standard / Extended)
- [x] Versioned local memory with hash-based incremental sync
- [x] Background file watching with debounce (`notify`)
- [x] Cloud sync (push/pull to UltraContext)

### Symbol model (LIP-aligned)
- [x] `SymbolKind` taxonomy — Function, Method, Class, Struct, Interface, Enum, TypeAlias, Variable, Macro, Namespace, Field (matches LIP §4.1 + Struct extension)
- [x] `line_start` — 0-indexed line number on every signature
- [x] `confidence: u8` — 30 = Tier 1 regex heuristic; ready for LIP Tier 2 upgrade
- [x] `qualified_name` — scope-qualified symbol names (`Foo.bar`) via brace-depth scope tracker
- [x] `doc_comment` — preceding `///` / `#` / `/**` lines attached to each signature
- [x] LIP symbol URI as `ckb_id` — `lip://local/<path>#<qualified_name>` replaces FNV hash

### Import resolution
- [x] Three-strategy cascade: exact stem → path segment → symbol-name match
- [x] Language-aware import parsing: Rust `use`, Python `from … import`, JS/TS `import … from`, Java, `require()`
- [x] Symbol-level match: resolves `import { useState }` to the file that defines `useState`

### Architectural analysis
- [x] Dependency graph (petgraph) with import resolution
- [x] Cycle detection (Tarjan SCC)
- [x] Bridge detection (Brandes betweenness centrality)
- [x] God module detection
- [x] Layer violation checking (`layers.toml`)
- [x] Predictive impact simulation
- [x] Architectural health score
- [x] Role classification — entry / core / utility / leaf / dead / bridge / standard
- [x] Dead code detection — in-degree=0, excluding entry points and test files
- [x] Unreferenced public export detection

### Git history analysis
- [x] `git_churn` — per-file commit count
- [x] `git_cochange` — temporal coupling pairs
- [x] Hotspot scoring — churn × signature_count, normalised 0–100
- [x] Semantic diff — function-level diff between any two commits
- [x] Bot-author filtering
- [x] Formatting-commit filtering
- [x] Hidden coupling detection — co-change pairs with no static import edge

### Output and export
- [x] Mermaid diagram export (role-based node colouring)
- [x] Graphviz DOT export
- [x] `llms.txt` generation
- [x] `CLAUDE.md` generation
- [x] Personalized PageRank skeleton (`navigator context --focus <file> --budget N`)

### Integrations
- [x] MCP server — JSON-RPC 2.0 stdio, 8 tools
- [x] C FFI (`libnavigator.a`) — 16 functions for CKB via CGo
- [x] `navigator check` — CI gate, exits non-zero on cycles or layer violations
- [x] `navigator symbols --unreferenced`
- [x] Global config (`~/.config/navigator/config.toml`)
- [x] Per-repo `.navigatorignore`
- [x] Content search — `navigator search <PATTERN>` + `navigator_search_content` FFI
- [x] File find — `navigator find <GLOB>` + `navigator_find_files` FFI
- [x] Context injection for tool-call-less models — `navigator context --query <PATTERN>` bundles ranked skeleton + search results in one invocation

---

## Deferred

| Feature | Why deferred |
|---------|-------------|
| Tree-sitter extraction | Full rewrite of mapper.rs + ~15 grammar crates; blocked on LIP readiness |
| LIP daemon integration | LIP protocol not yet implemented; data structures are already compatible |
| Hybrid BM25 + embedding search | Needs local model (bge-small) + vector store |
| `confidence` Tier 2 upgrade | Requires LIP Tier 2 (incremental compiler) to be available |
| Cross-file reference graph | Requires LIP Occurrence table; current import resolution is heuristic |
