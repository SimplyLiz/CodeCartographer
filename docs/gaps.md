# CodeCartographer — Gaps & Improvement Backlog

Findings from a dogfooding session run on the CodeCartographer repo itself. Bugs first, then feature ideas. Check off as you ship.

---

## Bugs

### Critical

- [x] **v1.3.0 binary deadlocks on startup for `health`, `dead`, `diagram`, `evolution`**
  - Single thread pinned in `_pthread_mutex_firstfit_lock_slow → __psynch_mutexwait` (confirmed via `sample <pid>`). Self-deadlock before any work runs.
  - `map`, `status`, `hotspots` work on the same binary.
  - Likely culprits: only `webhooks.rs` and `api.rs` use `Mutex`/`RwLock`.
  - v3.0.0 from source does not repro — ship a fresh binary via `install.sh`.
  - **Resolution**: v3.0.0 does not repro; close by shipping updated binary. Underlying Mutex re-entrance guard documented in api.rs:468-470.

### High

- [x] **`hotspots` / `dead` / `simulate` / `check` treat non-source files as code**
  - `hotspots` ranks `Cargo.toml` (CRITICAL, score 100) and `CHANGELOG.md` (HIGH) above every Rust file.
  - `dead` flags `LICENSE`, `.gitignore`, `.github/workflows/release.yml`, `Cargo.toml`, `SECURITY.md`, `state_key.md`, `requirements.txt`.
  - `simulate --module src/api.rs` reports "14 Direct Callers" — the list is `CHANGELOG.md` ×4, `docs/architecture.md` ×3, `docs/api/search.md` ×2, etc. Risk labeled HIGH on phantom edges from text mentions in markdown.
  - **Single root cause**: edge / in-degree detection runs on every file, no language/role gate.
  - **Resolution**: fixed in b73d5a5 — all four commands now filter by `is_source_file()` (main.rs:2597, 2725, 2275, 3412) before building the file list.

- [x] **`diagram --format mermaid` and `simulate` callers list contain duplicates**
  - In a fresh diagram run: `N0 --> N2` appears 10×, `N3 --> N4` 5×, `N1 --> N4` 4×.
  - `simulate` "Direct Callers: 14" count is inflated by the same duplication.
  - Fix: collapse edge multiset to a set (or carry a weight) before rendering / counting.
  - **Resolution**: `rebuild_graph()` now sort+dedup edges on (source, target) before returning (api.rs:482-484), fixing mermaid, DOT, simulate, and any other consumer in one place.

### Medium

- [x] **`codecartographer <subcommand> [PATH]` rejects path argument**
  - `codecartographer health .` → *"unexpected argument '.' found"*. Same for `hotspots`, `dead`, `diagram`.
  - Top-level help shows `Usage: codecartographer [OPTIONS] [PATH] [COMMAND]` — only the bare form (routing to `map`) actually accepts a path. Misleading.
  - Fix: accept `[PATH]` on every analysis subcommand, OR remove it from top-level usage.
  - **Resolution**: fixed in b73d5a5 — subcommand path arg wired through `resolve_path()` on all analysis commands.

### Low

- [x] **`install.sh` leaves stale binary with no upgrade hint**
  - `codecartographer --version` on a months-old install just prints the old version. No warning, no `codecartographer update`.
  - Add `codecartographer update` or an opt-in version-check on startup.
  - **Resolution**: `codecartographer update` command added. Walks up from the binary location to find `install.sh` and re-runs it. Falls back to a manual-install hint if the script isn't found.

---

## Features

### The single biggest fix

- [x] **Default `.codecartographerignore` + source-role classifier**
  - Closes most of the noise above in one move.
  - Ship defaults: `*.md`, `*.toml`, `*.lock`, `*.yaml`, `*.yml`, `LICENSE*`, `.github/**`, `.gitignore`, generated files, etc.
  - Classify each file by language; gate `hotspots`, `dead`, `simulate`, `check` on `is_source == true`.
  - **Resolution**: `is_source_file()` in scanner.rs:297 and applied in all structural analysis commands — `health`, `evolution`, `hotspots`, `dead`, `simulate`, `check` (b73d5a5), and `diagram` (this session, main.rs:2886). LLM-context commands (`map`, `llmstxt`, `claudemd`, `context`, `symbols`, `query`, MCP) intentionally keep all files. `.codecartographerignore` + `.gitignore` user overrides remain available.

### Closes existing gaps

