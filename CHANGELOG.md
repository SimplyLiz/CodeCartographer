# Changelog

All notable changes to CodeCartographer will be documented in this file.

## [Unreleased]

### Fixed — stack overflow (process abort) on very large repos

Measured against a full Linux-kernel checkout (~64k C/H files, 362k edges), two
unbounded recursions aborted the whole process (SIGABRT, "stack overflow") —
and, when linked via FFI, took the host down with it:

- **Recursive tree-sitter walkers overflowed on deep ASTs.** The per-language
  walkers (`walk_c_cpp` et al.) recurse by AST depth; macro-generated C
  initializers in the kernel nest deep enough to overflow a rayon worker's
  default ~2 MB stack. Parse extraction now runs on a dedicated pool with a
  256 MB per-worker stack (both the CLI and the FFI `build_mapped_files` path).
- **`petgraph::algo::tarjan_scc` is recursive.** Cycle detection (and
  `check_would_create_cycle`) recursed once per node; a large, deep graph blew
  the main-thread stack. Replaced with an iterative, explicit-stack Tarjan SCC —
  depth-independent and deterministic.

Result: the kernel now analyzes cleanly in ~34 s (cold), health 30.0, 12862
bridges / 76 cycles / 145 god-modules, deterministic run-to-run; directory
rollup folds it to 562 subsystems (depth 2). Godot output unchanged.

### Fixed

- **Structural analysis was blind to Rust code, measuring docs instead.** Rust
  crate-internal `use crate::io::language::Foo` imports collapsed to the bare stem
  `language` and fell through to the resolver's low-confidence `fuzzy` fallback.
  Since bridges/cycles/health/roles run only on non-fuzzy edges, *every*
  crate-internal edge was dropped — on a doc-heavy Rust repo the only edges left
  were Markdown cross-references, so health, cycles and `ranked_skeleton` were
  computed entirely from documentation and core modules were mislabeled `dead`.
  Rust imports now resolve authoritatively against the full qualified path (a
  branch mirroring the Go resolver), tagged `suffix`; external crates yield no
  edge. Separately, the code-health metrics now exclude the documentation
  subgraph and `ranked_skeleton` drops non-focused docs (docs have `doc_index` /
  `doc_context`). On a 186-file Rust repo: 80 code edges now structural (was 0).
- **MCP server crashed on non-ASCII indexed content.** `truncate_str` sliced a
  string at a raw byte offset with no char-boundary check, so any repo carrying
  multi-byte UTF-8 in a JSON string value (accented text, en/em-dashes, emoji)
  panicked and took the whole server down on `switch_project` / indexing. It now
  backs off to the nearest char boundary.
- **reach — cross-language and multi-line-string false callers.** `reach_symbol`
  caller search is text-based; it reported a bare `submit()` in a `.swift` file as
  a caller of a Rust `Gate::submit`, and a symbol named inside a multi-line prompt
  string (`…re-judge…`) as a real reference. Callers are now restricted to the
  definition's language group, and lines that begin inside a carried
  string/comment are masked (Rust/Python).
- **answer_question ranked name coincidences over real code.** Query terms were
  matched against symbol names by raw substring, so `converse_actionable` matched
  "action" and outranked `Gate` / `execute_proposal` for an "action approval gate"
  question. Matching is now token-aware (whole identifier words) with a cheap
  inflection rule (approve~approval, execute~executes) that rejects lookalikes
  (action~actionable).
- **git churn/coupling counted noise and returned unsorted churn.** `git_churn`,
  `git_cochange`, `hidden_coupling` and shotgun-surgery included lockfiles, images
  and PDFs as first-class hotspots. The scanner's noise lists are now applied to
  the git-history path (source/config/docs kept). `git_churn` also emits a sorted
  `{file, commits}` array instead of an arbitrarily-ordered map, and the
  documented `0 → 500` commit-limit default is now implemented (was `-n 0`).

## [4.0.0] - 2026-07-12

Version reconciliation: the crate had drifted to `1.5.0` in `Cargo.toml` while the
only published git tag was `v3.0.0`. This release realigns the two under a single
`4.0.0` that supersedes the orphaned `v3.0.0` tag.

### Fixed

- **reach — precise caller/callee references on Rust (and other languages).**
  `reach_symbol` caller/callee edges were derived from text search and surfaced
  incidental name matches as call edges. They are now resolved from the call graph,
  eliminating the text-search noise.
- **release pipeline — case-sensitive path break on Linux.** The release workflow
  referenced `mapper-core/CodeCartographer` while the tracked directory is
  `mapper-core/codecartographer`. This passed on case-insensitive macOS but failed
  the Linux matrix leg, which failed the `build` job and skipped the `release`
  publish. Paths are now lowercase so tagged releases publish across all targets.

### Changed

- **Docs — standalone positioning.** CodeCartographer is documented as a standalone
  tool rather than a CKB-dependent component.

## [1.5.0] - 2026-07-11

### Added — dynamic MCP project root (roots auto-follow + `switch_project`)

The MCP server pinned its root at launch, so every tool answered about that one
repo even when the session was working in another (silent wrong-repo). The
running server can now retarget without a restart: if the client advertises the
`roots` capability, the server requests its workspace root after `initialized`
(server→client `roots/list`) and re-roots there automatically; and a new
`switch_project` tool re-points the server at any absolute path on demand, for
evaluating several repos in one session. State lives behind a swappable
`RwLock<Arc<ApiState>>`; each request takes an `Arc` snapshot so a mid-flight
switch can't change the root under a running handler.

### Added — directory-level rollup (`health --rollup <depth>`)

On a very large tree the file-level graph is too big to reason about (Godot:
4336 files, 3182 bridges). Rollup folds every file into the folder formed by its
first `<depth>` path components, aggregates cross-directory dependencies (drops
intra-directory edges), and runs the full structural analysis on the folded
graph — so bridges / cycles / god modules / health now describe **subsystems**,
not files. Godot at depth 2: 160 subsystems, 139 bridges, with actionable hints
like `core/variant — 72 callers across 8 domains → split by domain`. Exposed on
the CLI (`--rollup`, mutually exclusive with `--compare`) and as
`ApiState::rebuild_graph_rolled_up(depth)`.

### Fixed — non-deterministic import resolution

`ImportIndex` candidate lists were built by iterating a `HashMap`, so when an
import had several equally-ranked resolution targets the resolver picked one by
hash order — invisible at file level, but it flipped which directory an
ambiguous import folded into (rollup edge counts drifted run-to-run). Candidate
lists are now sorted at build time, and edge dedup breaks ties by resolution
strength (exact < suffix < fuzzy) so the strongest edge always survives. Both
make the graph fully deterministic.

### Performance — betweenness centrality (98% of a graph rebuild)

Profiling `rebuild_graph` on Godot (~4.3k files) showed betweenness centrality
was ~3.14s of a 3.2s rebuild. Two changes:

- **Index-based, parallel Brandes.** Each sampled source's pass now runs over
  dense `Vec` buffers keyed by node index instead of per-source
  `HashMap<&str,_>` (which reallocated V-sized maps 800×), and the independent
  sources run across cores. Contributions are summed in fixed source order, so
  results are bit-identical regardless of core count — deterministic across
  machines. Betweenness 3,140ms → 57ms; full rebuild 3,197ms → 76ms (~42×), with
  identical output (3182 bridges / 68 god-modules / 0 cycles on Godot).
- **Topology-keyed centrality cache.** Betweenness depends only on the graph
  topology, so it's cached keyed by an order-independent fingerprint of the
  structural node+edge set. An edit that doesn't change imports reuses it and
  skips the Brandes pass entirely; the cache survives `invalidate_graph` and
  self-invalidates only when an import edge changes. This is the incremental
  win for long-lived `serve` sessions, and it grows with repo size — at 100×
  scale, where sampled betweenness re-inflates to seconds, a body-only edit
  stays near the rebuild floor.

## [1.4.0] - 2026-07-11

### Added — import-resolution confidence & non-fuzzy structural metrics

- Edge resolution now carries a confidence signal; fuzzy (low-confidence) edges
  are excluded from cycles, and all structural metrics (health, bridges, god
  modules, layer checks) are computed on the non-fuzzy graph only.
