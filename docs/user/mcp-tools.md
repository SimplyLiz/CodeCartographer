# MCP Tools

Navigator exposes 30+ tools over the Model Context Protocol (JSON-RPC 2.0, stdio transport). When connected to Claude Code, Cursor, or any MCP-aware client, these tools are available directly in the AI's tool call interface — no copy-pasting required.

## Starting the MCP server

```bash
navigator serve [PATH]
```

Runs on stdio. Register it in your MCP client:

```json
{
  "mcpServers": {
    "navigator": {
      "command": "navigator",
      "args": ["serve"]
    }
  }
}
```

To point the server at a specific project directory:

```json
{
  "mcpServers": {
    "navigator": {
      "command": "navigator",
      "args": ["serve", "/path/to/project"]
    }
  }
}
```

## MCP resources

In addition to tools, the server exposes two readable resources:

| URI | Description |
|-----|-------------|
| `navigator://project-graph` | Full dependency graph as JSON |
| `navigator://module-index` | Index of all modules and their signatures as JSON |

## MCP prompts

| Prompt | Parameters | Description |
|--------|-----------|-------------|
| `analyze_module` | `module_id` | Generates an analysis prompt for a specific module |
| `plan_refactoring` | `module_id`, `goal` | Generates a refactoring planning prompt |

---

## Semantic traversal tools (experimental)

These tools provide AI-optimized context at 1–3% of the token cost of skeleton tools. They trade breadth for precision: instead of the full project skeleton, they return exactly the context needed for a specific symbol or question.

### `reach_symbol`

Semantic graph traversal from a named symbol. Returns a compact context tree with distance-proportional detail: full signature + callee signatures at depth 1, type definitions at depth 2.

```
Parameters:
  symbol        string   — symbol name (e.g. "verify_token" or "Auth::verify_token") (required)
  file?         string   — file path fragment to disambiguate when name appears in multiple files
  depth?        number   — traversal depth (default: 2)
  budget?       number   — token cap; leaf nodes trimmed first (default: 6000)
  includeTests? boolean  — expand test call sites instead of collapsing them (default: false)
  showBody?     boolean  — include the function body of the root symbol, up to 40 lines (default: false)
```

**What you get:** root symbol with signature, production callers with one-line call context (tagged `[handler]`/`[middleware]`/`[entry]`), callees with signatures, depth-2 type definitions. Test callers are collapsed and counted. Private callee functions are surfaced for Rust/Python via call graph.

**Token cost:** 135–500 tokens per symbol vs ~18,000 for `ranked_skeleton` on the same files.

**Disambiguation:** if the symbol name appears in multiple files, the tool returns an error listing all candidates — pass `file` to select one.

### `answer_question`

Question-driven evidence chain. Takes a natural-language question and returns a numbered list of the minimum semantic units that together answer it, in reading order.

```
Parameters:
  question    string   — natural language question (e.g. "how does rate limiting work?") (required)
  maxItems?   number   — maximum evidence items (default: 6)
  budget?     number   — token cap (default: 8000)
  showBody?   boolean  — show function body for top-scored item (default: true)
```

**What you get:** numbered items (types before functions, entry points before internals), inter-item connections annotated (`[uses type #2]`, `[calls #3]`, `[imports from #4]`), role labels (`[core logic]`, `[entry point]`, `[internal]`, `[type]`). Private implementation functions are included when they score above the noise floor.

**Token cost:** 220–560 tokens for a 6-item chain vs ~18,000 for the equivalent `query_context` output.

**Best for:** conceptual questions ("how does X work?", "what is Y used for?"). For targeted per-symbol questions, `reach_symbol` is more precise.

---

## Context and skeleton tools

These are the most-used tools in a daily Claude Code session.

### `skeleton_map`

Full project skeleton — imports and public signatures for every file.

```
Parameters: none (optional: detail: "minimal" | "standard" | "extended")
```

Returns a compressed skeleton of all scanned files. The `detail` level controls how much information is included per symbol. Use `minimal` to maximize how many files fit in the context window; use `extended` to include doc comments.

### `ranked_skeleton`

PageRank-ordered skeleton within a token budget, optionally personalized to a set of focus files.

```
Parameters:
  focus?  string  — JSON array of file paths to personalize ranking
  budget? number  — token budget (default: no limit)
```

Ranks files by PageRank centrality (how many other files depend on them), optionally biased toward the `focus` files and their neighborhoods. Trims to `budget` tokens by dropping least-important files first. Use this when the full `skeleton_map` would exceed the context window.

