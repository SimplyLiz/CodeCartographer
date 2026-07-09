# Search & Find — Reference

CodeCartographer provides two commands — `search` and `find` — that give AI tools grep/find parity without leaving the project context. Both respect `.codecartographerignore` and the built-in noise filter (vendor, generated files, binaries) by default.

---

## `codecartographer search <PATTERN>`

Grep-like content search across all project files.

```
codecartographer search <PATTERN> [OPTIONS]
```

### Flags

| Flag | Short | Default | Description |
|------|-------|---------|-------------|
| `--regexp PATTERN` | `-e` | — | Additional pattern OR'd with the primary (repeatable) |
| `--literal` | | false | Treat pattern as a literal string, not regex |
| `--ignore-case` | `-i` | false | Case-insensitive matching |
| `--invert-match` | `-v` | false | Show lines that do NOT match |
| `--word-regexp` | `-w` | false | Whole-word matching (`\b…\b`) |
| `--only-matching` | `-o` | false | Print only the matched portion of each line |
| `--files-with-matches` | `-l` | false | Print only file paths that have matches |
| `--files-without-match` | | false | Print only file paths with NO matches |
| `--count` | `-c` | false | Print match count per file (`path:N`) |
| `--after-context N` | `-A` | 0 | Lines of context after each match |
| `--before-context N` | `-B` | 0 | Lines of context before each match |
| `--context N` | `-C` | 0 | Lines of context before and after (sets both) |
| `--glob GLOB` | | — | Include only files matching glob (e.g. `"*.rs"`) |
| `--exclude GLOB` | | — | Exclude files matching glob |
| `--path SUBDIR` | | — | Restrict to this repo-relative subdirectory |
| `--limit N` | | 100 | Maximum matches to return (0 = unlimited) |
| `--no-ignore` | | false | Search vendor/generated/noise files too |

### Examples

```bash
# Find all TODO/FIXME comments in Rust files
codecartographer search "TODO\|FIXME" --glob "*.rs"

# Same, case-insensitive, with 2 lines of context
codecartographer search "todo" -i -C 2 --glob "*.rs"

# Multiple patterns (OR) — find either error string
codecartographer search "connection refused" -e "dial tcp" --glob "*.go"

# Whole-word: find "fn" but not "fn_ptr" or "async_fn"
codecartographer search "fn" -w --glob "*.rs"

# List files that import a specific package
codecartographer search "from auth import" -l --glob "*.py"

# Count how many times each file references a constant
codecartographer search "MAX_RETRY" -c

# Only show the matched expression on each line
codecartographer search "version = \"[^\"]+\"" -o --glob "Cargo.toml" --no-ignore

# Find all lines NOT matching (files missing a license header)
codecartographer search "Copyright" -v -l --glob "*.go"

# Search within a subdirectory
codecartographer search "TODO" --path src/api --glob "*.go"

# Find error strings in non-code config files
codecartographer search "error" --glob "*.yaml" --no-ignore

# Invert + count: files with NO test coverage marker
codecartographer search "// coverage: ignore" --files-without-match --glob "*.go"
```

### Output format

Normal mode (one file header per group, line number prefix):
```
src/api.rs:
     42: pub fn authenticate(user: &User) -> Result<Token> {
     67: pub fn validate_token(t: &str) -> bool {

src/auth.rs:
    103: pub fn refresh_token(old: &Token) -> Result<Token> {
```

Context mode (`-C 2`):
```
src/api.rs:
     40-use crate::auth::Token;
     41-
     42:pub fn authenticate(user: &User) -> Result<Token> {
     43-    // implementation
     44-
```

`-l` mode: one path per line, no line numbers.

`-c` mode: `path:N` per file.

`-o` mode: prints only the matched text, prefixed with line number.

---

## `codecartographer find <PATTERN>`

Find files by name/path glob with optional mtime, size, and depth filters.

```
codecartographer find <PATTERN> [OPTIONS]
```

`PATTERN` uses glob syntax: `*` matches within a path segment, `**` crosses segment boundaries, `?` matches any single character. Patterns without `/` are matched against the filename only.

### Flags