- Source-relative resolution for JS/TS imports, including captured re-exports.
- Qualified path-suffix resolution for Python and other pathed imports.
- `go.mod`-aware Go import resolution.

### Fixed — large C++ codebase usability

Verified against Godot (~4.3k first-party C++ files):

- Exclude vendored source dirs (`thirdparty/`, `third_party/`, `3rdparty/`,
  `external/`) that .gitignore never catches and that dominated every metric.
- C/C++/Obj-C `.cpp` translation units with no importers are no longer flagged
  as dead code.
- Skip forward declarations (`class Foo;`) so symbols resolve to real
  definitions; `reach` ranks a definition above a declaration/prototype and
  returns real caller sites instead of string-literal or forward-decl noise.
- Bias NL `query` seeds to source files so data/doc files stop crowding out code.
- `cochange` drops mechanical mega-commits before pair generation, so it no
  longer explodes on large snapshots.

## [1.3.3] - 2026-07-09

### Changed — `context_health` project preset file

`context_health` reads an optional `CARTOGRAPHER.md` at the project root for its
`[commands]` section and surfaces the preset names in the health report under the
`commands` field. Helper: `parse_cartographer_commands`.

## [1.3.2] - 2026-07-09

### Removed — the two MCP resources (`project-graph`, `module-index`)

They each serialized the entire graph / module map as one JSON blob. Editors surface
MCP resources in the `@`-mention menu, so selecting one tried to inline the whole repo
into the prompt and failed with "too big for the index" — the exact "dump everything"
anti-pattern this tool avoids. Everything they offered is available on-demand and
budget-aware via tools: `get_project_graph`, `skeleton_map`, `ranked_skeleton --budget N`.

## [1.3.1] - 2026-07-09

### Added — call graphs & class diagrams for the six new languages

Completes "full depth" for Java, C#, Ruby, Kotlin, Swift, and PHP:

- **File-local call graphs** — a generic two-pass extractor resolves callees against
  the file's functions (`method_invocation` / `invocation_expression` / `call_expression`
  / `*_call_expression` / `call`). `reach_symbol` callees and `diagram --call-graph` now
  work for all six. Ruby additionally resolves paren-less bare-identifier calls when they
  uniquely match a known method.
- **Class diagrams** — `diagram --call-graph FILE --format class` emits classes with
  fields, methods (visibility, constructors, params), and inheritance/implements edges,
  e.g. Java `Circle --|> Shape`, `..|> Drawable`.

All twelve languages now sit at the same tier for the navigation surface.

## [1.3.0] - 2026-07-09

Sharper, broader lightweight navigation for AI — six improvements from the C++
evaluation plus six new languages. Everything stays tree-sitter/regex only;
compiler-precise resolution remains CKB's job.

### Added — six new languages (on by default)

Java, C#, Ruby, Kotlin, Swift, and PHP now have tree-sitter skeleton extraction
(classes, interfaces, structs, enums, methods, functions, properties, namespaces,
imports) with qualified names and doc-comments. A single generic walker covers the
brace-based OO grammars; Ruby has a dedicated `def…end` walker. `skeleton_map`,
`ranked_skeleton`, `reach_symbol`, and search work for all of them. Grammar crates
are pinned to tree-sitter 0.22-compatible versions (no core bump). File-local
call-graph and class-diagram support for these six is not yet wired (see the
Language support table in the README); `reach_symbol` callers work via text search.

### Added — symbol-aware search corpus

`query_context` and `answer_question` now rank over a BM25 corpus built from parsed
symbols (name + qualified name + signature + doc-comment) instead of raw file bytes,
falling back to content BM25 only when it finds nothing. Matches code intent rather
than string-literal/comment noise — e.g. "how does the hash map resize and rehash"
surfaces the hash-map files directly.

### Changed — reach_symbol navigation

Renders the scope-qualified name (`Object.get_class`) so overloads/overrides across
classes are distinguishable, surfaces the symbol's doc-comment, and skips call sites
that type-qualify the same name against a different class.

### Changed — orientation-first repo map

`ranked_skeleton` no longer sorts purely by PageRank-of-imports (which rewards
ubiquitous sink headers). Ranking is role-primary and uses fan-out, so the first
tokens land on entry points and domain core (e.g. Godot core/ now leads with
`register_core_types.cpp` and `variant.h`).

### Changed — import resolution

Ambiguous basenames (bare `#include "foo.h"` with `foo.h` in several dirs) now break
ties by directory proximity to the including file, then shortest path.

## [1.2.1] - 2026-07-09

### Fixed — graph build was O(N²) and hung on large C/C++ trees

`rebuild_graph` resolved each file's imports by scanning every other file, so
construction was O(N² × includes). On a 14k-file / 1.3M-LOC C++ repo (Godot) the
initial build ran for minutes and effectively hung. Import resolution now uses a
one-shot `ImportIndex` (basename / stem / segment / symbol maps) built once per
rebuild, making it O(N + edges). The resolver also prefers a **path-suffix** match
over a bare filename-stem match, so a project-root-relative `#include
"core/object/object.h"` resolves to `object.h` (not `object.cpp`). Combined
effect on Godot: `get_dependents` of `object.h` went from 1 to 66 (ground truth:
63 includers), and cold start went from >2 min (hang) to ~12 s.

### Fixed — betweenness centrality was O(V²) on large graphs

Bridge analysis re-initialized per-node maps for every source, ~100 s at 14k
nodes. For graphs above 1500 nodes it now estimates betweenness from a strided
sample of source nodes and scales the result (exact below the threshold). Cold
start on Godot dropped from ~104 s to ~12 s.

## [1.2.0] - 2026-07-09

### Fixed — MCP server was unusable with spec-compliant clients

`codecartographer serve` emitted `input_schema` (not `inputSchema`), content
blocks without a `type` field, and `is_error` (not `isError`). Spec-compliant
clients such as Claude Code rejected `tools/list` ("tools fetch failed"). The
`McpTool`, `McpContent`, and `McpToolResult` serializations now match the MCP
spec, so the server connects and its tools are callable.

### Fixed — natural-language retrieval returned the same files for every query

`query_context` passed the whole question to a raw-regex line search, which
almost never matched, so personalized PageRank had no anchor and fell back to
the graph's most central files regardless of the question. It now ranks via
`bm25_search` and biases toward code over docs. The tokenizer splits
camelCase/snake_case identifiers, so `churn` matches `git_churn` and `user`
matches `getUserById`. `ranked_skeleton` now places focus files first so a
relevant but low-centrality file survives the token-budget cut. `answer_question`
compared repo-relative BM25 paths against absolute `MappedFile.path` (dropping
every BM25 candidate); it now normalizes paths and seeds symbol-name matches.

### Added — `serve` incremental refresh (live sessions stay fresh)

A persistent `serve` scanned once at startup and went stale until restart.
`refresh_if_stale` runs a debounced (750 ms), mtime-based incremental sync on
each tool call: only changed files are re-parsed, deleted files are dropped, and
the graph cache is invalidated when anything changes. Uncommitted working-tree
edits are picked up mid-session; a burst of calls triggers at most one rescan.

### Added — `serve --preset=core`

Exposes a 12-tool discovery subset (also via `CARTOGRAPHER_PRESET`); the full
41-tool surface remains the default. Keeps the tool surface small so a model
picks the right tool.

### Changed — tool contract and output

- `target` is now a universal, backward-compatible alias for each tool's primary
  identifier (`module_id` / `file` / `focus` / `symbol` / `doc_path`).
- `reach_symbol` returns the candidate list on an ambiguous name instead of an
  error, and is documented as the primary symbol-discovery entry point.
- Response paths are repo-relative (were absolute), JSON is compact, and the
  `ranked_skeleton` budget now accounts for the serialized envelope (a 6000-token
  budget returns ≈6k, was ≈8.6k).
- The project graph is cached within a session instead of recomputed per call.

### Fixed — `get_evolution` unusable on fresh repos

Each FFI call previously appended one snapshot unconditionally, so three
calls 15 seconds apart produced three "snapshots" labelled with current
timestamps and presented as a trend. The output was a function of call count,
not project history.

