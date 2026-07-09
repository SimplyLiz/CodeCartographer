# Search

CodeCartographer provides four complementary search commands: `search` (grep-like content search), `find` (file discovery), `replace` (regex find-and-replace), and `extract` (capture-group extraction). All four respect `.codecartographerignore` and the built-in noise filter by default.

## search — content search

```bash
codecartographer search PATTERN [OPTIONS]
```

Grep-like search across file contents. Supports both regex and literal matching.

```bash
codecartographer search "AuthService"
codecartographer search "fn verify_token" --glob "*.rs"
codecartographer search "TODO|FIXME" -i
codecartographer search "^import" --path src/api
```

**Key flags:**

| Flag | Short | Description |
|------|-------|-------------|
| `--ignore-case` | `-i` | Case-insensitive match |
| `--invert-match` | `-v` | Show lines that do NOT match |
| `--word-regexp` | `-w` | Match whole words only |
| `--after-context N` | `-A N` | Show N lines after each match |
| `--before-context N` | `-B N` | Show N lines before each match |
| `--context N` | `-C N` | Show N lines before and after each match |
| `--glob PATTERN` | | Restrict to files matching a glob (e.g., `*.rs`, `src/**/*.ts`) |
| `--path DIR` | | Restrict to files under a directory |
| `--literal` | | Treat PATTERN as a literal string, not a regex |
| `--no-ignore` | | Bypass `.codecartographerignore` and noise filter |

**Output:** File path, line number, and matched line — same format as `grep`.

### Search inside a symbol

```bash
codecartographer search-in-symbol --file FILE --symbol SYMBOL --pattern PATTERN [--context-lines N]
```

Scopes the search to the body of a named function or method. Useful for finding a pattern that appears in many places but you only care about one function.

```bash
codecartographer search-in-symbol --file src/auth.rs --symbol verify_token --pattern "unwrap"
```

## find — file discovery

```bash
codecartographer find PATTERN [OPTIONS]
```

Glob-based file discovery. Finds files by path pattern rather than content.

```bash
codecartographer find "*.rs"
codecartographer find "src/**/*.test.ts"
codecartographer find "**/*auth*"
codecartographer find "*.toml" --max-depth 2
codecartographer find "*.log" --modified-since 24h
codecartographer find "*.rs" --min-size 10kb
```

**Key flags:**

| Flag | Description |
|------|-------------|
| `--max-depth N` | Limit directory traversal depth |
| `--modified-since DURATION` | Only files modified within DURATION (e.g., `24h`, `7d`, `30m`) |
| `--min-size SIZE` | Only files larger than SIZE (e.g., `10kb`, `1mb`) |
| `--max-size SIZE` | Only files smaller than SIZE |

**Output:** Path, detected language, and file size in bytes.

## replace — regex find-and-replace

```bash
codecartographer replace PATTERN REPLACEMENT [OPTIONS]
```

Regex find-and-replace across all scanned files. Supports capture groups (`$0` = full match, `$1`, `$2`, etc.).

```bash
# Preview changes before applying
codecartographer replace "AuthService" "AuthenticationService" --dry-run

# Apply the change
codecartographer replace "AuthService" "AuthenticationService"

# Use capture groups
codecartographer replace "fn (\w+)\(ctx: Context\)" "fn $1(ctx: RequestContext)"

# Restrict to specific files
codecartographer replace "console.log" "logger.debug" --glob "*.ts"

# Backup originals before replacing
codecartographer replace "old_api" "new_api" --backup

# Limit replacements per file
codecartographer replace "TODO" "FIXME" --max-per-file 1

# Show context around each replacement
codecartographer replace "old" "new" --dry-run --context-lines 3
```

**Key flags:**

| Flag | Description |
|------|-------------|
| `--dry-run` | Preview the diff without writing anything |
| `--literal` | Treat PATTERN as a literal string |
| `--case-sensitive` | Force case-sensitive match (default: auto) |
| `--glob PATTERN` | Restrict to files matching a glob |
| `--exclude-glob PATTERN` | Exclude files matching a glob |
| `--search-path DIR` | Restrict to a subdirectory |
| `--max-per-file N` | Maximum replacements per file |
| `--context-lines N` | Lines of context in dry-run output |
| `--backup` | Write `.bak` copies of modified files before replacing |

**Always run `--dry-run` first** on any non-trivial replacement. The diff output shows exactly what will change before you commit to it.

## extract — capture-group extraction

```bash
codecartographer extract PATTERN [OPTIONS]
```

Awk-like extraction of regex capture groups from file contents. Collects all matches and outputs the captured values.

```bash
# Extract all function names from Rust files
codecartographer extract "pub fn (\w+)" --glob "*.rs"

# Extract all import paths from TypeScript
codecartographer extract "from ['\"](.+)['\"]" --glob "*.ts"

# Count occurrences (frequency table)
codecartographer extract "use (\w+)::" --glob "*.rs" --count

# Deduplicate results
codecartographer extract "import .+ from ['\"](.+)['\"]" --dedup

# Sort alphabetically
codecartographer extract "pub (\w+)" --sort

# Specific capture groups (when pattern has multiple)
codecartographer extract "fn (\w+)\(([^)]+)\)" --groups 1,2

# Output as JSON
codecartographer extract "(\w+)=(\w+)" --groups 1,2 --format json
```

**Key flags:**

| Flag | Description |
|------|-------------|
| `--groups LIST` | Comma-separated capture group indices to output (default: all) |
| `--count` | Output a frequency table instead of raw matches |
| `--dedup` | Remove duplicate values |
| `--sort` | Sort output alphabetically |
| `--case-sensitive` | Force case-sensitive match |
| `--glob PATTERN` | Restrict to files matching a glob |
| `--search-path DIR` | Restrict to a subdirectory |
| `--limit N` | Maximum results to return |
| `--format text\|json\|csv\|tsv` | Output format (default: text) |

**Frequency table example:**

```bash
codecartographer extract "use (\w+)::" --glob "*.rs" --count --sort
```

Output:
```
42  std
31  anyhow
18  serde
12  tokio
 8  clap
```

## MCP equivalents

All four commands are also available as MCP tools for use inside Claude Code or Cursor:

| CLI | MCP tool |
|-----|----------|
| `codecartographer search` | `search_content` |
| `codecartographer find` | `find_files` |
| `codecartographer replace` | `replace_content` |
| `codecartographer extract` | `extract_content` |
| `codecartographer search-in-symbol` | `search_in_symbol` |

The MCP tools expose the same parameters as the CLI flags. See [MCP Tools](mcp-tools.md) for the full parameter reference.
