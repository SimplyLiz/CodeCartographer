# Nyx.Navigator

> Deterministic codebase mapper for AI context injection.

Nyx.Navigator packages your repository into a structured snapshot an AI can reason about. It sits between your codebase and your AI assistant — Claude, Cursor, GPT-4, or any model with a context window.

## How it works

1. Run `navigator` in any repo
2. Pick a mode — map (skeletons) or source (full content)
3. The snapshot is written to disk and optionally copied to clipboard
4. Paste it into your AI chat, or let the MCP server inject it automatically

```
  Project : my-app  (42 source files)

  map     ~18k tokens   signatures & structure only   (recommended)
  source  ~310k tokens  full file content
  diagram               visualise dependency graph
  query                 answer a specific question about the code

What would you like to do? [map/source/diagram/query/quit]:
```

## Quick Start

```bash
# Build and install
cd mapper-core/nyx-navigator && cargo build --release
cp target/release/navigator ~/.local/bin/navigator

# Initialise project config
navigator init

# Interactive overview — shows token estimates, lets you pick a mode
navigator

# Or go directly
navigator map      # skeleton only (~90% fewer tokens than full source)
navigator source   # full source code
navigator query "how does authentication work?"
```

## Context Modes

| Command | What it sends | When to use |
|---------|--------------|-------------|
| `navigator map` | Imports + signatures only | Daily use, architecture questions |
| `navigator source` | Full file content | Debugging, implementation review |
| `navigator copy` | Full source to clipboard only (no disk write) | One-shot paste |
| `navigator context --focus <FILE> --budget 8000` | PageRank-ranked skeleton pruned to token budget | Targeted deep dives |
| `navigator query <QUESTION>` | Search → PageRank → skeleton in one step | Specific questions |
| `navigator sync` | Incremental update (changed files only) | Keep snapshot fresh |
| `navigator watch` | Live watcher, updates skeleton on save | Ongoing sessions |

## Architecture & Analysis

| Command | Description |
|---------|-------------|
| `navigator health` | Health score 0–100 (cycles, bridges, god modules, violations) |
| `navigator simulate --module <FILE>` | Predict ripple effects before making a change |
| `navigator check` | CI gate — exits non-zero on cycles or layer violations |
| `navigator dead` | Dead code candidates (in-degree = 0) |
| `navigator symbols --unreferenced` | Public exports not referenced anywhere |
| `navigator hotspots` | High churn × high complexity files |
| `navigator cochange --min-count 3` | Temporal coupling — files that always change together |
| `navigator shotgun` | Shotgun surgery candidates (high co-change dispersion) |
| `navigator semidiff HEAD~1` | Function-level semantic diff between two commits |
| `navigator evolution --days 30` | Architectural trends over time |
| `navigator path --from <A> --to <B>` | Shortest import path between two modules |
| `navigator deps <MODULE>` | Dependencies of a module as JSON |
| `navigator todo` | TODO/FIXME/HACK density across source files |
| `navigator languages` | Languages detected and their file counts |

## Diagram

| Command | Description |
|---------|-------------|
| `navigator diagram` | Dependency graph (Mermaid by default) |
| `navigator diagram --format dot\|ascii` | Graphviz DOT or ASCII tree |
| `navigator diagram -o graph.html` | Interactive self-contained HTML explorer |
| `navigator diagram -o graph.svg\|.png` | SVG/PNG via `mmdc` or `dot` |
| `navigator diagram --call-graph FILE` | Function-level call graph for a single file (Rust/Python) |
| `navigator diagram --call-graph FILE --format sequence` | Mermaid `sequenceDiagram` — function call order within a file |
| `navigator diagram --call-graph FILE --format class` | Mermaid `classDiagram` — structs, classes, interfaces with fields and relationships |
| `navigator diagram --format quadrant` | Mermaid `quadrantChart` — churn × complexity scatter (top-right = refactor now) |
| `navigator diagram --call-graph FILE --format er` | Mermaid `erDiagram` — entity-relationship view inferred from struct fields and type annotations |
| `navigator diagram --blast-radius MODULE` | Target + direct deps + direct dependents |
| `navigator diagram --focus FILE [--depth N]` | BFS neighborhood around a module |
| `navigator diagram --group-by-folder DEPTH` | Collapse graph to folder granularity |
| `navigator diagram --color-by-owner` | Node fill by dominant git author |
| `navigator diagram --cochange-threshold N` | Overlay co-change edges |
| `navigator diagram --docs-only` | Doc-map: Markdown/YAML/TOML/JSON + referenced code |

Sequence, class, quadrant, and ER formats all output Mermaid syntax, so `--output out.svg` works via `mmdc` and IDEs that render Mermaid inline get diagrams without extra tooling.

### Examples — this repo

**Full module dependency graph** (`navigator diagram --format dot -o graph.svg`)

![Module dependency graph](docs/images/module-graph.svg)

**Blast radius of `api.rs`** — the central hub and everything it pulls (`navigator diagram --blast-radius src/api.rs --format dot -o blast.svg`)

![Blast radius of api.rs](docs/images/blast-radius-api.svg)