**Deduplication by git HEAD** — if the most recent history entry carries the
same commit SHA as the current call, the entry is updated in-place rather than
a new snapshot being appended. Callers can invoke `get_evolution`/`codecartographer_evolution`
on every startup without polluting the history.

**`trendAvailable` flag** — `ArchitectureEvolution` gains a `trend_available`
(`trendAvailable` in JSON) boolean. It is `false` when the look-back window
contains fewer than two snapshots from distinct git commits (or, for non-git
roots, when the window spans less than one hour). Callers must suppress
directional trend UI when this field is `false`.

**`gitRef` in snapshots** — `ArchitectureSnapshot` gains an optional `git_ref`
field (serialised as `gitRef`) holding the HEAD SHA at snapshot time. Omitted
when the root is not a git repository.

### Fixed — `get_blast_radius` edge quality

Two sources of false edges in the dependency graph have been eliminated:

**Cross-type edges** — `rebuild_graph` no longer creates edges between source files
and doc/fixture files. A Go source file importing `"encoding/json"` would previously
resolve to any file whose stem matched `"json"` (e.g. `testdata/review/json.json`),
causing JSON fixtures to appear as dependencies. Markdown files like `CHANGELOG.md`
were appearing as dependents of source modules because `extract_markdown` treats
prose path mentions as imports. Edges are now only created between files of the same
type (source↔source or doc↔doc).

**Substring target match** — `get_blast_radius` resolved the query target using
`path.contains(target)`, which matched any path containing the target string as an
arbitrary substring. The lookup now requires an exact path match or a path-component
prefix boundary (`internal/codecartographer` matches `internal/codecartographer/bridge.go` but not
`internal/codecartographer_extra/foo.go`).

### Fixed — `get_evolution` snapshot ordering

`get_evolution` was appending the current snapshot to the end of the persisted history
list, so `snapshots[0]` was the **oldest** entry in the look-back window rather than
the live reading. The CLI "Current Status" display and MCP clients that inspect
`snapshots[0]` were showing stale or zero-scored historical data instead of the
current health score.

Snapshots are now returned newest-first (`snapshots[0]` = current). The `healthTrend`
delta comparators are updated accordingly.

### Added — multi-symbol reach and answer --then

**`codecartographer reach SYMBOL [SYMBOL ...]`** — passing two or more symbols
produces a unified intersection view. Callers are merged and deduped by
`(file, line)`. Callees appearing in more than one root are annotated
`[shared]`. Depth-2 types present in multiple results are promoted to a
"shared types" section above the ordinary depth-2 tail, rather than being
buried at the end. Ambiguous or not-found symbols are reported per-symbol
and skipped; the remaining results still render.

**`codecartographer answer QUESTION --then N`** — after printing the evidence
chain, drills into item #N via `reach` (using the item's file for
disambiguation) and appends the context tree below. Reuses the scan
already done by `answer`, so there is no second disk pass.

### Added — companion ordering in `answer`

When two functions from different files score within 10% of the top scorer,
the one from the older file (earliest git first-commit timestamp) now ranks
first. This surfaces the original implementation before companions added
later — e.g. `build_file_call_graph` before `build_class_graph` for a
"call graph" query where both files score nearly identically.

File creation timestamps are fetched via `git log --diff-filter=A` once per
unique file in the candidate set. If git is unavailable the tiebreaker is
skipped and pure score order is preserved.

### Improved — private-function noise filtering in `answer`

The gate that suppresses low-confidence private functions now requires at
least **two distinct query terms** to match the symbol name, up from one.
The previous threshold (`name_score ≥ 3.0`) passed functions whose name
contained any single query term — e.g. `health_graph_at_ref` matching
"graph" for a "call graph" query would appear at position #6 from a
high-BM25 file like `main.rs`. Single-term collisions from private helpers
are now rejected.

### Added — Go, C, and C++ call graph extraction

`call_graph.rs` now resolves intra-file call edges for Go (`.go`), C
(`.c`/`.h`), and C++ (`.cpp`/`.cc`/`.cxx`/`.hpp`/`.hxx`) in addition to
the existing Rust and Python support. All three use the tree-sitter parsers
that were already compiled into the `default` feature set.

- **Go**: free functions and pointer/value receiver methods. Receiver type
  extracted from the `parameter_declaration` inside the `receiver` node,
  stripping leading `*` for pointer receivers.
- **C**: free functions only (standard C has no methods). Declarator chain
  (`pointer_declarator` → `function_declarator` → `identifier`) handles
  pointer-return signatures.
- **C++**: free functions, inline class methods (scope stack via
  `field_declaration_list`), and out-of-class method definitions
  (`qualified_identifier` in the declarator). Method names inside class
  bodies use `field_identifier` nodes — this distinction from `identifier`
  is handled explicitly. Template wrappers are stripped; namespace bodies
  are recursed without scope qualification.

`reach find_callees` automatically benefits — it calls `build_file_call_graph`
and returns precise callee lists for these languages instead of the
"call graph unavailable" heuristic note.

### Added — cross-file sequence trace (PR #9, spike)

`codecartographer diagram --entry FILE::FUNCTION --format sequence [--depth N]` traces
call edges across file boundaries rather than within a single file.

Resolution is two-level: (1) direct-import match — callee name found in a module
the current file explicitly imports; (2) heuristic match — callee name is unique
across the whole project. Heuristic edges are annotated `(~)` in the diagram so
reviewers can spot lower-confidence steps.

**`src/call_graph.rs`** — `FileCallGraph` gains `unresolved_calls: Vec<(String,
String)>` (caller, raw callee name). Previously the count was tracked but the
names were discarded, making cross-file resolution impossible.

**`src/cross_call.rs`** (new) — `trace_from_entry()` builds a symbol index from
the project's `MappedFile` map and an import adjacency graph, then BFS-walks from
the entry function up to `depth` cross-file hops. `CrossCallTrace` holds ordered
`(module, fn)` steps and a list of unmatched calls for diagnostics.

**`src/diagram.rs`** — `render_cross_sequence()` renders a `CrossCallTrace` as a
Mermaid `sequenceDiagram` with one participant per module (not per function).

**`src/extractor.rs`** — Rust `mod foo;` declarations (without a body) are now
captured as imports so the adjacency builder can recognise them as direct
dependencies. Previously only `use` declarations were recorded.

**`src/main.rs`** — `--entry FILE::FUNCTION` flag on `codecartographer diagram`; triggers
a full project scan to build the symbol index before tracing.

Spike validated on `diagram_mode` (the target from charts.md): 13 modules,
60 resolved edges in correct call order; direct-import edges unqualified,
transitive edges correctly annotated `(~)`.

### Added — quadrant chart and ER diagram formats (PR #8)

Two new `--format` values for `codecartographer diagram`:

**`--format quadrant`** — Mermaid `quadrantChart` plotting every file on a churn ×
complexity plane. Top-right = danger zone (refactor now); top-left = risky debt;
bottom-right = hotspots (add tests); bottom-left = stable. Coordinates are
min-max normalised to `[0.01, 0.99]` within the included node set. Uses
`signature_count` as a complexity proxy until tree-sitter cyclomatic complexity
extraction lands. Automatically triggers git enrichment — no extra flags needed.

**`--format er`** (with `--call-graph FILE`) — Mermaid `erDiagram` derived from
the existing `ClassGraph` extractor. Entities come from struct/class definitions;
relationships are inferred from field types: `Vec<T>` / `HashSet<T>` → one-to-many
(`||--o{`), `Option<T>` → zero-or-one (`||--o|`), bare `T` / `Box<T>` / `Arc<T>`
→ exactly-one (`||--||`). Nested wrappers are resolved one level deep; duplicate
edges are deduplicated. Language coverage matches `--format class` (Rust, Python,
TypeScript, Go).

Both formats route through `export_mermaid()` — SVG/PNG export via `mmdc` works
unchanged. `DiagramFormat` gains `Quadrant` and `Er` variants; `lib.rs` and
`diagram_export.rs` updated to cover all arms.

**Also fixes three pre-existing bugs:**
- `enrich_with_git` was keying churn, co-change, and owner lookups on
  `node.path` (absolute filesystem path) instead of `node.module_id`
  (repo-relative path matching `git log --name-only` output). This silently
  zeroed all git-backed overlays — hotspot sizing, co-change edges, and
  colour-by-owner — for every diagram command.