| Flag | Default | Description |
|------|---------|-------------|
| `--modified-since DURATION` | — | Files modified within this duration. Format: `24h`, `7d`, `30m`, `3600s` |
| `--newer FILE` | — | Files with mtime newer than `FILE`'s mtime (repo-relative path) |
| `--min-size BYTES` | — | Minimum file size in bytes (inclusive) |
| `--max-size BYTES` | — | Maximum file size in bytes (inclusive) |
| `--max-depth N` | — | Maximum directory depth (0 = root files only, 1 = one level deep, …) |
| `--limit N` | 50 | Maximum files to return (0 = unlimited) |
| `--no-ignore` | false | Include vendor/generated/noise files |

### Examples

```bash
# Find all Rust source files
codecartographer find "*.rs"

# Find Go files changed in the last 24 hours
codecartographer find "*.go" --modified-since 24h

# Find files newer than go.mod (recently added)
codecartographer find "*.go" --newer go.mod

# Find large files (possible accidental commits)
codecartographer find "*" --min-size 1048576

# Find small config files at root level only
codecartographer find "*.toml" --max-depth 0

# Find generated protobuf files (normally ignored)
codecartographer find "*.pb.go" --no-ignore

# Find recently modified test files
codecartographer find "*_test.go" --modified-since 1h

# Find TypeScript files in src, not too deep
codecartographer find "src/**/*.ts" --max-depth 3

# Find files within a size range (likely data files)
codecartographer find "*" --min-size 10000 --max-size 100000
```

### Output format

```
  src/api.rs      [Rust, 49.4K]  2026-04-09T12:27:43Z
  src/auth.rs     [Rust, 8.1K]   2026-04-09T11:05:12Z
  src/mapper.rs   [Rust, 56.8K]  2026-04-08T22:14:03Z
```

Fields: `path`, `[language, size]`, `ISO-8601 mtime`.

---

## `codecartographer context --query <PATTERN>`

Bundle ranked skeleton + search results into a single stdout emission for models without tool-call support.

```bash
codecartographer context --focus src/api.rs --budget 8000 --query "authentication"
```

Outputs:
1. `## Ranked Architecture Skeleton` — top files by PageRank weight toward `--focus` files
2. `## Search Results for "authentication"` — matching lines with 2 lines of context

Designed for piping into local models:
```bash
codecartographer context --focus src/api.rs --query "TODO" | ollama run qwen3
codecartographer context --budget 4000 --query "error handling" > context.txt
```

---

## FFI (CKB / CGo)

Both functions are exposed in `libcode_cartographer.a` via `include/codecartographer.h`.

### `codecartographer_search_content`

```c
char* codecartographer_search_content(
    const char* path,       // absolute repo root
    const char* pattern,    // primary search pattern
    const char* opts_json   // JSON SearchOptions or NULL for defaults
);
```

`opts_json` fields (all optional):

```json
{
  "literal":           false,
  "caseSensitive":     true,
  "contextLines":      0,
  "beforeContext":     0,
  "afterContext":      0,
  "maxResults":        100,
  "fileGlob":          "*.rs",
  "excludeGlob":       "*.gen.go",
  "extraPatterns":     ["FIXME", "HACK"],
  "invertMatch":       false,
  "wordRegexp":        false,
  "onlyMatching":      false,
  "filesWithMatches":  false,
  "filesWithoutMatch": false,
  "countOnly":         false,
  "noIgnore":          false,
  "searchPath":        "src/api"
}
```

Returns JSON envelope `{ "ok": true, "data": SearchResult }`.

**SearchResult shape:**
```json
{
  "matches": [
    {
      "path": "src/api.rs",
      "lineNumber": 42,
      "line": "pub fn authenticate(user: &User) -> Result<Token> {",
      "matchedTexts": [],
      "beforeContext": [],
      "afterContext": []
    }
  ],
  "totalMatches": 1,
  "filesSearched": 18,
  "truncated": false,
  "filesWithMatches": [],
  "filesWithoutMatch": [],
  "fileCounts": []
}
```

`filesWithMatches`, `filesWithoutMatch`, and `fileCounts` are only populated when the corresponding mode flag is set.

### `codecartographer_find_files`