### `focused_skeleton`

Skeleton for a seed file plus its N-hop import neighborhood.

```
Parameters:
  focus   string  — file path (required)
  depth?  number  — import hops (default: 1)
  detail? string  — "minimal" | "standard" | "extended"
```

Walks outward from `focus` by following imports in both directions (imports and importers) up to `depth` hops. Enriches the result with churn labels (hot/stable) and test coverage markers.

### `diff_skeleton`

Skeleton of files changed between two commits, plus their immediate importers.

```
Parameters:
  from?              string  — git ref (default: HEAD~1)
  to?                string  — git ref (default: HEAD)
  include_importers? boolean — include files that import changed files (default: true)
```

Useful for code review: gives an AI the public-API shape of everything that changed in a commit or PR, plus the callers who may be affected.

### `query_context`

Full pipeline in one call: BM25 + regex search → PageRank → context health check.

```
Parameters:
  query             string  — natural language or code question (required)
  budget?           number  — token budget (default: 8000)
  model?            string  — "claude" | "gpt4" | "llama" etc.
  maxSearchResults? number  — cap on BM25 search results before ranking
```

Returns a ready-to-use context string plus health metadata. This is the highest-level tool — use it when you want Navigator to figure out what context is relevant rather than specifying files manually.

### `search_skeleton`

Skeleton sections for files whose path or symbol names match a keyword. Cheaper than `skeleton_map`, more discoverable than `focused_skeleton` when you know a keyword but not the exact module.

```
Parameters:
  pattern  string  — substring matched against file paths and symbol names (case-insensitive) (required)
  detail?  string  — "minimal" | "standard" | "extended" (default: standard)
  budget?  number  — max tokens (0 = unlimited)
```

### `get_module_context`

Public API surface of a single module.

```
Parameters:
  module_id    string  — file path or module id (required)
  depth?       number  — include transitive imports to this depth
  detail_level? string — "minimal" | "standard" | "extended"
```

### `get_symbol_context`

Signatures matching a specific symbol name within a module.

```
Parameters:
  module_id    string  — file path or module id (required)
  symbol_name  string  — symbol name (required)
  detail_level string  — "minimal" | "standard" | "extended" (required)
```

---

## Graph and architecture tools

### `get_project_graph`

Full dependency graph: nodes are files, edges are imports.

```
Parameters: none
```

### `get_dependencies`

Direct (or transitive) dependencies of a module.

```
Parameters:
  module_id  string  — file path or module id (required)
  depth?     number  — traversal depth (default: 1)
```

### `get_dependents`

All modules that import a given module.

```
Parameters:
  module_id  string  — file path or module id (required)
```

### `get_blast_radius`

Files and symbols affected by changing a module.

```
Parameters:
  target       string  — file path or module id (required)
  max_related  number  — limit on related items (required)
```

Returns the target module, its direct dependencies, and its direct dependents. Also returns `lip_uris` — CKB deep-link URIs for drill-down into compiler-accurate symbol analysis.

### `get_health`

Health score and structural issues.

```
Parameters: none
```

Returns: score 0–100, cycle count, bridge count, god-module count, layer violation count.

### `get_cycles`

All circular dependency cycles.

```
Parameters: none
```

Returns each cycle with its severity rating and a suggested pivot node to break it.

### `check_layers`

Current layer violations against `layers.toml`.

```
Parameters: none
```

Returns violations with source module, target module, source layer, target layer, and severity.

### `unreferenced_symbols`

Public symbols with no callers (dead code candidates).

```
Parameters: none
```

Heuristic only — does not account for runtime dynamism.

### `simulate_change`

Predict architectural impact of adding or removing a signature.

```
Parameters:
  module_id?       string — target file (required unless using git options)
  new_signature?   string — a signature to simulate adding
  remove_signature? string — a signature to simulate removing
```

Returns affected modules, cycle risk, layer violation risk, and health score delta.

### `get_evolution`

Architectural health trend over time.

```
Parameters:
  days?  number  — look-back window (default: 30)
```

### `search_project`

Search graph nodes or edges by pattern.

```
Parameters:
  query       string  — search pattern (required)
  query_type  string  — "node" or "edge" (required)
```

---

## Git intelligence tools

### `git_churn`

Per-file commit counts.

```
Parameters:
  limit?  number  — commit count (default: 500)
```

### `git_cochange`

File pairs with high temporal coupling.