- `search_skeleton` `McpTool` was missing `title` and `annotations` fields,
  causing a compile error introduced in PR #6.
- `McpServerInfo::default()` name did not match the test expectation.

### Added — sequence and UML class diagrams (PR #7)

**`--format sequence`** (with `--call-graph FILE`) — Mermaid `sequenceDiagram`
showing the call order among functions defined in a single file. Participants are
emitted in source order; messages are call edges in AST order. Self-calls render
as Mermaid loop arrows. A trailing note reports how many external calls were
dropped so the reader knows the graph is file-local.

**`--format class`** (with `--call-graph FILE`) — Mermaid `classDiagram` extracted
from a single file's type structure. Coverage by language:

- **Rust** — `struct` fields (name, type, `pub`/private), `enum` variants, `trait`
  method signatures, `impl` methods attached to their type, `impl Trait for Type`
  arrows (`..|>`).
- **Python** — class declarations, base-class inheritance (`--|>`), instance fields
  from `__init__` with type annotations, method signatures.
- **TypeScript / TSX** — class fields with access modifiers, `extends` inheritance,
  `implements` interface arrows, `interface` declarations.
- **Go** — struct fields (uppercase = public), embedded struct inheritance, interface
  method sets, methods attached to their receiver type.

**`src/class_graph.rs`** (new) — `ClassGraph`, `ClassNode`, `FieldDef`, `MethodDef`
structs; `build_class_graph()` dispatches to per-language extractors via tree-sitter.
`ClassRelationship` covers `Inherits`, `Implements`, `Composes`, and `Depends`.

**`src/diagram.rs`** — `render_sequence()` and `render_class()` alongside the
existing `render()`; `DiagramFormat::Sequence` and `DiagramFormat::Class` variants.

### Added — `search_skeleton` MCP tool (PR #6)

New `search_skeleton` tool fills the gap between `focused_skeleton` (requires
a known module ID) and `skeleton_map` (returns everything). Takes a
case-insensitive `pattern` string, matches it against file paths first then
symbol names, and returns enriched skeleton sections for hits — same shape as
`focused_skeleton` with `heat`, `imports`, `signatures`, and churn labels.

Parameters: `pattern` (required), `detail` (minimal/standard/extended, default
standard), `budget` (token cap, 0 = unlimited). Path matches sort before symbol
matches within results.

Also fixes a latent panic in `compute_churn_labels` when called on a project
with zero indexed files: the previous `.max(1)` guard prevented division-by-zero
but still allowed an out-of-bounds index on the empty counts vec.

### Added — MCP tool schema: title, annotations, and enum hints (PR #5)

`McpTool` now carries two new optional fields that MCP clients and LLM planners
can consume without calling the tool:

- **`title`** — human-readable display name (e.g. `"Get Module Context"`).
  All 38 tools have a title; `McpServerInfo` gains one too (`"CodeCartographer"`).
- **`annotations`** — `ToolAnnotations` struct with `readOnlyHint`,
  `destructiveHint`, and `idempotentHint`. Every read-only tool advertises
  `readOnlyHint: true` via the `read_only!()` macro; `replace_content` sets
  `destructiveHint: true`; `set_compression_level` is `idempotentHint: true`.
- **`enum` hints** on string parameters with a fixed value set:
  `detail_level` (`minimal`/`standard`/`extended`), `level`
  (`minimal`/`standard`/`aggressive`), `query_type` (`node`/`edge`), and
  `model` (`claude`/`gpt4`/`llama`/`gpt35`) across `context_health`,
  `query_context`, and `query_docs`.

**Schema correctness fixes** (found during review):
- `get_symbol_context`: `detail_level` was incorrectly required and lacked an
  enum; it is now optional with `["minimal","standard","extended"]`.
- `search_project`: `query_type` was required; now optional with
  `["node","edge"]` enum.
- `get_blast_radius`: `max_related` was required despite having a default.
- `semidiff`: `commit1` and `commit2` were required despite both defaulting
  to `HEAD~1` / `HEAD`.
- `query_docs`: `model` enum was missing `"gpt35"` vs the other two scoring
  tools.

### Added — `focused_skeleton` and `diff_skeleton` MCP tools (PR #4)

`focused_skeleton` — returns the enriched skeleton for a seed file and every
file within N import-hops of it (importers + importees). BFS over the
dependency graph, depth-controlled by the `depth` parameter (default 1).
Cheaper than `skeleton_map`, more targeted than `ranked_skeleton`.

`diff_skeleton` — returns the enriched skeleton for files changed between two
commits plus their immediate importers. Defaults to `HEAD~1..HEAD`. Minimal
context for understanding a diff's blast radius without reading the whole graph.

Both tools enrich each file entry with `heat` (hot/stable from git churn),
`imports`, and per-symbol `tested` markers derived from `#[test]` coverage.

### Added — skeleton enrichment: type bodies, tested markers, churn labels (PR #3)

Skeleton output now includes:
- **Type bodies** — enum variants and struct fields are inlined into the
  skeleton rather than collapsed to `// ...`, making type shapes readable
  without jumping to source.
- **Tested markers** — public functions that have a corresponding `#[test]`
  (matched by stripped name) are annotated `// tested` in skeleton output.
- **Churn labels** — files in the top quartile of git commit frequency are
  marked `heat: "hot"`; bottom quartile marked `"stable"`. Derived from the
  last 300 commits via `git_churn`.

### Added — function-level call graphs for Rust and Python

`codecartographer diagram --call-graph PATH` now extracts a file-local call graph
and renders it through the existing Mermaid/DOT/ASCII pipeline. Nodes are
functions/methods, edges are caller→callee relations resolved within the same
file; calls into the stdlib or other files are dropped and reported as an
`unresolved` count so the reader knows the graph is local-only.

**`src/call_graph.rs`** (new) — tree-sitter walkers for Rust and Python:
- Two-pass traversal: enumerate function defs (with `impl` / `class` scope →
  `Type::method` / `Class.method` qualified names), then walk each body for
  `call_expression` / `call` nodes.
- `Resolver` disambiguates bare callee names (e.g. `self.bar()` → `S::bar`)
  by exact-qualified match first, then unique-simple-name match. Ambiguous
  simple names go to `unresolved` rather than guessing.
- Self-recursion edges are dropped (uninteresting in a diagram).
- Feature-gated behind `lang-rust` / `lang-python` to match the rest of the
  tree-sitter surface.
- `to_project_graph()` wraps output in a `ProjectGraphResponse` so
  `diagram::render()` can consume it without any call-graph-specific rendering
  code.

**`src/main.rs`** — `--call-graph FILE` flag on `codecartographer diagram`. When set,
import-graph-only options (`--cochange-threshold`, `--docs-only`,
`--group-by-folder`, `--color-by-owner`) are bypassed rather than erroring,
since the common case is combining `--call-graph` with `--focus`, `--depth`,
and `--format`.

**Tests** — 8 new tests in `call_graph`:
- Rust free-function edges, method resolution via simple name, unresolved
  counting, self-recursion dropped.
- Python free-function edges, method resolution via `attribute` callees.
- Unknown extension returns `None` cleanly.
- `to_project_graph` shape check (one node per function, `edge_type: "call"`).

### Added — ASCII tree diagram format

New `--format ascii` (aliases: `tree`, `text`) produces a terminal-friendly
indented tree using `├── ` / `└── ` / `│   ` box-drawing, rooted at the most
useful node available:
- Explicit `--focus` wins.
- Else the blast-radius epicenter (when `--blast-radius` is set).
- Else the included node with the highest **out-degree** — this deviates
  from top-by-degree's total-degree ranking on purpose, because a leaf node
  at the root renders as an empty tree.

**`src/diagram.rs`** — `DiagramFormat::Ascii` variant, `render_ascii()` DFS with
cycle-safe re-entry marker (`↑ seen`), depth cap honoured, and per-node label
showing signature count plus overlay tags (`★ epicenter`, `◉ cycle`, `✦ pivot`,
`♨ hot`, role). Nodes in `included` that aren't reachable from the root are
printed under a `(disconnected)` tail so the caller still sees them.