- [x] **`simulate --diff <ref>` / `simulate --staged`**
  - Today simulate only takes `--new-signature`. Real workflow: predict impact of pending changes already on disk.
  - `codecartographer simulate --staged --fail-on-cycle` for pre-commit hooks.
  - **Resolution**: `--staged`, `--diff <ref>`, and `--fail-on-cycle` added. `--module` is now optional when either diff flag is present. git_changed_source_files() helper runs the appropriate `git diff --name-only` and feeds each changed module through the existing impact analysis.

- [x] **PR health delta**
  - `codecartographer health --compare main` → "Health: 80 → 72 (-8). Reason: +1 cycle, +3 bridges."
  - The interesting number is the change, not the absolute score.
  - **Resolution**: `--compare <ref>` added to `health`. Builds a second graph from `git ls-tree`/`git show` at the ref, then prints a before→after table for score, bridges, cycles, god modules, and layer violations.

- [x] **`layers` subcommand group**
  - `layers.toml` is a headline feature with no tooling.
  - `codecartographer layers init` (auto-propose from imports), `validate`, `diagram`, `suggest`.
  - **Resolution**: All four subcommands implemented. Also fixed a critical bug: `detect_layer_violations` in api.rs was always using an empty default config, so `layers.toml` was never actually loaded — now loads from `./layers.toml` or `./.codecartographer/layers.toml` on every analysis. `init` topologically sorts directories by import in-degree and assigns presentation/domain/service/infrastructure names; `validate` exits 1 on any violation (with `--json`); `diagram` emits a collapsed layer graph in mermaid or dot; `suggest` reports unlayered directories, active violations with remediation hints, and unused allowed_flows entries.

- [x] **Bridge remediation hints**
  - `health` reports "14 bridges" silently. Each bridge deserves a one-line suggestion.
  - Example: *"src/mapper.rs bridges {scanner, extractor, formatter} — 3 of 12 exports used by only one. Split candidate: …"*
  - **Resolution**: `health` now prints a "Bridge Remediation Hints" section with top-5 bridges sorted by score. Each line shows fan-in, fan-out, and a context-aware suggestion: multi-domain callers → "split by domain or introduce a façade"; high fan-out → "extract a sub-layer"; otherwise "may be intentional mediator".

- [x] **Ownership + churn together**
  - `hotspots --by author` / `--bus-factor` columns.
  - Hotspots without ownership are hard to act on.
  - **Resolution**: `--by-author` adds dominant-owner column; `--bus-factor` adds unique-author count (lower = higher risk). New `git_bus_factor()` in `git_analysis.rs` collects per-file unique human author sets. Both columns appear in `--json` output automatically.

- [x] **Test-coverage dimension**
  - `hotspots --untested` filters to high-churn × high-complexity files lacking a sibling test file.
  - Probably the most actionable hotspot variant.
  - **Resolution**: `--untested` flag added. `has_sibling_test()` checks `foo_test.rs`, `test_foo.py`, `foo.test.ts`, `foo.spec.ts` patterns and `tests/`/`__tests__`/`test/` sibling directories.

- [x] **Snapshot + diff of `project_graph.json`**
  - `codecartographer snapshot save v3.0.0` / `snapshot diff v1.3.0 v3.0.0`.
  - Shows module/edge/cycle/bridge deltas between two saved graphs.
  - **Resolution**: `codecartographer snapshot save <TAG>` persists health_score, file/edge/cycle/bridge/god-module/layer-violation counts to `.codecartographer/snapshots/<TAG>.json`. `snapshot diff <TAG1> <TAG2> [--json]` shows a before→after table with deltas. `snapshot list` enumerates saved snapshots.

### New capabilities

- [x] **Edge kinds (type-only, test-only, runtime, macro, conditional)**
  - Currently every edge is "A imports B." Kind-aware edges fix `dead` noise and bridge math.
  - **Resolution**: `edge_type` on `GraphEdge` is now populated as `"runtime"`, `"test"`, or `"doc"` based on the source file path (using existing `is_test_path()` / `is_doc_path()`). Dead-code analysis uses a separate `runtime_in_degree` counter so modules imported only from tests are correctly flagged as dead-code candidates. Bridge math still uses total in-degree.

- [x] **Tree-sitter parser as opt-in**
  - `codecartographer map --parser tree-sitter` for accuracy over speed. Regex misses async/generic/macro corner cases.
  - **Resolution**: Already implemented — `extract_skeleton()` in `mapper.rs` runs regex first for imports, then upgrades signatures via `crate::extractor::ts_extract()` (tree-sitter, Tier 2 confidence) for all supported languages (Rust, Go, Python, TS/JS, C/C++). No flag needed; tree-sitter runs automatically when the feature is compiled in (default).

