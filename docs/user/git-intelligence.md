# Git Intelligence

CodeCartographer analyzes git history to surface patterns that aren't visible from the code alone: which files are constantly changing, which files always change together, and which changes scatter unpredictably across the codebase.

All git commands require git on your `$PATH` and a git repository with history.

## Hotspots

```bash
codecartographer hotspots [PATH] [OPTIONS]
```

Files that are both high-churn (changed frequently) and high-complexity (many imports or large surface area). These are the highest-risk files in a codebase — they change often and touch many things.

```bash
codecartographer hotspots                       # top 15 hotspots, last 500 commits
codecartographer hotspots --top 30              # show more results
codecartographer hotspots --commits 1000        # look further back
codecartographer hotspots --untested            # only hotspots with no sibling test file
codecartographer hotspots --by-author           # add dominant owner column
codecartographer hotspots --bus-factor          # add unique author count (lower = riskier)
codecartographer hotspots --json                # machine-readable output
```

**Flags:**
- `--commits N` — number of git commits to include (default 500)
- `--top N` — number of results to show (default 15)
- `--untested` — filter to only hotspots that have no corresponding test file
- `--by-author` — show the dominant git author per file
- `--bus-factor` — show the count of unique authors (1 = only one person ever touched this)
- `--json` — emit JSON

**What to do with hotspots:**
- Files with `--untested` are high-risk by definition — add tests before the next change
- Files with a bus factor of 1 are team knowledge silos — identify and document them
- Hotspots with high in-degree in the dependency graph are candidates for splitting

## Per-file churn

```bash
codecartographer hotspots --commits 500 --json | jq '.[].churn_count'
```

Raw churn (commit count per file) is included in the JSON output. The MCP tool `git_churn` exposes this directly.

## Co-change analysis

```bash
codecartographer cochange [PATH] [OPTIONS]
```

Files that are frequently committed together, regardless of whether they have an import relationship. This is temporal coupling — the codebase is telling you these files are coupled even if the import graph doesn't show it.

```bash
codecartographer cochange                        # top pairs, min 5 co-changes
codecartographer cochange --min-count 3          # lower threshold to see more pairs
codecartographer cochange --commits 1000         # look further back
codecartographer cochange --cluster              # cluster into implicit modules
codecartographer cochange --threshold 0.7        # coupling-score threshold for clusters
codecartographer cochange --json                 # machine-readable output
```

**Flags:**
- `--commits N` — number of git commits to analyze (default 500)
- `--min-count N` — minimum co-change count to include a pair (default 5)
- `--cluster` — run community detection on the co-change graph to identify implicit module boundaries; useful for finding hidden abstractions
- `--threshold F` — coupling-score threshold for community edges in cluster mode (default 0.5; range 0.0–1.0)
- `--json` — emit JSON

**Coupling score:** A Jaccard-style similarity between two files' commit sets. A score of 1.0 means they have always changed together. A score of 0.5 means they share half their commits.

**Hidden coupling:** The MCP tool `hidden_coupling` identifies co-change pairs that have no import edge — the coupling is invisible in the static graph. These are especially worth investigating.

**What to do with co-change:**
- High-score pairs with no import edge → consider adding an explicit dependency or extracting a shared module
- Clusters from `--cluster` → consider whether the cluster should be a proper package boundary
- Pairs that surprise you → investigate whether they share a conceptual responsibility that isn't reflected in the code structure

## Semantic diff

```bash
codecartographer semidiff COMMIT1 [COMMIT2]
```

Function-level diff between two commits. Instead of showing line changes, shows which public signatures were added, removed, or modified.

```bash
codecartographer semidiff HEAD~1             # compare current HEAD to one commit back
codecartographer semidiff HEAD~5 HEAD~1      # compare any two refs
codecartographer semidiff main               # compare working tree to main branch
```

**Output:**
```
src/auth.rs
  + pub fn refresh_token(id: UserId) -> Token
  ~ pub fn verify_token(token: &str, opts: VerifyOpts) -> Result<Claims>
      (was: pub fn verify_token(token: &str) -> Result<Claims>)
  - pub fn legacy_login(user: &str, pass: &str) -> bool
```

`+` = added, `~` = changed signature, `-` = removed.

**When to use it:** Code review — understanding what a branch actually changes at the public API level, without reading every line diff. Also useful for writing changelogs.

## Shotgun surgery candidates

```bash
codecartographer shotgun [PATH] [OPTIONS]
```

Files whose changes scatter across many unrelated modules — every time you touch file X, you also need to change files A, B, C, D, and E, which have nothing to do with each other. This is the "shotgun surgery" code smell: a single logical change requires edits in many places.

```bash
codecartographer shotgun                     # top 20, min 3 partners
codecartographer shotgun --top 30
codecartographer shotgun --commits 1000
codecartographer shotgun --min-partners 5    # only show files with high scatter
```

**Flags:**
- `--commits N` — number of commits to analyze (default 500)
- `--top N` — number of results to show (default 20)
- `--min-partners N` — minimum number of distinct co-change partners (default 3)

**Scatter score:** Measured as Shannon entropy over the co-change distribution. High entropy = changes scatter widely. Low entropy = changes concentrate in a few related files.

**What to do:** Shotgun surgery candidates often indicate a missing abstraction. The file with high scatter may be doing something conceptually central that should be encapsulated in its own module.

## TODO density

```bash
codecartographer todo [PATH] [--top N] [--json]
```

Scans all source files for `TODO`, `FIXME`, `HACK`, and similar markers. Reports per-file density (markers per thousand lines) and absolute counts.

```bash
codecartographer todo                 # top 20 files by TODO density
codecartographer todo --top 50
codecartographer todo --json
```

Use this to find accumulated technical debt that hasn't been tracked in an issue tracker.

## Polling and watching for changes

```bash
codecartographer watch [PATH]
```

Stays running and updates the skeleton map when files change. Updates are debounced at 500ms.

For programmatic use, the MCP tools `poll_changes` and `watch_graph` expose change events over the MCP protocol without shelling out.
