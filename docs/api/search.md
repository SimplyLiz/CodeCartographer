# Search & Find — Reference

Cartographer provides two commands — `search` and `find` — that give AI tools grep/find parity without leaving the project context. Both respect `.cartographerignore` and the built-in noise filter (vendor, generated files, binaries) by default.

---

## `cartographer search <PATTERN>`

Grep-like content search across all project files.

```
cartographer search <PATTERN> [OPTIONS]
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
cartographer search "TODO\|FIXME" --glob "*.rs"

# Same, case-insensitive, with 2 lines of context
cartographer search "todo" -i -C 2 --glob "*.rs"

# Multiple patterns (OR) — find either error string
cartographer search "connection refused" -e "dial tcp" --glob "*.go"

# Whole-word: find "fn" but not "fn_ptr" or "async_fn"
cartographer search "fn" -w --glob "*.rs"

# List files that import a specific package
cartographer search "from auth import" -l --glob "*.py"

# Count how many times each file references a constant
cartographer search "MAX_RETRY" -c

# Only show the matched expression on each line
cartographer search "version = \"[^\"]+\"" -o --glob "Cargo.toml" --no-ignore

# Find all lines NOT matching (files missing a license header)
cartographer search "Copyright" -v -l --glob "*.go"

# Search within a subdirectory
cartographer search "TODO" --path src/api --glob "*.go"

# Find error strings in non-code config files
cartographer search "error" --glob "*.yaml" --no-ignore

# Invert + count: files with NO test coverage marker
cartographer search "// coverage: ignore" --files-without-match --glob "*.go"
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

## `cartographer find <PATTERN>`

Find files by name/path glob with optional mtime, size, and depth filters.

```
cartographer find <PATTERN> [OPTIONS]
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
cartographer find "*.rs"

# Find Go files changed in the last 24 hours
cartographer find "*.go" --modified-since 24h

# Find files newer than go.mod (recently added)
cartographer find "*.go" --newer go.mod

# Find large files (possible accidental commits)
cartographer find "*" --min-size 1048576

# Find small config files at root level only
cartographer find "*.toml" --max-depth 0

# Find generated protobuf files (normally ignored)
cartographer find "*.pb.go" --no-ignore

# Find recently modified test files
cartographer find "*_test.go" --modified-since 1h

# Find TypeScript files in src, not too deep
cartographer find "src/**/*.ts" --max-depth 3

# Find files within a size range (likely data files)
cartographer find "*" --min-size 10000 --max-size 100000
```

### Output format

```
  src/api.rs      [Rust, 49.4K]  2026-04-09T12:27:43Z
  src/auth.rs     [Rust, 8.1K]   2026-04-09T11:05:12Z
  src/mapper.rs   [Rust, 56.8K]  2026-04-08T22:14:03Z
```

Fields: `path`, `[language, size]`, `ISO-8601 mtime`.

---

## `cartographer context --query <PATTERN>`

Bundle ranked skeleton + search results into a single stdout emission for models without tool-call support.

```bash
cartographer context --focus src/api.rs --budget 8000 --query "authentication"
```

Outputs:
1. `## Ranked Architecture Skeleton` — top files by PageRank weight toward `--focus` files
2. `## Search Results for "authentication"` — matching lines with 2 lines of context

Designed for piping into local models:
```bash
cartographer context --focus src/api.rs --query "TODO" | ollama run qwen3
cartographer context --budget 4000 --query "error handling" > context.txt
```

---

## FFI (CKB / CGo)

Both functions are exposed in `libcartographer.a` via `include/cartographer.h`.

### `cartographer_search_content`

```c
char* cartographer_search_content(
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

### `cartographer_find_files`

```c
char* cartographer_find_files(
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
import "github.com/SimplyLiz/CodeMCP/internal/cartographer"

// Search — nil opts = defaults
result, err := cartographer.SearchContent(repoRoot, "TODO", &cartographer.SearchContentOptions{
    FileGlob:      "*.go",
    FilesWithMatches: true,
})

// Find — nil opts = defaults
result, err := cartographer.FindFiles(repoRoot, "*.go", 0, &cartographer.FindOptions{
    ModifiedSinceSecs: ptr(uint64(86400)),
})

// Check availability before calling
if cartographer.Available() {
    // ...
}
```

`SearchContentOptions` mirrors the JSON fields above (camelCase → Go PascalCase).  
`FindOptions` mirrors `FindOptions` JSON fields.

Both functions return `ErrUnavailable` when built without `-tags cartographer`.

---

## MCP tools

When `cartographer serve` is running, both tools are available to any MCP client:

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

## Noise filter

By default both commands skip:

- `vendor/`, `node_modules/`, `dist/`, `build/`, `target/`, `.next/`
- Generated files: `*.pb.go`, `*.gen.go`, `*.min.js`, `*.d.ts`, `*.freezed.dart`, …
- Binary and non-UTF-8 files (silently skipped on read failure)
- Files listed in `.cartographerignore`

Pass `--no-ignore` to bypass all of this and search everything under the root.