```c
char* codecartographer_find_files(
    const char* path,       // absolute repo root
    const char* pattern,    // glob pattern
    unsigned int limit,     // max files, 0 = unlimited
    const char* opts_json   // JSON FindOptions or NULL for defaults
);
```

`opts_json` fields (all optional):

```json
{
  "modifiedSinceSecs": 86400,
  "newerThan":         "go.mod",
  "minSizeBytes":      1024,
  "maxSizeBytes":      1048576,
  "maxDepth":          3,
  "noIgnore":          false
}
```

Returns JSON envelope `{ "ok": true, "data": FindResult }`.

**FindResult shape:**
```json
{
  "files": [
    {
      "path": "src/api.rs",
      "language": "Rust",
      "sizeBytes": 50534,
      "modified": "2026-04-09T12:27:43Z"
    }
  ],
  "totalMatches": 1,
  "truncated": false
}
```

---

## Go bridge (CKB)

```go
import "github.com/SimplyLiz/CodeMCP/internal/codecartographer"

// Search — nil opts = defaults
result, err := codecartographer.SearchContent(repoRoot, "TODO", &codecartographer.SearchContentOptions{
    FileGlob:      "*.go",
    FilesWithMatches: true,
})

// Find — nil opts = defaults
result, err := codecartographer.FindFiles(repoRoot, "*.go", 0, &codecartographer.FindOptions{
    ModifiedSinceSecs: ptr(uint64(86400)),
})

// Check availability before calling
if codecartographer.Available() {
    // ...
}
```

`SearchContentOptions` mirrors the JSON fields above (camelCase → Go PascalCase).  
`FindOptions` mirrors `FindOptions` JSON fields.

Both functions return `ErrUnavailable` when built without `-tags codecartographer`.

---

## MCP tools

When `codecartographer serve` is running, both tools are available to any MCP client:

**`search_content`** — arguments map 1:1 to `SearchContentOptions` fields plus `pattern`:

```json
{
  "name": "search_content",
  "arguments": {
    "pattern": "TODO",
    "fileGlob": "*.go",
    "contextLines": 2,
    "filesWithMatches": true
  }
}
```

**`find_files`** — arguments map to `FindOptions` fields plus `pattern` and `limit`:

```json
{
  "name": "find_files",
  "arguments": {
    "pattern": "*.go",
    "limit": 50,
    "modifiedSinceSecs": 86400
  }
}
```

---

## `codecartographer replace <PATTERN> <REPLACEMENT>`

Sed-like in-place find-and-replace across all project files. Supports full regex with capture-group back-references, dry-run preview, and per-file `.bak` backups.

```
codecartographer replace <PATTERN> <REPLACEMENT> [OPTIONS]
```

`REPLACEMENT` supports `$0` (whole match) and `$1`/`$2` … (numbered capture groups).

### Flags

| Flag | Short | Default | Description |
|------|-------|---------|-------------|
| `--literal` | | false | Treat pattern as a literal string, not regex |
| `--ignore-case` | `-i` | false | Case-insensitive matching |
| `--word-regexp` | `-w` | false | Whole-word matching (`\b…\b`) |
| `--dry-run` | | false | Show a diff of what would change; write nothing |
| `--backup` | | false | Write a `.bak` copy before modifying each file |
| `--context N` | `-C` | 3 | Context lines shown in diff output |
| `--glob GLOB` | | — | Include only files matching glob (e.g. `"*.rs"`) |
| `--exclude GLOB` | | — | Exclude files matching glob |
| `--path SUBDIR` | | — | Restrict to this repo-relative subdirectory |
| `--max-per-file N` | | 0 | Cap replacements per file (0 = unlimited) |
| `--no-ignore` | | false | Bypass noise/vendor filter |

### Examples

```bash
# Dry-run: preview renaming a function across all Rust files
codecartographer replace "fn authenticate\b" "fn auth" --glob "*.rs" --dry-run

# Rename with capture groups — reorder two arguments
codecartographer replace "connect\((\w+),\s*(\w+)\)" "connect($2, $1)" --glob "*.go"

# Case-insensitive literal rename, with backup safety net
codecartographer replace --literal --ignore-case "TODO" "FIXME" --backup --glob "*.rs"

# Whole-word rename: "ctx" but not "context"
codecartographer replace "ctx" "rctx" -w --glob "*.go"

# Cap to 1 replacement per file (first occurrence only)
codecartographer replace "import React" "import React, { StrictMode }" --glob "*.tsx" --max-per-file 1

# Replace inside a subdirectory only
codecartographer replace "v1/api" "v2/api" --path src/http --glob "*.go"

# Bump a hard-coded version string across all config files
codecartographer replace "version = \"1\.7\.\d+\"" "version = \"1.8.0\"" --glob "*.toml" --no-ignore
```