**Focus neighborhood of `main.rs`** — direct imports only (`navigator diagram --focus src/main.rs --depth 1 --format dot -o focus.svg`)

![Focus neighborhood of main.rs](docs/images/main-focus.svg)

**Function-level call graph for `diagram.rs`** (`navigator diagram --call-graph src/diagram.rs --format dot -o calls.svg`)

![Call graph for diagram.rs](docs/images/call-graph-diagram.svg)

**Sequence diagram of `diagram.rs`** — function call order within the renderer (`navigator diagram --call-graph src/diagram.rs --format sequence`)

```mermaid
sequenceDiagram
    participant render
    participant collapse_by_folder
    participant blast_radius_selection
    participant bfs_from_anchor
    participant docs_only_selection
    participant top_by_degree
    participant compute_overlays
    participant render_mermaid
    participant render_dot
    participant render_ascii
    participant ascii_walk
    participant ascii_label
    render->>collapse_by_folder: call
    render->>blast_radius_selection: call
    render->>bfs_from_anchor: call
    render->>docs_only_selection: call
    render->>top_by_degree: call
    render->>compute_overlays: call
    render->>render_mermaid: call
    render->>render_dot: call
    render->>render_ascii: call
    render_ascii->>ascii_walk: call
    ascii_walk->>ascii_label: call
    ascii_walk->>ascii_walk: call
    render_ascii->>ascii_label: call
    render_mermaid->>role_class_suffix: call
    render_dot->>role_color_dot: call
```

**Class diagram of `class_graph.rs`** — the UML extraction data model (`navigator diagram --call-graph src/class_graph.rs --format class`)

```mermaid
classDiagram
    class ClassGraph {
        +classes Vec~ClassNode~
        +relationships Vec~ClassRelationship~
        +language str
    }
    class ClassNode {
        +name String
        +kind ClassKind
        +fields Vec~FieldDef~
        +methods Vec~MethodDef~
    }
    class FieldDef {
        +name String
        +type_annotation String
        +visibility Vis
    }
    class MethodDef {
        +name String
        +params String
        +return_type String
        +visibility Vis
        +is_static bool
        +is_constructor bool
    }
    class ClassKind {
        <<enumeration>>
        Struct
        Class
        Interface
        Trait
        Enum
    }
    class Vis {
        <<enumeration>>
        Public
        Private
        Protected
    }
    ClassGraph "1" --> "*" ClassNode : classes
    ClassGraph "1" --> "*" ClassRelationship : relationships
    ClassNode "1" --> "*" FieldDef : fields
    ClassNode "1" --> "*" MethodDef : methods
    ClassNode --> ClassKind : kind
    FieldDef --> Vis : visibility
    MethodDef --> Vis : visibility
```

**Quadrant chart — churn × complexity** (`navigator diagram --format quadrant`)

> Bottom-left = stable (leave it). Top-right = danger zone (refactor now). Top-left = risky debt (complex but rarely touched — schedule a refactor). Bottom-right = hotspots (high churn but simple — add tests).

```mermaid
quadrantChart
    title Churn vs Complexity
    x-axis Low Churn --> High Churn
    y-axis Low Complexity --> High Complexity
    quadrant-1 Danger zone
    quadrant-2 Risky debt
    quadrant-3 Stable
    quadrant-4 Hotspots
    api.rs: [0.26, 0.82]
    main.rs: [0.50, 0.34]
    mapper.rs: [0.26, 0.49]
    diagram.rs: [0.26, 0.99]
    lib.rs: [0.26, 0.60]
    mcp.rs: [0.99, 0.54]
    search.rs: [0.01, 0.51]
    webhooks.rs: [0.01, 0.37]
```

**ER diagram of `call_graph.rs`** — entity-relationship view of the call-graph data model (`navigator diagram --call-graph src/call_graph.rs --format er`)

```mermaid
erDiagram
    FileCallGraph {
        Vec functions
        Vec calls
        usize unresolved_count
        str language
    }
    FunctionInfo {
        String qualified
        String simple
        u32 line
        str kind
    }
    Resolver {
        HashMap by_qualified
        HashMap by_simple
    }
    FileCallGraph ||--o{ FunctionInfo : "has"
```

## Search & File Tools

| Command | Description |
|---------|-------------|
| `navigator search <PATTERN>` | Grep-like content search (`-i -v -w -A -B -C`, `--glob`, `--path`) |
| `navigator find <PATTERN>` | File find by glob (`--modified-since 24h`, `--min-size`, `--max-depth`) |
| `navigator replace <PATTERN> <REPLACEMENT>` | Regex find-and-replace (`--dry-run`, `--backup`, capture groups) |
| `navigator extract <PATTERN>` | Capture-group extraction (`--format text\|json\|csv\|tsv`) |

## Context Quality

| Command | Description |
|---------|-------------|
| `navigator context-health [FILE]` | Score a context bundle: signal density, entropy, position health (A–F) |
| `navigator llmstxt` | Generate `llms.txt` project index |
| `navigator claudemd` | Generate `CLAUDE.md` architecture guide |

## Layers & Snapshots