- [x] **Language-support introspection**
  - `codecartographer languages` lists supported languages.
  - Every command should report `skipped N files (unsupported language)` so users see coverage.
  - **Resolution**: `codecartographer languages [PATH] [--json]` added. Shows file count per language with ASCII bar chart, plus "skipped N non-source files" count.

- [x] **Shortest-path explainer**
  - `codecartographer path src/ui/login.tsx src/db/migrations.rs` → human-readable hop-by-hop chain.
  - Trivial from the graph, exactly the tool people reach for in code review.
  - **Resolution**: `codecartographer path --from <FILE> --to <FILE> [--json]` added. BFS on the directed import graph; accepts repo-relative paths or module IDs with fuzzy suffix matching. Reports hop count and each step.

- [x] **Co-change community detection**
  - `cochange` returns pairs. Cluster them (Louvain) to surface implicit modules vs declared modules → hidden coupling.
  - **Resolution**: `codecartographer cochange --cluster [--threshold 0.5] [--json]` added. Uses union-find on pairs with `coupling_score >= threshold` to form connected components (implicit modules). Output shows each community's size and max coupling score. `--json` emits a stable schema.

- [x] **TODO/FIXME/HACK density heatmap**
  - Cheap, predictive, pairs nicely with hotspots.
  - **Resolution**: `codecartographer todo [PATH] [--top N] [--json]` added. Scans all source files for TODO/FIXME/HACK/XXX/WORKAROUND/NOCOMMIT, reports per-file totals with per-marker breakdown, sorted by density.

### AI-context specific (the differentiation story)

- [x] **`context` should disclose what it dropped**
  - When `--budget 8000` prunes, print a line: *"dropped 12 files (lowest PageRank), trimmed 3 files' signatures."*
  - Lets the human / LLM judge whether to widen budget.
  - **Resolution**: `context_mode` now prints "Dropped: N files (lowest PageRank, exceeded budget)" to stderr whenever budget > 0 and files were excluded.

- [x] **`context --for-task "<description>"`**
  - Combine focus file + task description for ranking (TF-IDF or embeddings over the skeleton).
  - Pure graph centrality ignores prompt intent.
  - **Resolution**: `--for-task "<description>"` added to `context`. Tokenises the description, scores each file by term overlap against its path + signatures (TF-IDF), then blends 60% PageRank + 40% task-overlap before re-sorting. Task description echoed in stderr header.

- [x] **Token-budget regression in CI**
  - `context-health` exists; gate it. Warn when a PR pushes the architecture bundle past target budget.
  - **Resolution**: `--fail-if-over <TOKENS>` added to `context-health`. Exits 1 with a clear message when token count exceeds the threshold. Compose with `codecartographer context | codecartographer context-health --fail-if-over 8000` in CI.

- [x] **Machine-readable everywhere (`--json`)**
  - Most commands emit pretty ANSI tables. For MCP / tool integration, every command needs `--json` with a stable schema.
  - `deps` already does this — extend the pattern.
  - **Resolution**: `--json` added to `health`, `health --compare`, `hotspots`, `dead`, and `simulate` (single-module and staged/diff modes). Each emits a stable JSON schema; pretty banners are suppressed when `--json` is set.

### Ergonomics & integration

- [ ] **VSCode / JetBrains sidebar**
  - `serve` exposes MCP for Claude Code / Cursor. A native tree view ("this file's bridgeness / hotspot / health contribution") pulls CodeCartographer into daily IDE use.

- [x] **GitHub App / Action**
  - `codecartographer-action` runs `check` + posts health delta as PR comment + uploads snapshot artifact.
  - Without it, adoption stalls at "I installed it locally."
  - **Resolution**: `github-action/` composite action. `action.yml` defines inputs (`fail-on-cycle`, `fail-on-layer-violation`, `fail-on-regression`, `regression-threshold`, `post-comment`). `scripts/install.sh` downloads the platform binary from GitHub Releases. `scripts/run.sh` runs `health --compare BASE_SHA`, `hotspots`, `snapshot save`; cross-references changed files against hotspot list; posts/updates a single sticky PR comment (identified by HTML marker, PATCH if exists, POST if new); writes to `$GITHUB_STEP_SUMMARY`; exits 1 on gate failures. Release workflow updated to also publish binary tarballs alongside the static library. `example-workflow.yml` provided for users.

- [ ] **Monorepo / multi-root stitching**
  - `codecartographer-workspaces.toml` listing N paths, analyzed as one graph with cross-repo edges via a registry.
  - Answers: *"does service A still depend on legacy lib B?"*
