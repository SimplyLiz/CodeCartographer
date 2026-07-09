# CodeCartographer — User Documentation

CodeCartographer packages your repository into a structured snapshot an AI can reason about. It sits between your codebase and your AI assistant — Claude, Cursor, GPT-4, or any model with a context window.

## Guides

| Document | What it covers |
|----------|---------------|
| [Getting Started](getting-started.md) | Install, initialize a project, and run your first snapshot |
| [Context Modes](context-modes.md) | map, source, context, query, sync, watch — when to use each |
| [Architecture Analysis](architecture-analysis.md) | Health scoring, simulate-change, dead code, layer enforcement, evolution |
| [Git Intelligence](git-intelligence.md) | Hotspots, co-change, hidden coupling, semidiff, shotgun surgery |
| [Search](search.md) | search, find, replace, extract — grep/sed/awk for your codebase |
| [Semantic Traversal](cli-reference.md#semantic-traversal-experimental) | `reach` and `answer` — symbol-scoped and question-driven context at 1–3% of skeleton token cost |
| [Diagrams](diagrams.md) | Dependency graphs, call graphs, blast radius, HTML explorer |
| [MCP Tools](mcp-tools.md) | MCP server setup and full tool reference |
| [Configuration](configuration.md) | Global config, per-repo config, .codecartographerignore, layers.toml |
| [GitHub Action](github-action.md) | CI health gates and PR health-delta comments |
| [Ecosystem](ecosystem.md) | How CodeCartographer fits with CKB, TruthKeeper, LLMRouter, and CCE |
| [Integration](integration.md) | ShellAI integration |

## How it works

1. Run `codecartographer` in any repo.
2. Pick a mode — `map` (skeletons, ~90% fewer tokens) or `source` (full content).
3. The snapshot is written to disk and optionally copied to clipboard.
4. Paste it into your AI chat, or let the MCP server inject it automatically.

```
  Project : my-app  (42 source files)

  map     ~18k tokens   signatures & structure only   (recommended)
  source  ~310k tokens  full file content
  diagram               visualise dependency graph
  query                 answer a specific question about the code

What would you like to do? [map/source/diagram/query/quit]:
```

## Quick reference

```bash
codecartographer map        # skeleton — imports + public signatures (~200 tokens/module)
codecartographer source     # full source — everything
codecartographer query "how does authentication work?"
codecartographer health     # architectural health score 0–100
codecartographer serve      # start MCP server for Claude Code / Cursor
```
