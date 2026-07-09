# Getting Started

## Prerequisites

- **Rust toolchain** — install via [rustup.rs](https://rustup.rs)
- **git** on `$PATH` — required for all git-analysis commands (`hotspots`, `cochange`, `semidiff`, etc.)
- **Optional:** `mmdc` (`npm install -g @mermaid-js/mermaid-cli`) — for exporting diagrams to SVG/PNG via Mermaid
- **Optional:** `dot` (`brew install graphviz`) — for SVG/PNG via Graphviz

## Install

### From source (Linux / macOS)

```bash
git clone <repo>
cd mapper-core/CodeCartographer
cargo build --release
cp target/release/codecartographer ~/.local/bin/codecartographer
```

The `install.sh` script at the repo root automates the build and adds `~/.local/bin` to your `$PATH`.

### Windows

Run `install.ps1` in PowerShell. Run `verify_install.ps1` to confirm the binary is on your `$PATH`.

### Interactive launcher

If you don't have a Rust toolchain installed and just want to try it:

```bash
python3 launch.py
```

The launcher checks for `cargo`, builds the binary, installs it, and drops you into the interactive menu.

## Initialize a project

Run this once per repo:

```bash
cd my-repo
codecartographer init
```

This creates a `.codecartographer/` directory with a per-repo `config.toml`. You can commit this directory.

For CKB integration:

```bash
codecartographer init-ckb --ckb-url http://localhost:3001
```

## Your first run

Run `codecartographer` with no arguments from inside any directory to get the interactive menu:

```
  Project : my-app  (42 source files)
  Ignored : 1,204 noise files (node_modules, build artifacts, lock files)

  map     ~18k tokens   signatures & structure only   (recommended)
  source  ~310k tokens  full file content
  diagram               visualise dependency graph
  query                 answer a specific question about the code

What would you like to do? [map/source/diagram/query/quit]:
```

The menu shows token estimates so you can pick the right mode before committing. Choose `map` for almost everything — it's ~90% smaller than `source` and covers the vast majority of AI-assisted tasks.

## Running in non-interactive mode

You can skip the menu by calling subcommands directly:

```bash
codecartographer map        # writes codecartographer_map.xml to disk
codecartographer source     # writes codecartographer_source.xml to disk
codecartographer copy       # copies full source to clipboard, no disk write
```

## Set a global default target

CodeCartographer supports three output formats, one of which is the default for your AI client:

```bash
codecartographer config --default-target claude    # Claude Code (default)
codecartographer config --default-target cursor    # Cursor
codecartographer config --default-target raw       # plain output, no wrappers

codecartographer config --show   # check current global config
```

The `--target` flag on any command overrides the global default for that one invocation.

## Verify the installation

```bash
codecartographer --version
codecartographer status      # shows project status, file counts, last-sync time
codecartographer health      # runs a full architectural health check
```

## Connecting to Claude Code (MCP)

Start the MCP server:

```bash
codecartographer serve
```

Register it in your Claude Code settings (`.claude/settings.json` or the global settings):

```json
{
  "mcpServers": {
    "codecartographer": {
      "command": "codecartographer",
      "args": ["serve"]
    }
  }
}
```

Once registered, the MCP server exposes 30+ tools directly to Claude Code — skeleton maps, blast radius, search, git intelligence, and more. See [MCP Tools](mcp-tools.md) for the full reference.

## What gets ignored

CodeCartographer automatically excludes noisy paths from all scanning, search, and find operations:

- Package managers: `node_modules/`, `vendor/`, `.venv/`, `venv/`
- Build output: `target/`, `dist/`, `build/`, `out/`, `.next/`, `__pycache__/`
- Version control: `.git/`
- Lock files: `package-lock.json`, `Cargo.lock`, `yarn.lock`, `go.sum`
- Minified files: `*.min.js`, `*.min.css`
- Source maps, log files, binary files, large SVGs

Add project-specific patterns to `.codecartographerignore` (same syntax as `.gitignore`). Pass `--no-ignore` to bypass the filter for a single invocation.

## Next steps

- [Context Modes](context-modes.md) — understand when to use `map` vs `source` vs `query`
- [Architecture Analysis](architecture-analysis.md) — health scores, cycle detection, layer enforcement
- [Git Intelligence](git-intelligence.md) — find hotspots and co-change patterns
- [MCP Tools](mcp-tools.md) — use all 30+ tools from inside Claude Code or Cursor
