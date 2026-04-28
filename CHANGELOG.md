# Changelog

All notable changes to Nyx.Navigator will be documented in this file.

## [Unreleased]

### Added — multi-symbol reach and answer --then

**`navigator reach SYMBOL [SYMBOL ...]`** — passing two or more symbols
produces a unified intersection view. Callers are merged and deduped by
`(file, line)`. Callees appearing in more than one root are annotated
`[shared]`. Depth-2 types present in multiple results are promoted to a
"shared types" section above the ordinary depth-2 tail, rather than being
buried at the end. Ambiguous or not-found symbols are reported per-symbol
and skipped; the remaining results still render.

**`navigator answer QUESTION --then N`** — after printing the evidence
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

`navigator diagram --entry FILE::FUNCTION --format sequence [--depth N]` traces
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

**`src/main.rs`** — `--entry FILE::FUNCTION` flag on `navigator diagram`; triggers
a full project scan to build the symbol index before tracing.

Spike validated on `diagram_mode` (the target from charts.md): 13 modules,
60 resolved edges in correct call order; direct-import edges unqualified,
transitive edges correctly annotated `(~)`.

### Added — quadrant chart and ER diagram formats (PR #8)

Two new `--format` values for `navigator diagram`:

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
  All 38 tools have a title; `McpServerInfo` gains one too (`"Nyx Navigator"`).
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

`navigator diagram --call-graph PATH` now extracts a file-local call graph
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

**`src/main.rs`** — `--call-graph FILE` flag on `navigator diagram`. When set,
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
but never provided, so Go consumers linking `libnavigator.a` got
undefined-symbol errors at `navigator`-tagged builds.

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

### Added — `renderArchitecture` MCP tool + `navigator_render_architecture` FFI

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

**`src/lib.rs`** — `navigator_render_architecture(path, format, focus, depth, max_nodes)`:
- Defaults: `format` null → `"mermaid"`, `depth` 0 → 2, `max_nodes` 0 → 40
- Returns JSON `{ diagram, truncated, format, nodeCount }`
- cbindgen regenerates `include/navigator.h` automatically

**`src/main.rs`** — CLI `diagram_mode` now delegates to `diagram::render()`, so CLI and FFI outputs stay identical.

### Added — tree-sitter symbol localization for `libnavigator.a`

`libnavigator.a` now ships with its tree-sitter runtime and grammar
symbols hidden from the global symbol resolver, so consumers that also
link tree-sitter (e.g. Go projects using `go-tree-sitter`) no longer
trip duplicate-symbol errors at link time. This matters beyond the
ergonomic complaint: if both copies were left global, the linker would
bind Nyx.Navigator's Rust code to whichever archive came first on the
command line — and if the two tree-sitter versions drifted in struct
layout, the loser's callers would walk the wrong struct and produce
silent memory corruption.

**`scripts/localize-tree-sitter-symbols.sh`** (new):
- Partial-links all `.o` members of `libnavigator.a` into one combined relocatable object via `cc -nostdlib -Wl,-r`, so Nyx.Navigator's internal `ts_*`/`tree_sitter_*` references resolve within the archive
- `rust-objcopy --wildcard --localize-symbol='ts_*' --localize-symbol='tree_sitter_*'` then marks those symbols local on the combined object; `navigator_*` FFI entry points stay global
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

### Added — `watch_graph` MCP tool + NYX.md preset awareness

