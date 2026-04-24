# Nyx.Navigator — Architecture

## What it is

Nyx.Navigator builds a **semantic map** of a codebase — not full source, but the shape: public API surfaces, imports, symbol kinds, dependency graph, and git history signals. It exposes this map via CLI, MCP server, and a C FFI consumed by CKB.

The extraction is regex-based and intentionally fast. It is not a compiler. The trade-off is deliberate: 30 ms over an entire repo beats 30 minutes of accurate compilation for the use cases Nyx.Navigator serves (LLM context injection, architectural analysis, hotspot detection).

---

## Core pipeline

```
scan_files_with_noise_tracking()
        │  scanner.rs — file discovery, noise/security filtering
        ▼
extract_skeleton()
        │  mapper.rs — per-language regex extraction
        │  produces: MappedFile { imports, signatures: [Signature] }
        ▼
ApiState.rebuild_graph()
        │  api.rs
        ├── import resolution  → petgraph edges
        ├── Tarjan SCC         → cycle detection
        ├── Brandes centrality → bridge detection
        ├── role classification (entry/core/utility/leaf/dead/bridge/standard)
        ├── layer violation checking (layers.toml)
        └── unreferenced export detection (import-token heuristic)
        │
        ▼ (optional, CLI only)
enrich_with_git()
        │  git_analysis.rs
        ├── git_churn  → per-file commit count
        └── git_cochange → temporal coupling pairs → hotspot scores
```

---

## Module map

| Module | Responsibility |
|--------|---------------|
| `scanner.rs` | File discovery, noise filtering, `.navigatorignore`, security blocking |
| `mapper.rs` | Language skeleton extraction dispatcher; `Signature`, `MappedFile`, `SymbolKind` |
| `extractor.rs` | Tree-sitter extraction (Tier 2, confidence=60) for Rust/Go/Python/TS/JS; called by `mapper.rs` after regex pass |
| `api.rs` | `ApiState`, `rebuild_graph`, import resolution, all graph analysis |
| `git_analysis.rs` | `git_churn`, `git_cochange`, `git_show_file`, `git_diff_files` via subprocess |
| `layers.rs` | Architectural layer config (`layers.toml`), violation detection |
| `search.rs` | Content search (`search_content`, `bm25_search`) and file find (`find_files`) — regex + BM25 + glob, noise-filtered. See [`docs/api/search.md`](api/search.md) |
| `token_metrics.rs` | Context health scoring — 6 research-backed metrics, composite 0–100 score, graded A–F |
| `mcp.rs` | MCP server — JSON-RPC 2.0 stdio transport, 28 tools |
| `lib.rs` | C FFI (`extern "C"`, `#[no_mangle]`), 19 functions consumed by CKB via CGo |
| `memory.rs` | Versioned local memory, incremental hash-based sync |
| `formatter.rs` | Output formatting: XML, Markdown, JSON |
| `global_config.rs` | `~/.config/navigator/config.toml` |
| `main.rs` | CLI (`clap`), all commands and watch mode |

---

## Symbol model (`mapper.rs`)

Nyx.Navigator's symbol extraction follows the [LIP (Linked Incremental Protocol)](../../Protocols/LIP/docs/LIP_SPEC.mdx) taxonomy — designed so the data model is compatible when LIP becomes available, allowing a data-source swap without structural changes.

### `Signature` fields

```rust
pub struct Signature {
    pub raw: String,                    // full signature text, no body
    pub ckb_id: Option<String>,         // LIP URI: lip://local/<path>#<qualified_name>
    pub symbol_name: Option<String>,    // unqualified name: "bar"
    pub qualified_name: Option<String>, // scope-qualified: "Foo.bar"
    pub kind: SymbolKind,               // see taxonomy below
    pub line_start: usize,              // 0-indexed line in source file
    pub confidence: u8,                 // 30 = Tier 1 regex heuristic
    pub doc_comment: Option<String>,    // preceding /// / # / /** comment
}
```

### `SymbolKind` taxonomy

Matches LIP §4.1 with one extension (`Struct`, since Rust/C/Go distinguish structs from classes):

| Kind | Used for |
|------|---------|
| `Function` | Free function (top-level, not inside a class/impl) |
| `Method` | Function inside a class, impl, or trait scope |
| `Class` | Class definition; also impl blocks in Rust |
| `Struct` | Struct definition (Rust, C/C++, Go) |
| `Interface` | Interface, trait (Rust), protocol |
| `Enum` | Enum type |
| `TypeAlias` | `type Foo = ...`, `typedef`, `using` |
| `Variable` | `const`, `static`, `var` |
| `Macro` | `macro_rules!`, `#define` |
| `Namespace` | `namespace`, `mod`, Ruby `module` |
| `Field` | Struct/class field; Ruby `attr_accessor` |
| `Unknown` | Generic fallback |

