# Context Modes

CodeCartographer offers several ways to capture and deliver codebase context to an AI. Pick the one that matches what the AI needs to do.

## Mode comparison

| Command | Token count | What it includes | Best for |
|---------|-------------|------------------|----------|
| `map` | ~200/module | Imports + public signatures | Daily use, architecture questions, most things |
| `source` | ~5,000/module | Full file content | Debugging, implementation review, line-level questions |
| `copy` | same as source | Full source → clipboard only | One-shot paste with no disk write |
| `context --focus FILE` | budget-limited | PageRank-ranked skeleton around a seed | Targeted deep dives on one area |
| `query QUESTION` | auto-selected | BM25 search → PageRank → skeleton | Specific questions about the codebase |
| `sync` | incremental | Only changed files since last sync | Keeping a snapshot fresh during a session |
| `watch` | incremental | Live updates on save (debounced 500ms) | Ongoing sessions where context stays open |

## `map` — skeleton mode

```bash
codecartographer map [PATH]
```

Extracts imports and public symbol signatures from every file. Produces `codecartographer_map.xml`.

**What's in a skeleton:**
- File path and language
- All import statements
- Public function/method signatures (name, parameters, return type)
- Public type definitions (struct, class, interface, enum)
- Public constants and exported variables
- Doc comments attached to public symbols

**What's not in a skeleton:**
- Function bodies
- Private or unexported symbols
- Comments inside function bodies
- Test implementation details

**Token economics:** A 500-file repo maps to roughly 100k tokens in skeleton mode vs 2.5M in source mode. For most architecture questions and refactoring tasks, skeleton context is sufficient — the AI sees the public contract without the noise.

## `source` — full source mode

```bash
codecartographer source [PATH]
```

Writes complete file content for every scanned file to `codecartographer_source.xml`. Use this when the AI needs to read specific implementation logic, not just signatures.

**When skeleton isn't enough:**
- Debugging a crash where the logic is inside a function body
- Understanding complex conditional branching
- Code review where behavior matters, not just shape
- Writing tests that need to know what the function actually does

## `copy` — clipboard-only snapshot

```bash
codecartographer copy [PATH]
```

Captures full source (same as `source` mode) and puts it directly on the clipboard. Nothing is written to disk. Use this for a quick one-off paste into a chat window.

## `context` — budget-aware focused context

```bash
codecartographer context --focus FILE [--budget TOKENS]
```

Builds a token-budget-aware context bundle centered on a seed file. The algorithm:

1. Start from the seed file(s) specified by `--focus`
2. Walk the import graph outward via PageRank to find the most relevant neighborhood
3. Trim to fit within `--budget` (default 8,000 tokens), dropping lower-ranked files first
4. Return the skeleton for the surviving files

```bash
codecartographer context --focus src/auth.rs --budget 12000
codecartographer context --focus src/api.rs src/models.rs --budget 8000
```

**When to use it:** When you're working in a specific area of the codebase and don't want to dump the entire project skeleton. The PageRank weighting means the files that matter most to your seed file survive the budget cut.

## `query` — question-driven context

```bash
codecartographer query QUESTION
```

Full pipeline in one step: BM25 + regex search → PageRank → skeleton → context health check.

```bash
codecartographer query "how does authentication work?"
codecartographer query "where is the rate limiter implemented?"
codecartographer query "what calls the database migration code?"
```

CodeCartographer searches the codebase for files relevant to your question, ranks them by PageRank-weighted relevance, assembles a skeleton within the default token budget, and outputs context health metadata alongside the bundle.

**When to use it:** When you don't know which files to focus on and want CodeCartographer to figure it out. The output is ready to paste directly into a chat prompt.

## `sync` — incremental update

```bash
codecartographer sync [PATH]
```

Re-processes only files that changed since the last snapshot. Much faster than a full `map` on large repos. The output is a delta patch that clients can merge into the existing context.

Use `sync` during a long editing session to keep the context current without re-sending everything.

## `watch` — live watcher

```bash
codecartographer watch [PATH]
```

Stays running and re-processes changed files automatically when they are saved. Updates are debounced at 500ms to avoid thrashing. The skeleton map is rewritten on each change cycle.

**MCP equivalent:** The `watch_graph` MCP tool provides change-event streaming over the MCP protocol. The `poll_changes` tool queries files modified since a given epoch-millisecond timestamp.

## Output targets

All modes accept `--target` to control output format:

```bash
codecartographer map --target claude    # formatted XML with token budget metadata (default)
codecartographer map --target cursor    # Cursor-optimized format
codecartographer map --target raw       # plain output, no wrappers
```

Set the default globally with `codecartographer config --default-target TARGET`.

## Token efficiency

`map` mode achieves roughly 90% token reduction compared to `source`:

- Full source: ~5,000 tokens per module
- Skeleton: ~200 tokens per module

`context_health` — available as a CLI command and MCP tool — scores any context bundle on six metrics: signal density, compression density, position health, entity density, utilization headroom, and dedup ratio. Composite grade A–F.

```bash
codecartographer context-health codecartographer_map.xml
```

See [Architecture Analysis](architecture-analysis.md) for the full context-health metric breakdown.
