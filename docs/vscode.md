# CodeCartographer VSCode Extension — Specification

## Purpose

Surface per-file and project-level architectural metrics inside the editor without context-switching to the terminal. The extension is a thin shell over the existing CLI — it runs CodeCartographer commands with `--json`, parses the output, and renders it as decorators, tree views, and webview panels.

---

## Architecture

```
editor events → Extension Host
                    ↓  spawn subprocess or Language Client → MCP
       codecartographer <cmd> --json
                    ↓
               parsed JSON → VS Code API (decorations, tree, webview)
```

**Data source**: `codecartographer serve` (MCP over stdio) for live queries; cached `--json` CLI runs for heavier analyses (hotspots, health) that run on save or on command.

**Language**: TypeScript. Uses the VS Code Extension API only — no bundled Rust, no network calls.

---

## Views

### 1. Sidebar Panel — "CodeCartographer"

A single Activity Bar entry with three collapsible tree sections.

#### 1a. Project Health

Runs: `codecartographer health --json` on workspace open and on explicit refresh.

Displays:

```
▾ Project Health                      [↻]
    Score        72.4 / 100
    Bridges      14
    Cycles       3
    God Modules  2
    Layer Violations  0
```

Each metric is a tree item. Clicking "Cycles" or "Bridges" expands a child list of the offending files (populated from the same JSON — `graph.nodes` where `role == "bridge"` or from the cycles list in the health output). Clicking a file navigates to it.

Color coding: score ≥ 80 → green label, 60–79 → yellow, < 60 → red.

Refresh button reruns the analysis. No automatic polling — health is expensive.

#### 1b. File Metrics (active file)

Updates whenever the active editor changes. Runs `codecartographer hotspots --json` and `codecartographer dead --json` on first open (cached per session), then looks up the active file in the cached results.

```
▾ src/api.rs
    Hotspot score   87.3  CRITICAL
    Churn           142 commits
    Signatures      31
    Owner           Lisa
    Bus factor      2 authors
    Role            bridge
    TODO / FIXME    4
```

Each row is a tree item. No actions — informational only.

**Empty state**: "No data for this file — run analysis" with a button.

#### 1c. Co-Change Partners

For the active file: calls `codecartographer cochange --json` (cached per session), filters pairs where `file_a` or `file_b` matches the active file, shows the top 5 by `coupling_score`.

```
▾ Co-Change Partners
    src/scanner.rs     0.91 coupling
    src/mapper.rs      0.87 coupling
    src/layers.rs      0.74 coupling
```

Clicking a partner opens it. Items with `coupling_score >= 0.8` and no corresponding import edge (derived from `get_project_graph` MCP call) are marked ⚠ "hidden dependency".

---

### 2. Dependency Diagram (Webview)

Command: `CodeCartographer: Show Diagram`

