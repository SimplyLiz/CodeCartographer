# Architecture Analysis

CodeCartographer builds a dependency graph from your codebase and runs several analyses on top of it. This page covers the architectural analysis commands.

## Health score

```bash
codecartographer health [PATH] [--compare REF] [--json]
```

Produces an overall health score 0–100 and a breakdown of structural issues.

```
Health score: 74 / 100

  cycles           3    (degree of coupling in each)
  bridges          1    (single-path critical dependency)
  god modules      2    (modules with in-degree > threshold)
  layer violations 4    (back-calls and skip-calls per layers.toml)
```

**Flags:**
- `--compare REF` — compare the current score against a past git ref (e.g., `main`, `HEAD~1`). Shows a delta so you can tell if a branch improves or degrades architecture quality.
- `--json` — emit machine-readable JSON for CI pipelines.

**Score interpretation:**
- **90–100** — healthy, no significant issues
- **75–89** — minor issues; worth tracking but not blocking
- **50–74** — meaningful structural problems; plan to address
- **< 50** — significant debt; cycles or god modules likely impacting team velocity

## Simulate change

```bash
codecartographer simulate --module FILE [OPTIONS] [--json]
```

Predicts the architectural impact of a change before you make it.

```bash
# What happens if I add a new public function to auth.rs?
codecartographer simulate --module src/auth.rs --new-signature "pub fn refresh_token(id: UserId) -> Token"

# What if I remove this signature?
codecartographer simulate --module src/auth.rs --remove-signature "pub fn legacy_login"

# Analyse all staged changes (what would happen if I committed these?)
codecartographer simulate --staged

# Analyse changes relative to a branch
codecartographer simulate --diff main
```

**Output includes:**
- List of directly affected modules (import the changed module)
- Transitively affected modules (up to 2 hops)
- Whether the change would create or break any cycles
- Layer violation risk
- Predicted health score delta

**Flags:**
- `--module MODULE` — target file (path suffix or full path)
- `--new-signature SIGNATURE` — a signature string to add
- `--remove-signature SIGNATURE` — a signature string to remove
- `--staged` — analyse everything in the git staging area
- `--diff REF` — analyse changes relative to a git ref
- `--fail-on-cycle` — exit with status 1 if the simulation would introduce a cycle (useful in pre-commit hooks)
- `--json` — machine-readable output

## CI gate

```bash
codecartographer check
```

Exits non-zero if any hard constraint is violated. Designed for CI pipelines and pre-commit hooks.

Default failure conditions:
- Any import cycle exists
- Any layer violation defined in `layers.toml` exists

The GitHub Action wraps this with more fine-grained gates — see [GitHub Action](github-action.md).

## Dead code

```bash
codecartographer dead [PATH] [--json]
```

Lists files and public symbols that are not imported anywhere in the project (in-degree = 0 in the dependency graph). These are dead-code candidates.

**Caveats:** CodeCartographer's analysis is heuristic — it does not track runtime dynamism (reflection, dynamic imports, `require()` with variables). A symbol flagged here may still be used at runtime. Use the output as a list of candidates to verify manually, not as a deletion checklist.

## Unreferenced symbols

```bash
codecartographer symbols --unreferenced
```

Lists public exported symbols — functions, types, constants — that have no callers within the scanned project. Narrower than `dead` (which operates at the file level); this operates at the symbol level.

## Dependency tree

```bash
codecartographer deps TARGET [--format json]
```

Shows the dependency tree of a single module as JSON.

```bash
codecartographer deps src/api.rs
codecartographer deps src/auth.rs --format json | jq '.dependencies[]'
```

## Import path

```bash
codecartographer path A B
```

Finds the shortest import path between two modules — how does module A transitively depend on module B?

```bash
codecartographer path src/main.rs src/db/migrations.rs
```

Useful for understanding why a seemingly unrelated module is in the blast radius of a change.

## Architecture evolution

```bash
codecartographer evolution [PATH] [--days DAYS]
```

Shows architectural health trends over time. Looks back at the last N days (default 30) of git history and plots how the health score, cycle count, and god-module count have changed.

```bash
codecartographer evolution --days 90
```

**Output includes:**
- Health score trend (with dates)
- Debt indicators: which metrics are getting worse
- Actionable recommendations ranked by impact

## Layer enforcement

Define allowed import flows in `layers.toml` and CodeCartographer will detect violations automatically.

```toml
[layers]
ui = ["components", "pages"]
services = ["api", "auth"]
db = ["models"]

[allowed_flows]
ui -> services
services -> db
```

```bash
codecartographer layers      # check current violations
codecartographer check       # fail CI if violations exist
```

**Violation types:**
- **BackCall** — a lower layer imports from a higher one (e.g., `db` → `ui`)
- **SkipCall** — a layer bypasses an intermediate layer (e.g., `ui` → `db`)
- **CircularCrossLayer** — a cycle spans multiple layers
- **DirectForeignImport** — a module imports directly from a layer it is not allowed to access

See [Configuration](configuration.md) for `layers.toml` placement options.

## Architecture snapshots

```bash
codecartographer snapshot          # save current architecture snapshot
codecartographer snapshot --diff   # compare current state to last saved snapshot
```

Snapshots record the dependency graph, health score, cycle list, and layer violations at a point in time. Use them to track whether a branch improves or degrades the architecture relative to the baseline.

## Context health

```bash
codecartographer context-health [FILE]
```

Scores a context bundle (e.g., `codecartographer_map.xml`) on six metrics:

| Metric | What it measures |
|--------|-----------------|
| Signal density | Ratio of technical tokens (identifiers, paths, types) to total tokens |
| Compression density | How much information per token — penalizes padding and repetition |
| Position health | Whether important context is placed early (models attend more to early tokens) |
| Entity density | Unique code entities per token — prevents dilution by prose |
| Utilization headroom | How much of the target model's context window is consumed |
| Dedup ratio | Fraction of duplicate content — repeated imports, copy-pasted signatures |

Composite grade A–F. A score below B is worth improving — usually by switching to `map` mode or trimming with `--focus` and `--budget`.

```bash
codecartographer context-health codecartographer_map.xml --model claude
codecartographer context-health codecartographer_map.xml --model gpt4
```

The `--model` flag adjusts the utilization headroom calculation to use the correct context window size for your target model.

## Generate AI-friendly project files

```bash
codecartographer llmstxt    # generate llms.txt project index
codecartographer claudemd   # generate CLAUDE.md architecture guide
```

`llmstxt` produces a `llms.txt` file following the LLMs.txt standard — a structured index of your project an AI can use as a root context. `claudemd` produces a `CLAUDE.md` tailored to Claude Code's conventions.

## Languages detected

```bash
codecartographer languages
```

Lists the programming languages CodeCartographer detected in the project and the number of files per language. Useful for verifying the scanner is finding what you expect.

## Project status

```bash
codecartographer status [PATH]
```

Shows a dashboard: file counts, last-sync time, current health score summary, and whether any state is stale.