### LIP symbol URI

`ckb_id` is a LIP-format URI instead of a hash:

```
lip://local/src/api.rs#ApiState.rebuild_graph
lip://local/src/mapper.rs#Signature
lip://local/src/auth.ts#AuthService.verifyToken
```

This makes IDs human-readable, stable across moves within a file, and directly compatible with a future LIP daemon.

### Confidence tiers

| Tier | Score | Source | Languages |
|------|-------|--------|-----------|
| 1 | 30 | Regex heuristic | Java, Kotlin, C/C++, Ruby, PHP, and all other languages |
| 2 | 60 | Tree-sitter CST | Rust, Go, Python, TypeScript, JavaScript |

Tree-sitter extraction (`extractor.rs`) runs after the regex pass in `extract_skeleton()`: it replaces the `signatures` field when `Some` is returned, preserving the regex-extracted `imports`. When LIP is integrated, Tier 3 (compiler-verified, score 51–90) will upgrade these values in-place without changing the data structure.

### Scope tracking

Functions and methods are qualified using their enclosing scope:

- **Rust**: `impl Foo { fn bar }` → `qualified_name: "Foo.bar"`, `kind: Method`
- **Go**: `func (r MyType) Method()` → receiver extracted from signature
- **JS/TS/Java/PHP**: class scope via brace-depth tracker
- **Python**: `self`/`cls` first parameter → Method, qualified with most-recent class

---

## Import resolution (`api.rs`)

Import edges in the dependency graph are resolved with a three-strategy cascade:

1. **Exact stem match** — file stem equals the module name extracted from the import (`use crate::mapper::MappedFile` → look for a file named `mapper.*`)
2. **Path segment match** — file path contains the module stem as a path component (handles `src/utils/helpers.ts` matching `import from './utils/helpers'`)
3. **Symbol-level match** — file that defines a `symbol_name` matching the imported identifier (`useState` → finds `react/index.ts` if it defines `useState`)

This is still a heuristic — not compiler-accurate — but far more reliable than the previous single-word stem comparison.

---

## Git intelligence (`git_analysis.rs`)

All git metrics are computed by shelling out to `git` and parsing stdout. No libgit2 dependency.

| Metric | How |
|--------|-----|
| **Churn** | `git log --name-only` — commit count per file |
| **Co-change** | Jaccard-style coupling: `count / min(churn_a, churn_b)` |
| **Hotspot score** | `churn × signature_count`, normalised 0–100 |
| **Semantic diff** | `git show` at two refs → `extract_skeleton` on both → diff signatures |

**Filtering**: commits from bots (`[bot]`, `dependabot`, CI authors) and formatting-only commits (prettier/rustfmt/eslint, zero functional diff) are excluded from all metrics.

---

## C FFI (`lib.rs`)

Compiled as `libnavigator.a` (staticlib). CKB loads via CGo.

Memory contract: all output strings are heap-allocated by Rust and **must** be freed by the caller via `navigator_free_string(ptr)`. Input strings are borrowed. No panics across the FFI boundary — all errors returned as `{"ok":false,"error":"..."}`.

| Function | Returns |
|----------|---------|
| `navigator_free_string(ptr)` | — |
| `navigator_version()` | version string |
| `navigator_map_project(path)` | `ProjectGraphResponse` JSON |
| `navigator_health(path)` | health score + counts |
| `navigator_check_layers(path, layers_path)` | violations JSON |
| `navigator_simulate_change(path, module_id, new_sig, rem_sig)` | impact JSON |
| `navigator_skeleton_map(path, detail)` | skeleton JSON |
| `navigator_module_context(path, module_id, depth)` | module + deps JSON |
| `navigator_git_churn(path, limit)` | `{ "file": count }` |
| `navigator_git_cochange(path, limit, min_count)` | `[{fileA,fileB,count,couplingScore}]` |
| `navigator_semidiff(path, commit1, commit2)` | `[{path,status,added[],removed[]}]` |
| `navigator_hidden_coupling(path, limit, min_count)` | co-change pairs without import edge |
| `navigator_ranked_skeleton(path, focus_json, budget)` | PageRank-ordered skeleton |
| `navigator_unreferenced_symbols(path)` | `{totalCount, files:[{path,symbols}]}` |
| `navigator_search_content(path, pattern, opts_json)` | grep-like search results |
| `navigator_find_files(path, pattern, limit, opts_json)` | glob file discovery |
| `navigator_replace_content(path, pattern, replacement, opts_json)` | sed-like find-and-replace; supports dry-run + diff |
| `navigator_extract_content(path, pattern, opts_json)` | awk-like capture-group extraction; count/dedup/sort |
| `navigator_bm25_search(path, query, opts_json)` | BM25 ranked file retrieval for natural language queries |
| `navigator_query_context(path, query, opts_json)` | Full PKG pipeline: BM25+regex → PageRank → health → ready bundle |
| `navigator_shotgun_surgery(path, limit, min_partners)` | Co-change dispersion — shotgun surgery candidates ranked by entropy |

