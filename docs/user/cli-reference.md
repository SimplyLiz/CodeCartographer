# CLI Reference

Complete reference for all `codecartographer` commands and flags.

## Global flags

These flags apply to most commands:

| Flag | Description |
|------|-------------|
| `--target TARGET` | Output format: `claude` (default), `cursor`, or `raw` |
| `--copy` | Also copy output to clipboard |
| `--ignore FILE` | Additional ignore file(s) to load |
| `--no-ignore` | Bypass `.codecartographerignore` and built-in noise filter |
| `PATH` | Directory to scan (defaults to current directory) |

## codecartographer (no subcommand)

```bash
codecartographer [PATH] [--target TARGET] [--copy]
```

Interactive menu. Shows token estimates for each mode and prompts you to pick one.

---

## Context capture

### map

```bash
codecartographer map [PATH]
```

Skeleton map — imports and public signatures only. Writes `codecartographer_map.xml`.

Approximately 200 tokens per module vs 5,000 for full source. Use this for most tasks.

### source

```bash
codecartographer source [PATH]
```

Full source code. Writes `codecartographer_source.xml`. Use when you need function bodies.

### copy

```bash
codecartographer copy [PATH]
```

Full source to clipboard only. No disk write. Use for a quick one-off paste.

### context

```bash
codecartographer context --focus FILE [--budget TOKENS]
```

PageRank-ranked skeleton around a seed file, pruned to a token budget.

| Flag | Description |
|------|-------------|
| `--focus FILE` | Seed file(s) to center the context on |
| `--budget TOKENS` | Token budget (default: 8000) |

### query

```bash
codecartographer query QUESTION
```

BM25 search → PageRank → skeleton in one step. Takes a natural-language question and produces ready-to-use context.

### sync

```bash
codecartographer sync [PATH]
```

Incremental update — re-processes only files changed since the last snapshot.

### watch

```bash
codecartographer watch [PATH]
```

Live watcher. Stays running and re-processes changed files on save (debounced 500ms).

---

## Project setup

### init

```bash
codecartographer init [PATH]
```

Initialize a project. Creates `.codecartographer/config.toml`.

### init-ckb

```bash
codecartographer init-ckb [PATH] [--ckb-url URL] [--webhook-url URL]
```

Initialize with CKB integration. Writes CKB connection settings to `.codecartographer/config.toml`.

| Flag | Description |
|------|-------------|
| `--ckb-url URL` | CKB server URL |
| `--webhook-url URL` | Webhook endpoint for change notifications |

---

## Architecture analysis

### health

```bash
codecartographer health [PATH] [--compare REF] [--json]
```

Health score 0–100 plus breakdown: cycles, bridges, god modules, layer violations.

| Flag | Description |
|------|-------------|
| `--compare REF` | Compare to a git ref (e.g., `main`, `HEAD~1`) |
| `--json` | Machine-readable JSON output |

### simulate

```bash
codecartographer simulate [PATH] [OPTIONS]
```

Predict architectural impact of a change.

| Flag | Description |
|------|-------------|
| `--module MODULE` | Target file (path suffix or full path) |
| `--new-signature SIG` | Signature to simulate adding |
| `--remove-signature SIG` | Signature to simulate removing |
| `--staged` | Analyze all staged git changes |
| `--diff REF` | Analyze changes relative to a git ref |
| `--fail-on-cycle` | Exit 1 if simulation would introduce a cycle |
| `--json` | Machine-readable JSON output |

### check

```bash
codecartographer check [PATH]
```

CI gate. Exits non-zero if any cycle or layer violation exists.

### dead

```bash
codecartographer dead [PATH] [--json]
```

Dead code candidates — files and public symbols with no callers.

### symbols

```bash
codecartographer symbols --unreferenced [PATH]
```

Public symbols with no callers (symbol-level, narrower than `dead`).

### deps

```bash
codecartographer deps TARGET [--format json]
```

Dependency tree of a single module as JSON.

### path

```bash
codecartographer path --from A --to B [PATH]
```

Shortest import path between two modules.

| Flag | Description |
|------|-------------|
| `--from FILE` | Starting file (repo-relative path or module id) |
| `--to FILE` | Destination file (repo-relative path or module id) |
| `--json` | Machine-readable JSON output |

### evolution

```bash
codecartographer evolution [PATH] [--days DAYS]
```

Architectural health trend over the last N days (default 30). "Current Status"
always reflects the live health score from the current scan, matching what
`codecartographer health` reports for the same project state.

### layers

```bash
codecartographer layers [PATH] SUBCOMMAND
```

Manage architectural layer definitions.