Opens a webview panel. Renders `codecartographer diagram --format mermaid --json` output using [Mermaid.js](https://mermaid.js.org/) loaded from CDN (or bundled).

Controls rendered inside the webview:
- **Focus file**: text input, pre-filled with the active file. Runs `codecartographer diagram --focus <file> --depth 2 --format mermaid`.
- **Depth**: stepper 1–4.
- **Blast radius**: checkbox. Runs `codecartographer diagram --blast-radius <file>`.
- **Edge kind filter**: checkboxes for `runtime`, `test`, `doc`. Filters client-side from the `edge_type` field on each edge in the mermaid source.
- **Export**: button downloads the SVG.

The diagram re-renders on focus-file change (debounced 300 ms). Clicking a node in the diagram navigates the editor to that file.

---

### 3. Simulate Impact (Webview)

Command: `CodeCartographer: Simulate Impact for Active File`

Runs: `codecartographer simulate --module <active-file> --json`

Displays a three-column layout:

```
Direct Callers (N)      Transitive Impact (M)      Risk
─────────────────       ──────────────────────      ────
src/main.rs             src/formatter.rs            HIGH
src/api.rs              src/uc_sync.rs
                        …
```

Each entry is clickable (navigate to file).

When there are staged changes: adds a "Staged changes" badge and runs `codecartographer simulate --staged --json` instead, showing only changed modules. A "Fail on cycle?" toggle maps to `--fail-on-cycle` (result highlighted red if the flag would have triggered).

---

### 4. Path Finder (Quick Pick)

Command: `CodeCartographer: Find Dependency Path`

Two sequential Quick Pick inputs: "From file" → "To file" (with fuzzy file completion from the workspace file list).

Runs: `codecartographer path --from <f1> --to <f2> --json`

On result, opens a notification with the hop count and a "Show path" action that opens a webview listing each hop as a clickable file link.

On no path found: notification "No dependency path from X to Y."

---

## Editor Decorations

### File Explorer Badges

After health/hotspot analysis completes, the extension adds `FileDecorationProvider` entries on files in the Explorer tree:

| Condition | Badge | Color |
|-----------|-------|-------|
| hotspot score > 80 | `●` | red |
| hotspot score 50–80 | `●` | orange |
| `role == "bridge"` | `⬡` | yellow |
| `is_dead == true` | `◌` | grey |
| bus_factor == 1 | `!` | orange |

Badges are additive: a file that is both a bridge and a hotspot shows `⬡●`. Limit to the two highest-priority badges to avoid clutter.

### Gutter Decorations (Editor)

When a file is opened that appears in the hotspot results, the extension adds a gutter icon on the first line of each function whose name appears in the top-N hotspot signatures list. Icon: a small flame SVG (bundled). Hover tooltip: "Hotspot — churn: N commits, score: X.Y".

Implementation: `vscode.window.createTextEditorDecorationType`, match signature names via the `signatures` array from `codecartographer hotspots --json` output.

### Status Bar Item

Always visible when a CodeCartographer workspace is detected (`.codecartographer/` dir or `codecartographer.toml` in the workspace root).

```
⬡ 72  ⚡ 14 hotspots
```

- `⬡ 72` = health score, color-coded (green/yellow/red)
- `⚡ 14 hotspots` = count of CRITICAL + HIGH hotspots

Clicking the status bar item opens the CodeCartographer sidebar panel.

---

## Commands

All commands appear in the Command Palette under the prefix `CodeCartographer: `.

| Command | Description | When run |
|---------|-------------|----------|
| `CodeCartographer: Refresh Analysis` | Reruns health + hotspots + dead | Manual |
| `CodeCartographer: Show Diagram` | Opens diagram webview | Manual |
| `CodeCartographer: Simulate Impact` | Opens simulate webview for active file | Manual |
| `CodeCartographer: Find Dependency Path` | Opens path-finder quick pick | Manual |
| `CodeCartographer: Show Health` | Focuses sidebar health section | Manual |
| `CodeCartographer: Snapshot Save` | Prompts for tag, runs `snapshot save` | Manual |
| `CodeCartographer: Snapshot Diff` | Prompts for two tags, shows diff in output channel | Manual |
| `CodeCartographer: Open Layers Diagram` | Runs `layers diagram --format mermaid`, opens webview | Manual |

---

## Configuration

All settings under `codecartographer.*` in `settings.json`.

| Setting | Default | Description |
|---------|---------|-------------|
| `codecartographer.binaryPath` | `codecartographer` | Path to the CLI binary |
| `codecartographer.refreshOnSave` | `false` | Re-run file metrics on every save |
| `codecartographer.hotspotThreshold` | `50` | Minimum score to show gutter decoration |
| `codecartographer.commitWindow` | `500` | `--commits` value for hotspot/churn analysis |
| `codecartographer.showEdgeKinds` | `["runtime"]` | Which edge kinds to show in diagram by default |
| `codecartographer.statusBar` | `true` | Show status bar item |
| `codecartographer.explorerBadges` | `true` | Show file-explorer badges |

---

## Data Flow & Caching

Analysis results are cached in memory for the VS Code session. Cache is invalidated on:
- `git checkout` / branch change (detected via `vscode.workspace.onDidSaveTextDocument` on `.git/HEAD`)
- Explicit `CodeCartographer: Refresh Analysis` command
- If `refreshOnSave` is true: on every file save

Cache entries:
```typescript
interface Cache {
  health:   HealthJson    | null;  // codecartographer health --json
  hotspots: HotspotsJson  | null;  // codecartographer hotspots --json
  dead:     DeadJson      | null;  // codecartographer dead --json
  cochange: CochangeJson  | null;  // codecartographer cochange --json
  graph:    GraphJson     | null;  // MCP get_project_graph
}
```

Stale entries (> 10 min) show a ⚠ stale indicator in the sidebar header.

Subprocess execution uses Node's `child_process.spawn`. Stderr is piped to an output channel ("CodeCartographer") for debugging. If the binary isn't found, the extension shows a one-time notification with a link to the install guide.

---

## Out of Scope

- Real-time graph updates on every keystroke (too slow; use refresh)
- Inline diff annotations beyond gutter icons
- Any write operations to the codebase (no replace, no refactor)
- Authentication or cloud sync (that's the `push`/`pull` CLI path)
- Windows support in v1 (paths, `child_process` differences — add after Linux/Mac stable)