**`src/diagram_export.rs`** — rejects `.svg`/`.png` output for ASCII diagrams
with a clear "pick mermaid/dot for images" error; passthrough writes still
work for any text extension.

**Tests** — 4 new tests: format parse aliases, tree-glyphs present, root
lands on focus, cycle → `↑ seen` marker, depth cap excludes deeper nodes.

### Added — interactive HTML diagram export

Writing diagram output to a `.html` path now produces a self-contained
single-file explorer instead of raw diagram source. Vanilla JS, no CDN,
no build step — open directly in a browser.

**`src/html_export.rs`** (new) — `render_html(graph, &included) -> String`:
- Embeds node/edge metadata as inline JSON (same selection rules as
  Mermaid/DOT/ASCII — reuses `RenderedDiagram.included`).
- Sidebar filter, click-to-select, neighbor lists annotated with violation
  badges so the reader sees structural signals without re-rendering.
- 4 tests for structural invariants, metadata presence, edge filtering on
  the included set, and JSON-escaping of path strings.

`RenderedDiagram` now exposes `included: Vec<String>` so any downstream
exporter can reuse the selection. `diagram::select_for_render()` is a public
helper that returns the selection alone for callers that need it without a
rendered payload.

### Added — SVG / PNG export via external converters

Output path extension drives the exporter: `.svg`/`.png` shells out to the
right tool based on source format — Mermaid → `mmdc` (npm
`@mermaid-js/mermaid-cli`), DOT → `dot` (Graphviz). Missing binaries produce
an actionable install message. Any other extension still writes the raw
diagram source.

**`src/diagram_export.rs`** (new) — `export_diagram(content, source_format,
target) -> Result<ExportKind, String>`. Returns an enum so the CLI can print a
matching status line (`"SVG exported to …"` vs `"Diagram written to …"`).

### Added — ownership coloring (`--color-by-owner`)

New `--color-by-owner` flag replaces role-based node fills with a stable
palette keyed on the dominant git author. Useful for seeing team-ownership
boundaries overlaid on the import graph.

**`src/git_analysis.rs`** — new `git_ownership(root, limit) -> HashMap<path,
author>`. Reuses the existing bot/formatter filters; picks the author with the
most commits per file (alphabetical tiebreak for stability). Enrichment only
runs when `--color-by-owner` or `--cochange-threshold` is set so default
diagram builds don't pay for git calls.

**`src/diagram.rs`** — `RenderOptions.color_by_owner: bool`; `owner_color()`
hashes author → 10-color palette via FNV-1a 32-bit. Overlay borders (cycle,
pivot, hot, epicenter) still take precedence. Mermaid path emits per-node
`style` lines instead of the role class so the owner fill wins cleanly.

### Added — folder-collapsed view (`--group-by-folder DEPTH`)

New flag collapses the graph to folder granularity before rendering:
`--group-by-folder 1` groups by top-level dir, `2` groups by second level,
etc. Edges are aggregated (self-loops dropped; inter-folder edges summed);
focus/blast-radius/docs-only all work on the collapsed graph.

**`src/diagram.rs`** — `collapse_by_folder(graph, depth)` rebuilds a synthetic
`ProjectGraphResponse` where each folder becomes a single `GraphNode` with
`language: "folder"` and aggregated `signature_count`/`fan_in`. Renderers
detect `language == "folder"` and emit `shape=folder` in DOT / the blue
`:::folder` class in Mermaid.

### Added — doc-map diagram (`--docs-only`)

`--docs-only` filters the selection to the documentation subgraph: every
Markdown/YAML/TOML/JSON node plus the code files they directly reference.
Surfaces dead docs (doc nodes with no referenced code) and orphan code
(code with no documentation). Doc nodes render distinctly regardless of the
flag: Mermaid stadium shape `([...])` with yellow fill, DOT `shape=note`.

### Added — co-change edges + blast-radius view

- **`--cochange-threshold THRESHOLD`** overlays dotted purple edges for every
  co-change pair whose `coupling_score ≥ THRESHOLD` and whose both endpoints
  are in the included set. Requires git history (enriched via
  `enrich_with_git`).
- **`--blast-radius MODULE`** overrides selection: included = `{target} ∪
  direct deps ∪ direct dependents`. Target renders as an epicenter (bold red
  fill in both Mermaid and DOT).

### Fixed — `rebuild_graph` deadlock when any import resolves

`ApiState::rebuild_graph` held the `mapped_files` Mutex across its whole
loop and then called `resolve_import_target`, which re-acquired the
**same** non-reentrant `std::sync::Mutex` — any project with at least one
resolvable import would deadlock. `diagram`, `health`, and every command
that rebuilds the graph would hang indefinitely. Split the lookup into a
public `resolve_import_target` (locks, for external callers) and a
private `resolve_import_target_in(&HashMap<_, _>, …)` that takes the
already-held map; `rebuild_graph` now calls the latter. Added regression
test `rebuild_graph_does_not_deadlock_on_imports`.

### Fixed — `localize-tree-sitter-symbols.sh` silently dropped grammar C parsers

The post-build script extracted archive members via `ar x` before partial-
linking them into `combined.o`. Cargo emits one `parser.o` and one
`scanner.o` per grammar crate (`tree-sitter-c`, `-cpp`, `-rust`, `-go`,
…) and they all share filenames — `ar x` writes each extraction on top
of the previous, so only the **last** grammar's C parser survived on
disk. The resulting localized archive then had `_tree_sitter_c` and
`_tree_sitter_cpp` as undefined externals, referenced by the Rust
`tree_sitter_c::language()` / `tree_sitter_cpp::language()` wrappers
but never provided, so Go consumers linking `libcode_cartographer.a` got
undefined-symbol errors at `codecartographer`-tagged builds.

The script now feeds the archive directly to `ld -r` via
`-Wl,-force_load,input.a` (Mach-O) or
`-Wl,--whole-archive input.a -Wl,--no-whole-archive` (ELF), which pulls
every member in without ever writing them to the filesystem. No more
name collisions; all grammar parsers end up inside `combined.o` and
localize correctly.

### Added — architectural overlays on Mermaid / DOT diagrams

The diagram renderer now surfaces cycles, layer violations, and hotspots
directly in the output instead of leaving them buried in the JSON graph.
Nothing is opt-in: if the data is in `ProjectGraphResponse`, it shows up.