| Subcommand | Description |
|-----------|-------------|
| `init [-o FILE]` | Auto-propose a `layers.toml` from the current import graph |
| `validate [--config FILE] [--json]` | Check violations against `layers.toml` |
| `diagram [--config FILE] [--format mermaid\|dot]` | Show the collapsed layer graph |
| `suggest [--config FILE]` | Suggest improvements to an existing `layers.toml` |

### snapshot

```bash
codecartographer snapshot SUBCOMMAND
```

Save or compare architecture snapshots.

| Subcommand | Description |
|-----------|-------------|
| `save TAG [PATH]` | Save current architecture with an identifying tag |
| `diff TAG1 TAG2 [--json]` | Compare two saved snapshots |
| `list [PATH]` | List all saved snapshots |

---

## Git intelligence

### hotspots

```bash
codecartographer hotspots [PATH] [OPTIONS]
```

High-churn × high-complexity files.

| Flag | Default | Description |
|------|---------|-------------|
| `--commits N` | 500 | Commits to analyze |
| `--top N` | 15 | Results to show |
| `--untested` | — | Only hotspots without a sibling test file |
| `--by-author` | — | Show dominant git author |
| `--bus-factor` | — | Show unique author count |
| `--json` | — | Machine-readable output |

### cochange

```bash
codecartographer cochange [PATH] [OPTIONS]
```

Files that frequently change together.

| Flag | Default | Description |
|------|---------|-------------|
| `--commits N` | 500 | Commits to analyze |
| `--min-count N` | 5 | Minimum co-change count |
| `--cluster` | — | Community detection on co-change graph |
| `--threshold F` | 0.5 | Coupling-score threshold for cluster edges |
| `--json` | — | Machine-readable output |

### semidiff

```bash
codecartographer semidiff COMMIT1 [COMMIT2]
```

Function-level semantic diff: which public signatures were added, removed, or changed.

### shotgun

```bash
codecartographer shotgun [PATH] [OPTIONS]
```

Shotgun surgery candidates — files whose changes scatter across many unrelated modules.

| Flag | Default | Description |
|------|---------|-------------|
| `--commits N` | 500 | Commits to analyze |
| `--top N` | 20 | Results to show |
| `--min-partners N` | 3 | Minimum distinct co-change partners |

### todo

```bash
codecartographer todo [PATH] [--top N] [--json]
```

TODO/FIXME/HACK density across source files. Default: top 20.

---

## Search and files

### search

```bash
codecartographer search PATTERN [OPTIONS]
```

Grep-like content search.

| Flag | Short | Description |
|------|-------|-------------|
| `--ignore-case` | `-i` | Case-insensitive |
| `--invert-match` | `-v` | Lines not matching |
| `--word-regexp` | `-w` | Whole word only |
| `--after-context N` | `-A N` | Lines after match |
| `--before-context N` | `-B N` | Lines before match |
| `--context N` | `-C N` | Lines before and after |
| `--glob PATTERN` | | Restrict to glob |
| `--path DIR` | | Restrict to directory |
| `--literal` | | Literal string (not regex) |

### find

```bash
codecartographer find PATTERN [OPTIONS]
```

Glob file discovery.

| Flag | Description |
|------|-------------|
| `--max-depth N` | Limit traversal depth |
| `--modified-since DURATION` | Files modified within DURATION (e.g., `24h`, `7d`) |
| `--min-size SIZE` | Minimum file size (e.g., `10kb`) |
| `--max-size SIZE` | Maximum file size |

### replace

```bash
codecartographer replace PATTERN REPLACEMENT [OPTIONS]
```

Regex find-and-replace. Supports `$0` (full match), `$1`, `$2` (groups).

| Flag | Description |
|------|-------------|
| `--dry-run` | Preview only, no writes |
| `--literal` | Literal string pattern |
| `--glob PATTERN` | Restrict to glob |
| `--exclude PATTERN` | Exclude files matching a glob |
| `--path DIR` | Restrict to a subdirectory |
| `--max-per-file N` | Max replacements per file |
| `--context-lines N` | Lines of context in dry-run output |
| `--backup` | Write `.bak` files before replacing |

### extract

```bash
codecartographer extract PATTERN [OPTIONS]
```

Capture-group extraction.

| Flag | Description |
|------|-------------|
| `--group N` | Capture group index to extract (repeatable: `--group 1 --group 2`), short: `-g` |
| `--count` | Frequency table mode |
| `--dedup` | Remove duplicates |
| `--sort` | Sort alphabetically |
| `--glob PATTERN` | Restrict to glob |
| `--path DIR` | Restrict to a subdirectory |
| `--limit N` | Maximum results |
| `--format text\|json\|csv\|tsv` | Output format |

---

## Diagram

```bash
codecartographer diagram [PATH] [OPTIONS]
```

Full reference in [Diagrams](diagrams.md). Quick summary:

| Flag | Description |
|------|-------------|
| `--format mermaid\|dot\|ascii` | Output format for import graph (default: mermaid) |
| `--format quadrant` | Churn × complexity scatter — top-right = refactor now |
| `--format sequence\|seq` | Sequence diagram — requires `--call-graph FILE` (Rust/Python/Go/C/C++) |
| `--format class\|uml` | Class diagram — requires `--call-graph FILE` (Rust/Python/Go/C++/TS) |
| `--format er\|entity\|erd` | ER diagram — requires `--call-graph FILE` (Rust/Python/Go/C++/TS) |
| `-o FILE` | Write to file; extension determines rendering |
| `--max-nodes N` | Node cap (default: 60) |
| `--focus MODULE` | Neighborhood view around a module |
| `--depth N` | BFS depth for `--focus` (default: 2) |
| `--blast-radius MODULE` | Epicenter + direct deps + direct dependents |
| `--call-graph FILE` | File-level analysis: call graph, sequence, class, or ER diagram |
| `--cochange-threshold F` | Overlay co-change edges above this coupling score |
| `--docs-only` | Show only doc files and their code references |
| `--group-by-folder DEPTH` | Collapse to folder granularity |
| `--color-by-owner` | Node fill by dominant git author |

---

## Context quality

### context-health

```bash
codecartographer context-health [FILE] [--model MODEL]
```

Score a context bundle on six metrics. Grade A–F.

### llmstxt

```bash
codecartographer llmstxt [PATH]
```

Generate `llms.txt` — a structured project index following the LLMs.txt standard.

### claudemd

```bash
codecartographer claudemd [PATH]
```

Generate `CLAUDE.md` — an architecture guide formatted for Claude Code.

---

## Semantic traversal (experimental)

### reach

```bash
codecartographer reach SYMBOL [SYMBOL ...] [OPTIONS] [PATH]
```

Semantic graph traversal from a named symbol. Returns a compact AI-native context tree: callers with one-line snippets, callees with signatures, depth-2 type definitions. 135–500 tokens per symbol.

Pass two or more symbols for an **intersection view**: callers are merged and deduped, callees shared across both roots are annotated `[shared]`, and depth-2 types appearing in both trees are promoted to a "shared types" section.

| Flag | Description |
|------|-------------|
| `--file FILE` | Disambiguate when the symbol name appears in multiple files (single-symbol only) |
| `--depth N` | Traversal depth (default: 2) |
| `--budget TOKENS` | Token cap; trims leaf nodes first (default: 6000) |
| `--include-tests` | Expand test call sites (default: collapsed and counted) |
| `--show-body` | Include the function body of the root symbol, up to 40 lines |
| `--format compact\|json` | Output format (default: compact; single-symbol only for json) |

```bash
codecartographer reach verify_token
codecartographer reach "Auth::verify_token" --file src/auth.rs
codecartographer reach FileCallGraph --depth 1
codecartographer reach build_reach --show-body
codecartographer reach verify_token decode_jwt
codecartographer reach build_reach render_reach --depth 1
```

### answer

```bash
codecartographer answer QUESTION [OPTIONS] [PATH]
```

Question-driven evidence chain. Takes a natural-language question and returns a numbered list of the minimum semantic units that together answer it, in reading order with inter-item connections annotated. When companion implementations score within 10% of each other, the one from the older file ranks first.

| Flag | Description |
|------|-------------|
| `--max-items N` | Maximum evidence items (default: 6) |
| `--budget TOKENS` | Token cap (default: 8000) |
| `--no-body` | Skip the function body for the top-scored item |
| `--then N` | After the chain, drill into item #N via `reach` and append its context tree |

```bash
codecartographer answer "how does rate limiting work?"
codecartographer answer "what is FileCallGraph and how is it built"
codecartographer answer "how does token budget trimming work" --max-items 4
codecartographer answer "how does the call graph work?" --then 2
```

---

## Server and config

### serve

```bash
codecartographer serve [PATH]
```

Start the MCP server on stdio (JSON-RPC 2.0). See [MCP Tools](mcp-tools.md).

### config

```bash
codecartographer config [--default-target TARGET] [--show]
```

Manage global configuration.

| Flag | Description |
|------|-------------|
| `--default-target TARGET` | Set default output target (`claude`, `cursor`, `raw`) |
| `--show` | Print current global config |

### status

```bash
codecartographer status [PATH]
```

Project dashboard: file counts, last-sync time, health score summary.

### languages

```bash
codecartographer languages [PATH]
```

Detected programming languages and file counts.

### update

```bash
codecartographer update
```

Re-runs the install script to build and install the latest version from source. Requires the repo to be present at the expected location relative to the binary.