---

## MCP server (`mcp.rs`)

Exposed via `navigator serve` — JSON-RPC 2.0 over stdio. 30 tools covering the full FFI surface.

### Graph & architecture

| Tool | Purpose |
|------|---------|
| `get_project_graph` | Full dependency graph |
| `get_dependencies` | Dependency tree for a module |
| `get_dependents` | Reverse dependencies |
| `get_blast_radius` | Deps + dependents (change impact) |
| `get_health` | Health score + counts |
| `get_cycles` | Circular dependencies with pivot suggestions |
| `check_layers` | Layer violation detection (`layers.toml`) |
| `unreferenced_symbols` | Dead-code candidates |
| `simulate_change` | Predict impact of modifying a module |

### Context / skeleton

| Tool | Purpose |
|------|---------|
| `get_module_context` | Public API surface of a single module |
| `get_symbol_context` | Signatures matching a symbol name |
| `skeleton_map` | Compressed skeleton of all files (imports + signatures) |
| `ranked_skeleton` | PageRank-ordered skeleton within a token budget |

### Git intelligence

| Tool | Purpose |
|------|---------|
| `git_churn` | Per-file commit counts (hotspot signal) |
| `git_cochange` | Temporally coupled file pairs |
| `hidden_coupling` | Co-change pairs with no import edge |
| `semidiff` | Function-level diff between two commits |
| `get_evolution` | Health trend + debt indicators over time |
| `poll_changes` | Files modified since an epoch-ms timestamp |

### Search & editing

| Tool | Purpose |
|------|---------|
| `search_content` | Grep-like text/regex search across files |
| `find_files` | Glob file discovery |
| `replace_content` | Sed-like find-and-replace (supports dry-run) |
| `extract_content` | Awk-like capture-group extraction |

### Utility

| Tool | Purpose |
|------|---------|
| `search_project` | Search graph nodes/edges by pattern |
| `watch_status` | Check for changes since last watch cycle |
| `set_compression_level` | Configure response detail level |

---

## CKB integration

Nyx.Navigator and CKB operate at complementary layers:

| Aspect | Nyx.Navigator | CKB |
|--------|-------------|-----|
| Level | File / module | Symbol |
| Method | Regex skeleton | AST + code graph |
| Speed | < 100 ms (whole repo) | Seconds |
| Git signals | Churn, co-change, semidiff | — |
| Symbol model | Heuristic (Tier 1, confidence=30) | Compiler-accurate |

**CKB consumes Nyx.Navigator via FFI:**
1. `navigator_map_project()` → graph for navigation and blast-radius pre-filtering
2. `navigator_git_churn()` + `navigator_git_cochange()` → hotspot prioritization
3. `navigator_semidiff()` → semantic context for `reviewPR` / `summarizeDiff`
4. `navigator_ranked_skeleton()` → token-budget-aware context
5. `navigator_version()` → compatibility gating before any call

---

## Design boundaries

**Stays in Nyx.Navigator permanently** (not replaced by LIP):
- Git temporal coupling — LIP is file-state-aware, not git-history-aware
- Architectural layer violation detection (`layers.toml`)
- God module / cycle detection (Petgraph)
- Context compression and LLM-oriented output formats
- Noise filtering and security blocking
- FFI / MCP interface layer

**Will be replaced by LIP when available**:
- Tree-sitter extraction → LIP Tier 2/3 (compiler-verified symbols, currently at 60)
- `ckb_id` FNV hash → already replaced with LIP URI scheme
- Import string → definition resolution → LIP reference graph
- `confidence: 60` (tree-sitter) → upgraded to Tier 3 from LIP daemon when available
- Regex fallback path (Java, C/C++, Ruby, etc.) → will be replaced language by language as grammars are added
