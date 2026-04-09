# Changelog

All notable changes to Cartographer will be documented in this file.

## [1.5.0] - 2026-04-09

### Added
- **`cartographer_version()`** — FFI function returning the library version string; CKB uses this for compatibility checks before calling any other function
- **`cartographer_git_churn(path, limit)`** — FFI wrapper for git churn analysis; returns `{ "src/api.rs": 42, ... }` (empty object when not a git repo)
- **`cartographer_git_cochange(path, limit, min_count)`** — FFI wrapper for temporal coupling; returns sorted array of `{ fileA, fileB, count, couplingScore }` pairs
- **`cartographer_semidiff(path, commit1, commit2)`** — FFI wrapper for semantic diff; returns per-file `{ path, status, added[], removed[] }` using skeleton extraction at each commit
- `mod git_analysis` added to `lib.rs` — git subprocess helpers are now available to all FFI callers, not just the CLI binary

## [1.4.0] - 2026-04-09

### Added
- **CCE integration** — `compressor.py` now compresses context through [ContextCompressionEngine](https://github.com/SimplyLiz/ContextCompressionEngine), reducing token usage while preserving code verbatim
  - `python compressor.py --messages chat.json --token-budget 8000` compresses any message array to fit a token budget
  - Cartographer dependency context is appended as a system message before compression
  - CCE path auto-discovered via `CCE_DIST` env var, `.cartographer/cce_dist` config, or sibling directory
- **`tools/cce_bridge.mjs`** — thin stdin/stdout Node.js bridge to CCE; normalises messages (adds `id`/`index`), accepts `--cce-dist` flag
- **`launch.py` CCE setup** — steps 5–6 check Node.js 20+ and build CCE; dist path saved to `.cartographer/cce_dist` for `compressor.py` to use
  - `--cce-path <dir>` overrides the default sibling-directory assumption

## [1.3.0] - 2026-04-09

### Added
- **`cochange`** — temporal coupling analysis from git history; surfaces files that always change together without an import link (`cartographer cochange --min-count 3`)
- **`hotspots`** — churn × complexity ranking with CRITICAL / HIGH / MODERATE / LOW tiers (`cartographer hotspots --top 10`)
- **`dead`** — dead code candidates based on in-degree = 0 in the dependency graph (`cartographer dead`)
- **`diagram`** — exports dependency graph as Mermaid or Graphviz DOT with role-based colouring (`cartographer diagram --format mermaid -o graph.md`)
- **`llmstxt`** — generates `llms.txt` index (entry points first, sorted by symbol count) for LLM inference-time context (`cartographer llmstxt`)
- **`claudemd`** — generates a `CLAUDE.md` architecture guide covering entry points, core modules, hotspots, cycles, and hidden coupling (`cartographer claudemd`)
- **`semidiff`** — function-level semantic diff between two commits using skeleton extraction (`cartographer semidiff HEAD~1`)
- **`git_analysis` module** — `git_churn`, `git_cochange`, `git_show_file`, `git_diff_files` helpers (binary-only; not exposed via C FFI)
- **Role classification** — every `GraphNode` now carries `role` (entry / core / utility / leaf / dead / bridge / standard), `churn`, `hotspot_score`, and `is_dead`
- **`CoChangePair`** in `ProjectGraphResponse` — populated by `enrich_with_git()`

## [1.2.0] - 2026-04-09

### Added
- **`launch.py`** — cross-platform Python installer replacing `install.sh`; supports Linux, macOS, and Windows; updates shell RC automatically
- **`deps` command** — `cartographer deps <target> --format json` outputs dependency graph for a target module as JSON
- **`serve` command** — `cartographer serve` starts the MCP server with full JSON-RPC 2.0 stdio transport
- **MCP tools** — `get_symbol_context` (filter signatures by symbol name) and `get_blast_radius` (dependencies + dependents up to depth limit)
- **`#[serde(rename = "type")]`** fix on `McpInputSchema` and `McpProperty` so tool schemas serialise correctly

### Fixed
- `compressor.py` called a non-existent `cmp deps` subcommand; now calls `cartographer deps`
- `verify_ignore.py` hardcoded the old `cmp` binary path; now resolves the correct platform binary
- Stale "architect" branding in `install.sh`

## [1.1.0] - 2025-04-07

### Changed
- Renamed binary from `architect` to `cartographer`
- Updated package description to "Code Cartographer for Architectural Intelligence"

### Added
- LICENSE file (CKB License)

## [1.0.0] - 2025-04-04

### Added
- Initial release as `architect` (formerly `cmp`)
- Graph-based code analysis engine
- Module context generation with dependency mapping
- Git-aware file scanning
- MCP server integration
- Webhook notifications for sync events
- Analytics and agents use cases
- Webhook use case handlers
- Python integration examples
- Shell installation scripts (install.sh, install.ps1)