| Command | Description |
|---------|-------------|
| `navigator layers` | Manage architectural layer definitions (`layers.toml`) |
| `navigator snapshot` | Save or compare architecture snapshots |
| `navigator status` | Show project status |

## MCP Server

```bash
navigator serve   # stdio JSON-RPC 2.0 — connects to Claude Code, Cursor, etc.
```

Exposes 40 tools over Model Context Protocol. Skeleton tools:

| Tool | Description |
|------|-------------|
| `skeleton_map` | Full project skeleton (all files) |
| `ranked_skeleton` | Token-budget skeleton ranked by PageRank, optionally personalised to focus files |
| `focused_skeleton` | Seed file + N import-hops (importers + importees), enriched with churn and test markers |
| `diff_skeleton` | Files changed between two commits + their immediate importers |
| `search_skeleton` | Skeleton sections for files matching a keyword — path-first, then symbol names |

Other highlights: `get_blast_radius`, `renderArchitecture`, `search_content`, `semidiff`, `doc_index`, `query_context`, `shotgun_surgery`, `context_health`.

`renderArchitecture` returns Mermaid or DOT directly — IDEs that render Mermaid inline get paste-able diagrams without extra tooling.

## Layer Enforcement

Prevent architectural drift with `layers.toml`:

```toml
[layers]
ui = ["components", "pages"]
services = ["api", "auth"]
db = ["models"]

[allowed_flows]
ui -> services
services -> db
```

Detects: BackCalls (db→ui), SkipCalls (ui→db), CircularCrossLayer, DirectForeignImport.

## Architecture

### Module dependency graph

```mermaid
graph LR
    subgraph Extraction["Extraction"]
        scanner["scanner\nfile scan · gitignore"]
        mapper["mapper\n15+ languages · regex"]
        extractor["extractor\ntree-sitter · Tier 2"]
        git_analysis["git_analysis\nchurn · co-change · BM25"]
    end

    subgraph Analysis["Analysis"]
        api["api\ndep graph · health · simulate"]
        layers["layers\nlayer config · violations"]
        call_graph["call_graph\nfn-level graph"]
        search["search\ngrep · BM25 · find"]
        token_metrics["token_metrics\ncontext health A–F"]
    end

    subgraph Rendering["Rendering"]
        diagram["diagram\nMermaid · DOT · ASCII\nsequence · class · quadrant · ER"]
        class_graph["class_graph\nUML extraction · Rust/Py/TS/Go"]
        html_export["html_export\ninteractive HTML"]
        diagram_export["diagram_export\nSVG · PNG"]
    end

    subgraph Integration["Integration"]
        mcp["mcp\nMCP server · 30+ tools"]
        webhooks["webhooks\nchange notifications"]
        sync["sync\nincremental state"]
        memory["memory\npersistent state"]
        formatter["formatter\ntoken budget · compress"]
    end

    extractor --> mapper
    mapper --> api
    layers --> api
    scanner --> api
    scanner --> search
    scanner --> sync

    api --> diagram
    api --> html_export
    api --> call_graph
    api --> mcp
    call_graph --> diagram
    class_graph --> diagram
    diagram --> diagram_export
    memory --> formatter
    memory --> sync
```

### How `navigator map` flows

```mermaid
sequenceDiagram
    participant CLI as main.rs
    participant SC as scanner
    participant MP as mapper
    participant EX as extractor
    participant FM as formatter

    CLI->>SC: scan_files_with_noise_tracking(path)
    SC-->>CLI: Vec<FileEntry> + ignored noise list
    CLI->>MP: extract_skeleton(files)
    MP->>EX: tree-sitter parse (Rust/Go/Py/TS/JS/C)
    EX-->>MP: Symbols (confidence=60)
    MP-->>CLI: Vec<MappedFile>
    CLI->>FM: format output (token budget, target)
    FM-->>CLI: compressed skeleton XML
    CLI-->>User: navigator_map.xml + token report
```

## Token Efficiency

`map` mode achieves **~90% token reduction** vs full source:
- Full source: ~5,000 tokens/module
- Skeleton: ~200 tokens/module

`context-health` scores bundles on six metrics: signal density, compression density, position health, entity density, utilisation headroom, dedup ratio. Composite score A–F.

## Nyx.Navigator vs CKB

| Aspect | Nyx.Navigator | CKB |
|--------|--------------|-----|
| View | Macro (file/module) | Micro (symbol/AST) |
| Speed | Fast (regex + tree-sitter) | Deep (AST) |
| Purpose | Map, warn, predict, inject context | Analyze, refactor |
| Output | Skeleton XML / source context | Call graphs, refs |

**The handoff:** Nyx.Navigator identifies where to look; CKB does deep analysis there.

```mermaid
graph LR
    Nyx.Navigator["Nyx.Navigator\nmacro view — file/module\nfast · regex + tree-sitter"]
    CKB["CKB\nmicro view — symbol/AST\ndeep · full parse"]

    Nyx.Navigator -->|"webhook on graph change\n+ blast radius hint"| CKB
    CKB -->|"deep symbol analysis\nat identified hotspots"| Nyx.Navigator
```

## Author

SimplyLiz