**`src/diagram.rs`** — new `Overlays` precomputation step, applied by both
Mermaid and DOT renderers so CLI and MCP stay lock-step:
- **Cycles** — nodes that appear in any `graph.cycles` member get a thick red border (DOT `color=#cc0000 penwidth=3`; Mermaid `:::cycle` via `class` statement). Cycle-internal edges get a heavy red arrow (`==>` in Mermaid, solid red in DOT). An edge participates iff both endpoints share a cycle's `nodes` set.
- **Pivot nodes** — `CycleInfo.pivot_node` gets a dashed red border (`:::pivot`) so it stands out inside the cycle. Pivot takes precedence over plain cycle marking on the same node.
- **Layer violations** — edges matching `graph.layer_violations` pick up violation-type styling: `BackCall`/`CircularCrossLayer` → dashed red; `SkipCall` → dotted orange; `DirectForeignImport` → dotted yellow. Mermaid uses `-.->` arrows + per-edge `linkStyle` directives; DOT uses `style=dashed|dotted` + colour.
- **Hotspots** — nodes with `hotspot_score ≥ 70` get an orange border. In DOT they also scale: `width`, `height`, and `fontsize` all interpolate linearly from the score. In Mermaid they pick up `:::hot` (Mermaid can't size nodes, so border-only).
- **Precedence** — a node that's both hot and in a cycle wears the cycle red, not the hot orange (architectural signal wins over performance signal).

**Tests** — 8 new unit tests in `src/diagram.rs`:
- Mermaid cycle/pivot class assignments
- DOT cycle edges render red
- Mermaid + DOT layer-violation styling (both BackCall and SkipCall)
- DOT hotspot sizing + orange border; cold nodes stay at default width
- Mermaid hot-class assignment
- Cycle border precedence over hot border
- Truncation safety: `linkStyle` indices never exceed the count of emitted edges when `max_nodes` cuts the graph

Backwards-compatible: the existing role-based fill colours (`core`, `bridge`,
`dead`, `entry`) remain untouched; overlays live on the border/edge so they
compose rather than collide. Existing tests asserting `:::core`/`:::bridge`
still pass.

### Added — `renderArchitecture` MCP tool + `codecartographer_render_architecture` FFI

The CLI's `diagram` command has been factored into a shared renderer and
exposed via FFI so MCP clients can return Mermaid/DOT directly. IDEs that
render Mermaid inline (Cursor, Claude Desktop, VS Code markdown preview,
GitHub) now get paste-able diagrams without any extra UI.

**`src/diagram.rs`** (new) — shared renderer, pure over `ProjectGraphResponse`:
- `render(graph, RenderOptions) -> RenderedDiagram { diagram, truncated, node_count }`
- No focus → top-N nodes by degree, isolated nodes skipped ("shape of the codebase")
- With focus → undirected BFS over import edges to `depth` ("shape of the neighborhood I'm editing"); undirected because the area being edited usually includes both what it imports and what imports it
- `focus` accepts module_id, exact path, or path suffix (e.g. `"server.rs"` matches `"src/server.rs"`)
- `truncated: true` in the response signals the node cap kicked in so the caller/model can tighten focus or lower depth
- 12 unit tests cover top-N, BFS direction, path-suffix match, cycle safety, truncation, format parsing, and output structure

**`src/lib.rs`** — `codecartographer_render_architecture(path, format, focus, depth, max_nodes)`:
- Defaults: `format` null → `"mermaid"`, `depth` 0 → 2, `max_nodes` 0 → 40
- Returns JSON `{ diagram, truncated, format, nodeCount }`
- cbindgen regenerates `include/codecartographer.h` automatically

**`src/main.rs`** — CLI `diagram_mode` now delegates to `diagram::render()`, so CLI and FFI outputs stay identical.

### Added — tree-sitter symbol localization for `libcode_cartographer.a`

`libcode_cartographer.a` now ships with its tree-sitter runtime and grammar
symbols hidden from the global symbol resolver, so consumers that also
link tree-sitter (e.g. Go projects using `go-tree-sitter`) no longer
trip duplicate-symbol errors at link time. This matters beyond the
ergonomic complaint: if both copies were left global, the linker would
bind CodeCartographer's Rust code to whichever archive came first on the
command line — and if the two tree-sitter versions drifted in struct
layout, the loser's callers would walk the wrong struct and produce
silent memory corruption.

**`scripts/localize-tree-sitter-symbols.sh`** (new):
- Partial-links all `.o` members of `libcode_cartographer.a` into one combined relocatable object via `cc -nostdlib -Wl,-r`, so CodeCartographer's internal `ts_*`/`tree_sitter_*` references resolve within the archive
- `rust-objcopy --wildcard --localize-symbol='ts_*' --localize-symbol='tree_sitter_*'` then marks those symbols local on the combined object; `codecartographer_*` FFI entry points stay global
- Resolves `rust-objcopy` via `rustc --print target-libdir`; falls back to `llvm-objcopy`/`objcopy` if `llvm-tools-preview` isn't installed
- `scripts/tests/test-localize-symbols.sh` — synthetic fixture smoke test
- **Background:** tree-sitter's own build.rs already passes `-fvisibility=hidden`, but `tree_sitter/api.h` wraps the API in `#pragma GCC visibility push(default)`, which wins over the command-line flag whenever a C source includes the header. Compile-time visibility is therefore insufficient; the archive must be post-processed.
- **Bonus:** partial-link dead-strips unused sections, shrinking the arm64 release archive from ~57 MB → ~19 MB.

**`.github/workflows/release.yml`** — runs the localization script after `cargo build --release` on all targets; added `components: llvm-tools-preview` to the rustup install.

---

## [2.5.0] - 2026-04-11

### Added — `search_in_symbol`, `list_key_handlers`, `map_state_machine` MCP tools

Three new diagnostic tools for navigating large source files, motivated by TUI codebases
where a single file can exceed 6000 lines with complex state-machine dispatch.

**`src/mcp.rs`** — `search_in_symbol`:
- Scopes a content search to the body of a named function or method
- Locates the symbol in the skeleton index to get its `line_start`; estimates `line_end`
  from the next symbol's `line_start` (fallback +500 lines)
- Filters `search_content` results to that estimated range — eliminates false positives
  when the same pattern appears in multiple functions across a large file
- Parameters: `file`, `symbol`, `pattern` (required); `context_lines` (optional, default 2)

**`src/mcp.rs`** — `list_key_handlers`:
- Extracts a structured key-binding map from a TUI source file
- Searches for `case "` and `== "` patterns (covers Go/Bubble Tea, Rust/crossterm, and
  any framework using quoted key strings)
- Groups results by key string using a BTreeMap (sorted output); each entry includes
  line number, matched text, and surrounding context
- Parameters: `file` (required); `context_lines` (optional, default 4)

**`src/mcp.rs`** — `map_state_machine`:
- Produces a state × handlers matrix: which keys are handled in which state
- Step 1: finds all state enum variants containing `state_prefix` in the file
- Step 2: finds all state guard locations (`state_var == `) and parses which state each checks
- Step 3: collects all key handler matches; attributes handlers within 60 lines of each guard
  to that state
- Useful for Bubble Tea chatviews, Redux reducers, finite automata, and any switch-on-state code
- Parameters: `file` (required); `state_var` (default `m.state`), `state_prefix` (default `State`),
  `context_lines` (optional, default 3)

**`src/mcp.rs`** — shared helper:
- `extract_quoted_key(line) -> Option<String>`: extracts first double-quoted token ≤ 30 chars
  from a line; used by both `list_key_handlers` and `map_state_machine`

---

## [2.4.2] - 2026-04-11

### Added — `watch_graph` MCP tool + CARTOGRAPHER.md preset awareness

**`src/mcp.rs`** — `watch_graph` tool (#30):
- Watches a directory recursively for source file changes (`.rs`, `.go`, `.py`, `.ts`, `.js`, `.dart`) using the `notify` crate
- Streams incremental graph events as newline-delimited JSON: `{ kind, path, timestamp_ms }`
- `kind` values: `file_reindexed` | `graph_updated`
- `timeout_secs` argument (default 30, max 300); returns event count summary on completion

**`src/mcp.rs` + `src/token_metrics.rs`** — CARTOGRAPHER.md `[commands]` preset integration:
- `context_health` now reads the `[commands]` section from `CARTOGRAPHER.md` at the project root
- Preset names are included in the health report as `commands: [...]`
- Warns if any preset command string references a file that participates in a detected dependency cycle

**`src/token_metrics.rs`**:
- `ContextHealthReport.commands: Option<Vec<String>>` field
- `parse_cartographer_commands(root) -> HashMap<String, String>` — parses `[commands]` key=value pairs from `CARTOGRAPHER.md`

---

## [2.4.1] - 2026-04-10

### Added — Tier-1 regex extraction for C#, Swift, Lua, Shell, SQL, Markdown, YAML, TOML

**`src/mapper.rs`** — 8 new language extractors:

- **C#** (`.cs`): `using` imports, class/interface/enum/struct/record type declarations (with access modifiers), method/function signatures with scope qualification via `ScopeTracker`
- **Swift** (`.swift`): `import` statements, class/struct/enum/protocol/actor types, `func` (method-qualified), `extension` (as Namespace), `typealias`, `var`/`let` properties inside types
- **Lua** (`.lua`): `require` imports, `function foo()` declarations, `foo = function()` assignments
- **Shell** (`.sh`/`.bash`/`.zsh`/`.fish`): `function foo()` and `foo()` style function declarations
- **SQL** (`.sql`): `CREATE TABLE/VIEW/FUNCTION/PROCEDURE/INDEX/TRIGGER` (SymbolKind matched to object type), `ALTER TABLE`
- **Markdown** (`.md`): headings `#`–`######` → Namespace (H1) / Field (H2–H6); slug used as LIP URI key for stability
- **YAML** (`.yaml`/`.yml`): top-level key extraction (no-indent lines ending in `:`)
- **TOML** (`.toml`): section headers `[name]` and `[[name]]`

All extractors carry `confidence = 30` (Tier 1 regex). Previously all these file types returned `MappedFile::empty()` or fell through to the generic extractor.

---

## [2.4.0] - 2026-04-10

### Added — Co-change dispersion / shotgun surgery detection

**`src/git_analysis.rs`** — `CoChangeDispersion` struct + `git_cochange_dispersion()`:
- For each file, computes: `partner_count` (distinct co-change partners), `total_cochanges`, Shannon entropy (`−Σ p_i·log₂(p_i)`), and `dispersion_score` (0–100 normalised). High entropy + many partners = shotgun surgery smell (arXiv:2504.18511)
- Reuses existing `git_cochange()` output — no extra git subprocess

**`src/api.rs`** — 4 new fields on `GraphNode`:
- `fan_in` — in-degree (number of files that import this file)
- `fan_out` — out-degree = CBO, Coupling Between Objects (number of files this imports)
- `cochange_partners` — distinct co-change partners (populated by `enrich_with_git`)
- `cochange_entropy` — Shannon entropy of co-change distribution

**CLI**: `codecartographer shotgun [--commits N] [--top N] [--min-partners N]` — ranked shotgun surgery candidates with HIGH/MODERATE/LOW tiers

**MCP tool**: `shotgun_surgery` — tool #29; returns `CoChangeDispersion[]` ranked by dispersion score

**FFI**: `codecartographer_shotgun_surgery(path, limit, min_partners) -> *mut c_char` — #19

---

## [2.3.0] - 2026-04-10

### Added — Context health scoring (`token_metrics`)

**`src/token_metrics.rs`** — new module with research-backed context quality analysis:

- **Signal density** — ratio of symbol-bearing tokens to total. Below 5% triggers the attention dilution warning from Morph 2024 "Context Rot" (effective attention reduced to 1/40th at 2.5% density)
- **Compression density** — zlib ratio as an information entropy proxy (Entropy Law, arXiv:2407.06645). Below 30% = high boilerplate/redundancy
- **Position health** — U-shaped attention bias score; key modules at context boundaries score higher (Liu et al., TACL 2024: >30% accuracy drop for middle-placed content)
- **Entity density** — symbols per 1K tokens, BudgetMem-style signal (arXiv:2511.04919)
- **Utilisation headroom** — buffer between used tokens and model window (penalises >85%)
- **Dedup ratio** — unique-line fraction as quick redundancy check
- Composite score (0–100, graded A–F) with BudgetMem-informed weights: signal_density 25%, compression_density 20%, position_health 20%, entity_density 15%, utilisation_headroom 10%, dedup_ratio 10%

**CLI**: `codecartographer context-health [FILE] [--model claude|gpt4|llama|gpt35] [--window N] [--format text|json]`

**MCP tool**: `context_health` — tool #27; scores any context string passed directly as an argument

**FFI**: `codecartographer_context_health(content, opts_json) -> *mut c_char` for CKB

**13 tests** covering all individual metrics, composite analysis, and warning generation

### Added — PKG retrieval pipeline (`query_context`, `codecartographer query`)

**MCP tool #28: `query_context`** — single-call retrieval pipeline replacing the manual search → ranked_skeleton → context_health sequence:
1. Searches the codebase for files matching the query (regex)
2. Uses matching files as the PageRank personalization seed
3. Builds a token-budget-aware skeleton ranked by relevance
4. Scores the bundle with context_health
5. Returns `{ context, filesUsed, focusFiles, totalTokens, health }` — ready to inject

**CLI**: `codecartographer query <QUERY> [--budget N] [--model claude|gpt4|llama|gpt35] [--format text|json]`

**BM25 search** (`src/search.rs`): `bm25_search(root, query, opts)` — TF-IDF ranked file search for natural language queries, used by `query_context` as a complement to regex matching. No external dependencies; pure Rust with standard BM25 (k1=1.5, b=0.75). Returns ranked `Vec<BM25Match>` with per-file scores and matching term snippets.

---

## [2.1.0] - 2026-04-10

### Added — C/C++ tree-sitter extraction, import extraction, tests

**C and C++ grammars** (`lang-c`, `lang-cpp` features):
- C: `function_definition`, `declaration` (prototypes), `struct_specifier`, `union_specifier`, `enum_specifier`, `type_definition`, `preproc_def`, `preproc_function_def`, `preproc_include` (→ imports)
- C++: all of C plus `class_specifier` (with body walk for inline methods), `namespace_definition` (scoped), `template_declaration` (unwrapped), `linkage_specification` (`extern "C"`)
- `.h`/`.hpp`/`.cpp`/`.cc`/`.cxx` routed to C++ grammar when `lang-cpp` is enabled; `.c` uses C grammar

**Import extraction** — tree-sitter now also replaces the regex import pass for all supported languages:
- Rust: `use_declaration` nodes → strip `use ` / `;`
- Go: `import_declaration` → `import_spec` path strings (quoted paths stripped)
- Python: `import_statement` module names, `import_from_statement` module_name field
- TypeScript / JavaScript: `import_statement` source field (quotes stripped)
- C/C++: `preproc_include` path field (retains `<>` / `""` delimiters)

**Tests** — 27 tests across all 7 languages covering function extraction, method qualification, import extraction, visibility filtering, and symbol kinds.

---

## [2.0.0] - 2026-04-10

### Added — Tree-sitter skeleton extraction (Tier 2)

**`src/extractor.rs`** — new module that replaces regex heuristics for five languages:

- **Rust** — `function_item`, `impl_item`, `trait_item`, `struct_item`, `enum_item`, `type_item`, `const_item`, `static_item`, `macro_definition`, `mod_item`
- **Go** — `function_declaration`, `method_declaration` (receiver-qualified names), `type_declaration`, `const_declaration`, `var_declaration`
- **Python** — `function_definition`, `class_definition`, `decorated_definition`, `assignment` (ALL_CAPS constants only)
- **TypeScript / TSX** — function, class, method, interface, type alias, enum, arrow function (via `export const`), export statement wrappers
- **JavaScript / JSX / MJS / CJS** — same as TypeScript minus interfaces/type aliases

### Changed — Symbol confidence upgrade

All symbols extracted from Rust, Go, Python, TS, and JS now carry `confidence = 60` (LIP Tier 2) instead of `30`. C/C++, Java, Ruby, PHP, and all other languages continue to use the Tier 1 regex path until their grammars are added.

### Wiring

`mapper.rs:extract_skeleton()` runs the regex path first (to preserve import extraction, which tree-sitter does not do), then calls `crate::extractor::ts_extract()`. When `Some(sigs)` is returned, the regex `signatures` are replaced with the higher-confidence tree-sitter result.

---

## [1.8.0] - 2026-04-09

### Added — sed + awk equivalents

**`codecartographer replace <PATTERN> <REPLACEMENT>`** — regex find-and-replace across project files:
- Replacement string supports `$0` (whole match), `$1`/`$2` (capture groups)
- `--dry-run` — preview what would change (shows colored diff, no writes)
- `--backup` — write `.bak` before modifying each file
- `-i` — case-insensitive; `-w` — whole-word; `--literal` — treat as literal string
- `-C N` — context lines in diff output (default: 3)
- `--glob "*.rs"` / `--exclude "*.gen.go"` / `--path src/api` — scope filters
- `--max-per-file N` — cap replacements per file (0 = unlimited)
- `--no-ignore` — operate on vendor/generated files too
- Colored terminal diff: red `-` for removed, green `+` for added lines
- Summary: files changed, total replacements, backup notice

**`codecartographer extract <PATTERN>`** — capture-group extraction across project files (awk-like):
- `-g N` / `--group N` — capture group index (repeatable; default: 0 = whole match)
- `--count` — aggregate: show frequency table sorted by count descending
- `--dedup` — deduplicate extracted values
- `--sort` — sort output alphabetically (combined with `--count` → by frequency)
- `--format text|json|csv|tsv` — output format
- `--sep SEP` — separator between multiple groups (default: tab)
- `-i` — case-insensitive; `--glob` / `--exclude` / `--path` / `--no-ignore` — scope filters
- `--limit N` — cap total results

**FFI additions** (CKB + CGo consumers):
- `codecartographer_replace_content(path, pattern, replacement, opts_json)`
- `codecartographer_extract_content(path, pattern, opts_json)`

**CKB bridge** — `ReplaceOptions`, `ReplaceResult`, `FileChange`, `DiffLine`, `ExtractOptions`, `ExtractResult`, `ExtractMatch`, `CountEntry` added to `internal/codecartographer`

---

## [1.7.0] - 2026-04-09

### Added — full grep + find parity

**`codecartographer search <PATTERN>`** — complete grep parity:
- `-e PATTERN` — additional patterns OR'd together (like `grep -e`)
- `-i` — case-insensitive
- `-v` — invert match (lines that don't match)
- `-w` — whole-word match (`\b…\b`)
- `-o` — only-matching: print just the matched portion
- `-l` — files-with-matches: print only file paths
- `--files-without-match` — print only files with no matches
- `-c` — count matches per file
- `-A N` / `-B N` / `-C N` — after/before/symmetric context lines
- `--glob "*.rs"` — include filter; `--exclude "*.gen.go"` — exclude filter
- `--path src/api` — restrict to subdirectory
- `--no-ignore` — search vendor/generated/noise files too
- `--limit N` — cap results

**`codecartographer find <PATTERN>`** — complete find parity:
- `--modified-since 24h` / `7d` / `30m` / `3600s` — mtime filter
- `--newer <FILE>` — files newer than reference file's mtime
- `--min-size N` / `--max-size N` — size filter in bytes
- `--max-depth N` — depth limit (0 = root only)
- `--no-ignore` — include vendor/noise directories
- Reports language + human-readable size + ISO-8601 mtime per file

**`codecartographer context --query <PATTERN>`** — bundles ranked skeleton + search results for context injection into models without tool-call support (Qwen3, Llama 3, local models)

**FFI additions** (CKB + any CGo consumer):
- `codecartographer_search_content(path, pattern, opts_json)` — all grep options exposed via JSON; `opts_json` can be null for defaults
- `codecartographer_find_files(path, pattern, limit, opts_json)` — all find options via JSON

**MCP tool expansion** — `search_content` and `find_files` tools now expose all new options as top-level MCP arguments

**CKB bridge** — `SearchContentOptions`, `FindOptions`, `FileCount`, `MatchedTexts`, `FilesWithMatches`, `FilesWithoutMatch`, `FileCounts` added to `internal/codecartographer` package

## [1.6.0] - 2026-04-09

### Added
- **Bot-author filtering** in git history analysis — commits from bots (`[bot]`, `dependabot`, `renovate`, `github-actions`, `snyk-bot`, etc.) are excluded from churn and co-change metrics; eliminates the ~74% noise inflation documented in arXiv 2602.13170
- **Formatting-commit filtering** — commits matching patterns like `cargo fmt`, `prettier`, `rustfmt`, `eslint fix`, `trailing whitespace`, etc. are excluded; same noise gate applied to all git-history paths (`git_churn`, `git_cochange`, FFI wrappers)
- **Personalized PageRank** over the dependency graph (`ranked_skeleton()` in `api.rs`) — 30-iteration power iteration with damping 0.85; personalization vector concentrates weight on focus files; used by:
  - `codecartographer context --focus src/api.rs --budget 8000` — ranked skeleton pruned to token budget, highest-rank files first
  - `codecartographer_ranked_skeleton(path, focus_json, budget)` — new FFI function for CKB context injection
- **CI enforcement** — `codecartographer check` scans the project and exits non-zero if any cycles or layer violations are found; suitable for CI gates (pre-commit hook, GitHub Actions step)
- **Unreferenced export detection** — `rebuild_graph` builds an import-token corpus from all files and marks public symbols whose names don't appear in any import as `unreferenced_exports`; surfaced via:
  - `codecartographer symbols --unreferenced` — file-by-file listing with caveat note
  - `codecartographer_unreferenced_symbols(path)` — new FFI function

## [1.5.0] - 2026-04-09

### Added
- **`codecartographer_version()`** — FFI function returning the library version string; CKB uses this for compatibility checks before calling any other function
- **`codecartographer_git_churn(path, limit)`** — FFI wrapper for git churn analysis; returns `{ "src/api.rs": 42, ... }` (empty object when not a git repo)
- **`codecartographer_git_cochange(path, limit, min_count)`** — FFI wrapper for temporal coupling; returns sorted array of `{ fileA, fileB, count, couplingScore }` pairs
- **`codecartographer_semidiff(path, commit1, commit2)`** — FFI wrapper for semantic diff; returns per-file `{ path, status, added[], removed[] }` using skeleton extraction at each commit
- `mod git_analysis` added to `lib.rs` — git subprocess helpers are now available to all FFI callers, not just the CLI binary

## [1.4.0] - 2026-04-09

### Added
- **CCE integration** — `compressor.py` now compresses context through [ContextCompressionEngine](https://github.com/SimplyLiz/ContextCompressionEngine), reducing token usage while preserving code verbatim
  - `python compressor.py --messages chat.json --token-budget 8000` compresses any message array to fit a token budget
  - CodeCartographer dependency context is appended as a system message before compression
  - CCE path auto-discovered via `CCE_DIST` env var, `.codecartographer/cce_dist` config, or sibling directory
- **`tools/cce_bridge.mjs`** — thin stdin/stdout Node.js bridge to CCE; normalises messages (adds `id`/`index`), accepts `--cce-dist` flag
- **`launch.py` CCE setup** — steps 5–6 check Node.js 20+ and build CCE; dist path saved to `.codecartographer/cce_dist` for `compressor.py` to use
  - `--cce-path <dir>` overrides the default sibling-directory assumption

## [1.3.0] - 2026-04-09

### Added
- **`cochange`** — temporal coupling analysis from git history; surfaces files that always change together without an import link (`codecartographer cochange --min-count 3`)
- **`hotspots`** — churn × complexity ranking with CRITICAL / HIGH / MODERATE / LOW tiers (`codecartographer hotspots --top 10`)
- **`dead`** — dead code candidates based on in-degree = 0 in the dependency graph (`codecartographer dead`)
- **`diagram`** — exports dependency graph as Mermaid or Graphviz DOT with role-based colouring (`codecartographer diagram --format mermaid -o graph.md`)
- **`llmstxt`** — generates `llms.txt` index (entry points first, sorted by symbol count) for LLM inference-time context (`codecartographer llmstxt`)
- **`claudemd`** — generates a `CLAUDE.md` architecture guide covering entry points, core modules, hotspots, cycles, and hidden coupling (`codecartographer claudemd`)
- **`semidiff`** — function-level semantic diff between two commits using skeleton extraction (`codecartographer semidiff HEAD~1`)
- **`git_analysis` module** — `git_churn`, `git_cochange`, `git_show_file`, `git_diff_files` helpers (binary-only; not exposed via C FFI)
- **Role classification** — every `GraphNode` now carries `role` (entry / core / utility / leaf / dead / bridge / standard), `churn`, `hotspot_score`, and `is_dead`
- **`CoChangePair`** in `ProjectGraphResponse` — populated by `enrich_with_git()`

## [1.2.0] - 2026-04-09

### Added
- **`launch.py`** — cross-platform Python installer replacing `install.sh`; supports Linux, macOS, and Windows; updates shell RC automatically
- **`deps` command** — `codecartographer deps <target> --format json` outputs dependency graph for a target module as JSON
- **`serve` command** — `codecartographer serve` starts the MCP server with full JSON-RPC 2.0 stdio transport
- **MCP tools** — `get_symbol_context` (filter signatures by symbol name) and `get_blast_radius` (dependencies + dependents up to depth limit)
- **`#[serde(rename = "type")]`** fix on `McpInputSchema` and `McpProperty` so tool schemas serialise correctly

### Fixed
- `compressor.py` called a non-existent `cmp deps` subcommand; now calls `codecartographer deps`
- `verify_ignore.py` hardcoded the old `cmp` binary path; now resolves the correct platform binary
- Stale "architect" branding in `install.sh`

## [1.1.0] - 2025-04-07

### Changed
- Renamed binary from `architect` to `codecartographer`
- Updated package description to "CodeCartographer for Architectural Intelligence"

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