### Output format

Dry-run and live runs both emit a per-file diff followed by a summary:

```
src/api.rs  (4 replacements)
  10 - pub fn authenticate(user: &User) -> Result<Token> {
  10 + pub fn auth(user: &User) -> Result<Token> {
  ...

Summary: 3 files changed, 12 replacements total
```

Without `--dry-run` the summary line also confirms `(written)`.

---

## `codecartographer extract <PATTERN>`

Awk-like value extraction — pull specific pieces of text out of every matching line across the project. Supports capture groups, frequency tables, deduplication, and structured output.

```
codecartographer extract <PATTERN> [OPTIONS]
```

`PATTERN` is a regex. Wrap the portion you care about in capture groups: e.g. `pub fn (\w+)` to extract function names.

### Flags

| Flag | Short | Default | Description |
|------|-------|---------|-------------|
| `--group N` | `-g` | 0 | Capture group index to extract (repeatable; 0 = whole match) |
| `--sep SEP` | | `\t` | Separator between groups when multiple `-g` are given |
| `--format text\|json\|csv\|tsv` | | `text` | Output format |
| `--count` | | false | Aggregate: emit a frequency table sorted by count descending |
| `--dedup` | | false | Deduplicate extracted values |
| `--sort` | | false | Sort output alphabetically; with `--count` sorts by frequency |
| `--ignore-case` | `-i` | false | Case-insensitive matching |
| `--glob GLOB` | | — | Include only files matching glob |
| `--exclude GLOB` | | — | Exclude files matching glob |
| `--path SUBDIR` | | — | Restrict to this repo-relative subdirectory |
| `--limit N` | | 1000 | Cap total results returned (0 = unlimited) |
| `--no-ignore` | | false | Bypass noise/vendor filter |

### Examples

```bash
# Extract all public function names from Rust source
codecartographer extract "pub fn (\w+)" -g 1 --glob "*.rs" --dedup --sort

# Frequency table: which functions are called most often?
codecartographer extract "(\w+)\s*\(" -g 1 --glob "*.rs" --count

# Extract HTTP status codes returned in Go handlers
codecartographer extract "http\.StatusCode\((\d+)\)|w\.WriteHeader\((\d+)\)" -g 1 -g 2 --glob "*.go" --count

# Pull all import paths from Go files, deduplicated
codecartographer extract '"([^"]+)"' -g 1 --glob "*.go" --path src --dedup --sort

# Find every TODO author tag — emit as CSV
codecartographer extract "TODO\((\w+)\)" -g 1 --glob "*.go" --format csv --count

# Extract semver strings across all TOML/JSON config files
codecartographer extract "(\d+\.\d+\.\d+)" -g 1 --glob "*.toml" --dedup --sort --no-ignore

# Whole-match extraction (group 0): pull all URLs from docs
codecartographer extract "https?://[^\s\)]+" --glob "*.md" --dedup
```

### Output format

**text** (default): one extracted value per line, prefixed with location:
```
src/api.rs:42       authenticate
src/api.rs:67       validate_token
src/auth.rs:103     refresh_token
```

**`--count`** mode: frequency table, highest first:
```
  42  authenticate
  17  validate_token
   8  refresh_token
```

**json**: see Extract response shape in the FFI section below.

**csv** / **tsv**: header row (`path,line,group0[,group1,…]`), one row per match.

---

## FFI (CKB / CGo)

Both functions are exposed in `libcode_cartographer.a` via `include/codecartographer.h`.

### `codecartographer_replace_content`

```c
char *codecartographer_replace_content(
    const char *path,         // absolute repo root
    const char *pattern,      // regex (or literal) pattern
    const char *replacement,  // replacement string; $0/$1/$2 back-references
    const char *opts_json     // JSON ReplaceOptions or NULL for defaults
);
```