```
Parameters:
  limit?     number  — commit count
  min_count? number  — minimum co-change count (default: 2)
```

### `hidden_coupling`

Co-change pairs that have no import edge.

```
Parameters:
  limit?     number  — commit count
  min_count? number  — minimum co-change count
```

### `semidiff`

Function-level diff: which public signatures were added, removed, or changed.

```
Parameters:
  commit1  string  — first git ref (required)
  commit2  string  — second git ref (required)
```

### `poll_changes`

Files modified since a given epoch-millisecond timestamp.

```
Parameters:
  since_ms?  number  — epoch ms (default: last 60 seconds)
```

### `watch_graph`

Watch a directory for changes; emits NDJSON events.

```
Parameters:
  root          string  — directory to watch (required)
  timeout_secs? number  — maximum watch time (default: 30, max: 300)
```

Events: `file_reindexed`, `graph_updated`. Each event includes the changed file path and updated graph metadata.

### `shotgun_surgery`

Files whose changes scatter across many unrelated modules.

```
Parameters:
  maxResults?  number  — result limit
  minPartners? number  — minimum co-change partners (default: 3)
  commits?     number  — commit count
```

---

## Search and editing tools

### `find_files`

Glob file discovery.

```
Parameters:
  pattern  string  — glob pattern (required)
  limit?   number  — result cap (default: 200)
```

Returns: path, language, size in bytes.

### `search_content`

Grep-like regex search across files.

```
Parameters:
  pattern        string  — regex pattern (required)
  literal?       boolean — treat pattern as literal string
  caseSensitive? boolean
  contextLines?  number  — lines of context around each match
  maxResults?    number
  fileGlob?      string  — restrict to glob
```

### `replace_content`

Regex find-and-replace across files.

```
Parameters:
  pattern        string  — regex pattern (required)
  replacement    string  — replacement string; $0 = full match, $1/$2 = groups (required)
  dryRun?        boolean — preview only, no writes
  literal?       boolean
  caseSensitive? boolean
  fileGlob?      string
  excludeGlob?   string
  searchPath?    string  — restrict to directory
  maxPerFile?    number
  contextLines?  number
```

### `extract_content`

Capture-group extraction.

```
Parameters:
  pattern       string  — regex pattern (required)
  groups?       string  — JSON array of group indices
  count?        boolean — frequency table mode
  dedup?        boolean
  sort?         boolean
  caseSensitive? boolean
  fileGlob?     string
  searchPath?   string
  limit?        number
```

### `search_in_symbol`

Search scoped to a single named function or method body.

```
Parameters:
  file          string  — file path (required)
  symbol        string  — function/method name (required)
  pattern       string  — search pattern (required)
  context_lines? number — context lines (default: 2)
```

---

## Documentation tools

### `doc_index`

All documentation files (Markdown, YAML, TOML, JSON) with headings, config keys, and cross-references.

```
Parameters: none
```

### `doc_context`

A single document's structure plus the skeleton of all code it references.

```
Parameters:
  doc_path  string  — path to the doc file (required)
  budget?   number  — token budget (default: 4000)
```

### `query_docs`

Doc-biased retrieval: searches docs first, follows cross-references into code.

```
Parameters:
  query   string  — question or search term (required)
  budget? number
  model?  string
```

---

## Utility tools

### `context_health`

Score a context bundle on six quality metrics.

```
Parameters:
  content           string  — context string to score (required)
  model?            string  — target model family ("claude", "gpt4", "llama", "gpt35", "custom")
  windowSize?       number  — context window size override
  signatureCount?   number
  signatureTokens?  number
  keyPositions?     string
```

Returns: composite score 0–100, letter grade A–F, and per-metric breakdown.

### `set_compression_level`

Configure response verbosity for this session.

```
Parameters:
  level  string  — "minimal" | "standard" | "aggressive"
```

`aggressive` compression is useful when working in a token-constrained context. `minimal` preserves more detail.

### `list_key_handlers`

Extracts key-binding maps from TUI source files.

```
Parameters:
  file           string  — source file path (required)
  context_lines? number  — context lines per handler (default: 4)
```

Supports Go (Bubble Tea) and Rust (crossterm).

### `map_state_machine`

Correlates state guards with key handlers; produces a state × handlers matrix.

```
Parameters:
  file           string  — source file path (required)
  state_var?     string  — state variable name to track
  state_prefix?  string  — prefix filtering for state constants
  context_lines? number
```

### `watch_status`

Check for changes since the last `navigator watch` cycle.

```
Parameters: none
```