**`src/mcp.rs`** — `watch_graph` tool (#30):
- Watches a directory recursively for source file changes (`.rs`, `.go`, `.py`, `.ts`, `.js`, `.dart`) using the `notify` crate
- Streams incremental graph events as newline-delimited JSON: `{ kind, path, timestamp_ms }`
- `kind` values: `file_reindexed` | `graph_updated`
- `timeout_secs` argument (default 30, max 300); returns event count summary on completion

**`src/mcp.rs` + `src/token_metrics.rs`** — NYX.md `[commands]` preset integration:
- `context_health` now reads the `[commands]` section from `NYX.md` at the project root
- Preset names are included in the health report as `nyx_commands: [...]`
- Warns if any preset command string references a file that participates in a detected dependency cycle

**`src/token_metrics.rs`**:
- `ContextHealthReport.nyx_commands: Option<Vec<String>>` field
- `parse_nyx_commands(root) -> HashMap<String, String>` — parses `[commands]` key=value pairs from `NYX.md`

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

**CLI**: `navigator shotgun [--commits N] [--top N] [--min-partners N]` — ranked shotgun surgery candidates with HIGH/MODERATE/LOW tiers

**MCP tool**: `shotgun_surgery` — tool #29; returns `CoChangeDispersion[]` ranked by dispersion score

**FFI**: `navigator_shotgun_surgery(path, limit, min_partners) -> *mut c_char` — #19

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

**CLI**: `navigator context-health [FILE] [--model claude|gpt4|llama|gpt35] [--window N] [--format text|json]`

**MCP tool**: `context_health` — tool #27; scores any context string passed directly as an argument

**FFI**: `navigator_context_health(content, opts_json) -> *mut c_char` for CKB

**13 tests** covering all individual metrics, composite analysis, and warning generation

### Added — PKG retrieval pipeline (`query_context`, `navigator query`)

**MCP tool #28: `query_context`** — single-call retrieval pipeline replacing the manual search → ranked_skeleton → context_health sequence:
1. Searches the codebase for files matching the query (regex)
2. Uses matching files as the PageRank personalization seed
3. Builds a token-budget-aware skeleton ranked by relevance
4. Scores the bundle with context_health
5. Returns `{ context, filesUsed, focusFiles, totalTokens, health }` — ready to inject

**CLI**: `navigator query <QUERY> [--budget N] [--model claude|gpt4|llama|gpt35] [--format text|json]`

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

**`navigator replace <PATTERN> <REPLACEMENT>`** — regex find-and-replace across project files:
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

**`navigator extract <PATTERN>`** — capture-group extraction across project files (awk-like):
- `-g N` / `--group N` — capture group index (repeatable; default: 0 = whole match)
- `--count` — aggregate: show frequency table sorted by count descending
- `--dedup` — deduplicate extracted values
- `--sort` — sort output alphabetically (combined with `--count` → by frequency)
- `--format text|json|csv|tsv` — output format
- `--sep SEP` — separator between multiple groups (default: tab)
- `-i` — case-insensitive; `--glob` / `--exclude` / `--path` / `--no-ignore` — scope filters
- `--limit N` — cap total results

**FFI additions** (CKB + CGo consumers):
- `navigator_replace_content(path, pattern, replacement, opts_json)`
- `navigator_extract_content(path, pattern, opts_json)`

**CKB bridge** — `ReplaceOptions`, `ReplaceResult`, `FileChange`, `DiffLine`, `ExtractOptions`, `ExtractResult`, `ExtractMatch`, `CountEntry` added to `internal/navigator`

---

## [1.7.0] - 2026-04-09

### Added — full grep + find parity

**`navigator search <PATTERN>`** — complete grep parity:
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

**`navigator find <PATTERN>`** — complete find parity:
- `--modified-since 24h` / `7d` / `30m` / `3600s` — mtime filter
- `--newer <FILE>` — files newer than reference file's mtime
- `--min-size N` / `--max-size N` — size filter in bytes
- `--max-depth N` — depth limit (0 = root only)
- `--no-ignore` — include vendor/noise directories
- Reports language + human-readable size + ISO-8601 mtime per file

**`navigator context --query <PATTERN>`** — bundles ranked skeleton + search results for context injection into models without tool-call support (Qwen3, Llama 3, local models)

**FFI additions** (CKB + any CGo consumer):
- `navigator_search_content(path, pattern, opts_json)` — all grep options exposed via JSON; `opts_json` can be null for defaults
- `navigator_find_files(path, pattern, limit, opts_json)` — all find options via JSON

**MCP tool expansion** — `search_content` and `find_files` tools now expose all new options as top-level MCP arguments

**CKB bridge** — `SearchContentOptions`, `FindOptions`, `FileCount`, `MatchedTexts`, `FilesWithMatches`, `FilesWithoutMatch`, `FileCounts` added to `internal/navigator` package

## [1.6.0] - 2026-04-09

### Added
- **Bot-author filtering** in git history analysis — commits from bots (`[bot]`, `dependabot`, `renovate`, `github-actions`, `snyk-bot`, etc.) are excluded from churn and co-change metrics; eliminates the ~74% noise inflation documented in arXiv 2602.13170
- **Formatting-commit filtering** — commits matching patterns like `cargo fmt`, `prettier`, `rustfmt`, `eslint fix`, `trailing whitespace`, etc. are excluded; same noise gate applied to all git-history paths (`git_churn`, `git_cochange`, FFI wrappers)
- **Personalized PageRank** over the dependency graph (`ranked_skeleton()` in `api.rs`) — 30-iteration power iteration with damping 0.85; personalization vector concentrates weight on focus files; used by:
  - `navigator context --focus src/api.rs --budget 8000` — ranked skeleton pruned to token budget, highest-rank files first
  - `navigator_ranked_skeleton(path, focus_json, budget)` — new FFI function for CKB context injection
- **CI enforcement** — `navigator check` scans the project and exits non-zero if any cycles or layer violations are found; suitable for CI gates (pre-commit hook, GitHub Actions step)
- **Unreferenced export detection** — `rebuild_graph` builds an import-token corpus from all files and marks public symbols whose names don't appear in any import as `unreferenced_exports`; surfaced via:
  - `navigator symbols --unreferenced` — file-by-file listing with caveat note
  - `navigator_unreferenced_symbols(path)` — new FFI function

## [1.5.0] - 2026-04-09

### Added
- **`navigator_version()`** — FFI function returning the library version string; CKB uses this for compatibility checks before calling any other function
- **`navigator_git_churn(path, limit)`** — FFI wrapper for git churn analysis; returns `{ "src/api.rs": 42, ... }` (empty object when not a git repo)
- **`navigator_git_cochange(path, limit, min_count)`** — FFI wrapper for temporal coupling; returns sorted array of `{ fileA, fileB, count, couplingScore }` pairs
- **`navigator_semidiff(path, commit1, commit2)`** — FFI wrapper for semantic diff; returns per-file `{ path, status, added[], removed[] }` using skeleton extraction at each commit
- `mod git_analysis` added to `lib.rs` — git subprocess helpers are now available to all FFI callers, not just the CLI binary

## [1.4.0] - 2026-04-09

### Added
- **CCE integration** — `compressor.py` now compresses context through [ContextCompressionEngine](https://github.com/SimplyLiz/ContextCompressionEngine), reducing token usage while preserving code verbatim
  - `python compressor.py --messages chat.json --token-budget 8000` compresses any message array to fit a token budget
  - Nyx.Navigator dependency context is appended as a system message before compression
  - CCE path auto-discovered via `CCE_DIST` env var, `.navigator/cce_dist` config, or sibling directory
- **`tools/cce_bridge.mjs`** — thin stdin/stdout Node.js bridge to CCE; normalises messages (adds `id`/`index`), accepts `--cce-dist` flag
- **`launch.py` CCE setup** — steps 5–6 check Node.js 20+ and build CCE; dist path saved to `.navigator/cce_dist` for `compressor.py` to use
  - `--cce-path <dir>` overrides the default sibling-directory assumption

## [1.3.0] - 2026-04-09

### Added
- **`cochange`** — temporal coupling analysis from git history; surfaces files that always change together without an import link (`navigator cochange --min-count 3`)
- **`hotspots`** — churn × complexity ranking with CRITICAL / HIGH / MODERATE / LOW tiers (`navigator hotspots --top 10`)
- **`dead`** — dead code candidates based on in-degree = 0 in the dependency graph (`navigator dead`)
- **`diagram`** — exports dependency graph as Mermaid or Graphviz DOT with role-based colouring (`navigator diagram --format mermaid -o graph.md`)
- **`llmstxt`** — generates `llms.txt` index (entry points first, sorted by symbol count) for LLM inference-time context (`navigator llmstxt`)
- **`claudemd`** — generates a `CLAUDE.md` architecture guide covering entry points, core modules, hotspots, cycles, and hidden coupling (`navigator claudemd`)
- **`semidiff`** — function-level semantic diff between two commits using skeleton extraction (`navigator semidiff HEAD~1`)
- **`git_analysis` module** — `git_churn`, `git_cochange`, `git_show_file`, `git_diff_files` helpers (binary-only; not exposed via C FFI)
- **Role classification** — every `GraphNode` now carries `role` (entry / core / utility / leaf / dead / bridge / standard), `churn`, `hotspot_score`, and `is_dead`
- **`CoChangePair`** in `ProjectGraphResponse` — populated by `enrich_with_git()`

## [1.2.0] - 2026-04-09

### Added
- **`launch.py`** — cross-platform Python installer replacing `install.sh`; supports Linux, macOS, and Windows; updates shell RC automatically
- **`deps` command** — `navigator deps <target> --format json` outputs dependency graph for a target module as JSON
- **`serve` command** — `navigator serve` starts the MCP server with full JSON-RPC 2.0 stdio transport
- **MCP tools** — `get_symbol_context` (filter signatures by symbol name) and `get_blast_radius` (dependencies + dependents up to depth limit)
- **`#[serde(rename = "type")]`** fix on `McpInputSchema` and `McpProperty` so tool schemas serialise correctly

### Fixed
- `compressor.py` called a non-existent `cmp deps` subcommand; now calls `navigator deps`
- `verify_ignore.py` hardcoded the old `cmp` binary path; now resolves the correct platform binary
- Stale "architect" branding in `install.sh`

## [1.1.0] - 2025-04-07

### Changed
- Renamed binary from `architect` to `navigator`
- Updated package description to "Nyx.Navigator for Architectural Intelligence"

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