`opts_json` fields (all optional):

```json
{
  "literal":      false,
  "caseSensitive": true,
  "wordRegexp":   false,
  "dryRun":       false,
  "backup":       false,
  "contextLines": 3,
  "fileGlob":     "*.rs",
  "excludeGlob":  null,
  "searchPath":   null,
  "noIgnore":     false,
  "maxPerFile":   0
}
```

Returns JSON envelope `{ "ok": true, "data": ReplaceResult }`.

**ReplaceResult shape:**
```json
{
  "filesChanged": 3,
  "totalReplacements": 12,
  "dryRun": false,
  "changes": [
    {
      "path": "src/api.rs",
      "replacements": 4,
      "diff": [
        { "kind": "removed", "lineNumber": 10, "content": "old line" },
        { "kind": "added",   "lineNumber": 10, "content": "new line" }
      ]
    }
  ]
}
```

### `codecartographer_extract_content`

```c
char *codecartographer_extract_content(
    const char *path,       // absolute repo root
    const char *pattern,    // regex with optional capture groups
    const char *opts_json   // JSON ExtractOptions or NULL for defaults
);
```

`opts_json` fields (all optional):

```json
{
  "groups":       [1, 2],
  "separator":    "\t",
  "format":       "text",
  "count":        false,
  "dedup":        false,
  "sort":         false,
  "caseSensitive": true,
  "fileGlob":     null,
  "excludeGlob":  null,
  "searchPath":   null,
  "noIgnore":     false,
  "limit":        0
}
```

`groups` is a list of capture-group indices to extract. An empty list or `[0]` returns the whole match.

Returns JSON envelope `{ "ok": true, "data": ExtractResult }`.

**ExtractResult shape:**
```json
{
  "matches": [
    {
      "path": "src/api.rs",
      "lineNumber": 42,
      "groups": ["pub fn foo", "foo"]
    }
  ],
  "counts": [],
  "total": 1,
  "filesSearched": 18,
  "truncated": false
}
```

`counts` is populated when `"count": true`; each entry is `{ "value": "foo", "count": 42 }`. `matches` is empty in that mode.

---

## Go bridge (CKB)

```go
import "github.com/SimplyLiz/CodeMCP/internal/codecartographer"

// Replace — nil opts = defaults
result, err := codecartographer.ReplaceContent(repoRoot, `fn authenticate\b`, "fn auth", &codecartographer.ReplaceOptions{
    FileGlob: "*.rs",
    DryRun:   true,
})

// Extract — nil opts = defaults
result, err := codecartographer.ExtractContent(repoRoot, `pub fn (\w+)`, &codecartographer.ExtractOptions{
    Groups: []int{1},
    Dedup:  true,
    Sort:   true,
    FileGlob: "*.rs",
})
```

`ReplaceOptions` and `ExtractOptions` mirror the JSON fields above (camelCase → Go PascalCase).

Both functions return `ErrUnavailable` when built without `-tags codecartographer`.

---

## MCP tools

When `codecartographer serve` is running, both tools are available to any MCP client:

**`replace_content`** — arguments map 1:1 to `ReplaceOptions` fields plus `pattern` and `replacement`:

```json
{
  "name": "replace_content",
  "arguments": {
    "pattern": "fn authenticate",
    "replacement": "fn auth",
    "fileGlob": "*.rs",
    "dryRun": true
  }
}
```

**`extract_content`** — arguments map to `ExtractOptions` fields plus `pattern`:

```json
{
  "name": "extract_content",
  "arguments": {
    "pattern": "pub fn (\\w+)",
    "groups": [1],
    "count": true,
    "fileGlob": "*.rs"
  }
}
```

---

## Noise filter

By default both commands skip:

- `vendor/`, `node_modules/`, `dist/`, `build/`, `target/`, `.next/`
- Generated files: `*.pb.go`, `*.gen.go`, `*.min.js`, `*.d.ts`, `*.freezed.dart`, …
- Binary and non-UTF-8 files (silently skipped on read failure)
- Files listed in `.codecartographerignore`

Pass `--no-ignore` to bypass all of this and search everything under the root.
