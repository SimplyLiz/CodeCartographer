# Adding Nyx.Navigator to GitHub CI

The Nyx.Navigator GitHub Action runs on every pull request and posts a health-delta comment — architecture score before and after, hotspots the PR touched, and optional gate checks that fail the build on cycles or regressions.

---

## What you get

Every PR gets a comment like this:

```
🟡 Nyx.Navigator Health — 72.4/100

| Metric           | Base | Head | Delta  |
|------------------|-----:|-----:|-------:|
| Health Score     | 80.2 | 72.4 | -7.8 ⬇ |
| Bridges          |   11 |   14 |   +3 ⬆ |
| Cycles           |    2 |    3 |   +1 ⬆ |
| God Modules      |    1 |    1 |     0  |
| Layer Violations |    0 |    0 |     0  |

> ⚠️ 1 dependency cycle detected. Run `navigator health` locally to see details.

Hotspots touched by this PR:

| File              | Score | Severity | Owner | Authors |
|-------------------|------:|----------|-------|--------:|
| src/api.rs        |  87.3 | CRITICAL | Lisa  |       2 |
| src/mapper.rs     |  65.1 | HIGH     | Chris |       4 |
```

The comment is updated in-place on each push — no spam. It also appears in the workflow's **Summary** tab, so it's visible without opening the PR.

---

## Setup

### 1. Create the workflow file

Add `.github/workflows/navigator.yml` to your repository:

```yaml
name: Architecture Health

on:
  pull_request:
    types: [opened, synchronize, reopened]

permissions:
  contents: read
  pull-requests: write

jobs:
  navigator:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
        with:
          fetch-depth: 0          # required — see note below

      - uses: anthropics/navigator@v3
        with:
          github-token: ${{ secrets.GITHUB_TOKEN }}
```

That's it. `GITHUB_TOKEN` is provided automatically by GitHub — no secrets to configure.

### 2. Merge the workflow

Push the file to any branch, open a PR, and the check will run. The first run has no base to compare against, so the comment shows current metrics only; subsequent PRs show the full delta.

---

## The `fetch-depth: 0` requirement

By default, `actions/checkout` fetches only the last commit (a shallow clone). Nyx.Navigator's health comparison reads the base branch's source files using `git show <base-sha>:<file>`, which requires that commit to be present in local history.

Without full history, the action skips the delta table and posts a warning. Add `fetch-depth: 0` to your checkout step to get the full diff.

If your repo is large and full-history checkout is too slow, fetch the merge base explicitly instead:

```yaml
- uses: actions/checkout@v4
  with:
    fetch-depth: 1

- name: Fetch base branch history
  run: git fetch origin ${{ github.base_ref }} --depth=50
```

50 commits is usually enough to find the merge base for any normal PR.

---

## Configuration

All inputs are optional beyond `github-token`.

```yaml
- uses: anthropics/navigator@v3
  with:
    github-token: ${{ secrets.GITHUB_TOKEN }}

    # Pin to a specific release instead of "latest".
    navigator-version: v3.1.0

    # Subdirectory to analyse (monorepos with a single-language subfolder).
    working-directory: backend/

    # Number of commits to include in churn / hotspot analysis.
    commits: 500

    # Gate: fail the check if any dependency cycle exists.
    fail-on-cycle: false

    # Gate: fail if any layers.toml violation is found.
    fail-on-layer-violation: false

    # Gate: fail if health score drops more than N points vs the base branch.
    fail-on-regression: false
    regression-threshold: 5

    # Set to false to skip the PR comment (report still appears in Summary).
    post-comment: true
```

---

## Progressive gate adoption

Start permissive, tighten as the codebase improves. A typical rollout:

**Week 1 — visibility only** (default config)
The comment appears. No failures. The team sees the numbers and learns what they mean.

**Week 2 — block new cycles**
```yaml
fail-on-cycle: true
```
PRs that introduce a *new* cycle fail. PRs that don't change cycle count pass even if cycles already exist.

> Note: the gate checks the absolute cycle count on HEAD, not the delta. If the base branch already has 2 cycles and this PR adds one more, the gate fires. Use `fail-on-regression` + `regression-threshold` if you want delta-only gating.

**Week 4 — block large regressions**
```yaml
fail-on-regression: true
regression-threshold: 10
```
A single PR is unlikely to move the score 10 points. This catches architectural mistakes (a new god module, a circular subsystem) without failing routine feature work.

**Ongoing — layer enforcement**
Once `layers.toml` is established and the team has fixed existing violations:
```yaml
fail-on-layer-violation: true
```

---

## Snapshot diffing

The action saves a snapshot of the current architecture state after each run and uploads it as a workflow artifact (retained 90 days). To compare two snapshots locally:

```bash
# Download the artifact from the GitHub UI or via gh:
gh run download <run-id> --name navigator-snapshot-<sha>

# Diff two snapshots:
navigator snapshot diff v2.0.0 v3.0.0
```

To pin a named snapshot before a major refactor:

```yaml
- uses: anthropics/navigator@v3
  with:
    github-token: ${{ secrets.GITHUB_TOKEN }}

- name: Save named snapshot
  run: navigator snapshot save pre-auth-rewrite
```

Then after the refactor merges:

```bash
navigator snapshot diff pre-auth-rewrite post-auth-rewrite
```

---

## Troubleshooting

**The action can't find the Nyx.Navigator binary**
The binary is downloaded from the GitHub Release for the tag specified in `navigator-version`. If the release doesn't include a binary for `ubuntu-latest` (`x86_64-unknown-linux-gnu`), the install step will fail. Check that the release tag exists and includes a `navigator-binary-navigator-x86_64-unknown-linux-gnu.tar.gz` asset.

**No delta table in the comment**
The base SHA wasn't in local history. Add `fetch-depth: 0` to your checkout step or fetch the base branch explicitly (see above).

**PR comment isn't being posted**
The `pull-requests: write` permission is required. Check your workflow's `permissions` block. If your organisation has restricted default token permissions, you may need to explicitly grant `pull-requests: write` at the repo or org level in Settings → Actions → General.

**The action fires on pushes to main, not just PRs**
The example workflow only triggers on `pull_request`. If you added it to a `push` trigger, `github.event.pull_request.number` will be empty and the comment step is skipped (the Summary tab still works). Remove the `push` trigger or add a condition: `if: github.event_name == 'pull_request'`.

**Health score jumps on the first run**
The base branch has never been analysed. The first PR on a newly-added workflow has no comparison point, so the action reports HEAD metrics only. The second PR will have a baseline.
