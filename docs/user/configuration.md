# Configuration

CodeCartographer has four layers of configuration, applied in this priority order (highest first):

1. **CLI flags** — override everything for a single invocation
2. **Per-repo config** — `.codecartographer/config.toml` in the project root
3. **Global config** — `~/.config/codecartographer/config.toml`
4. **Defaults** — built-in defaults (`target = "claude"`)

## Global configuration

File: `~/.config/codecartographer/config.toml`

```toml
[defaults]
target = "claude"   # "claude" | "cursor" | "raw"
```

Manage via CLI:

```bash
codecartographer config --default-target claude    # set default output target
codecartographer config --default-target cursor
codecartographer config --default-target raw
codecartographer config --show                     # print current global config
```

## Per-repo configuration

File: `.codecartographer/config.toml` (created by `codecartographer init`)

Per-repo settings override the global config. Commit this file so your team shares the same settings.

```toml
[defaults]
target = "claude"

[ckb]
url = "http://localhost:3001"
webhook_url = "http://localhost:3002"
```

The `[ckb]` section is populated by `codecartographer init-ckb`.

## .codecartographerignore

Same syntax as `.gitignore`. Place in the project root.

```
# Skip generated files
generated/
*.pb.go

# Skip fixtures
tests/fixtures/large_*.json

# Skip vendor
vendor/
```

**Built-in noise filter (always active):** CodeCartographer automatically excludes the following even without a `.codecartographerignore`:

| Category | Paths |
|----------|-------|
| Package managers | `node_modules/`, `vendor/`, `.venv/`, `venv/` |
| Build output | `target/`, `dist/`, `build/`, `out/`, `.next/`, `__pycache__/` |
| Version control | `.git/` |
| Lock files | `package-lock.json`, `Cargo.lock`, `yarn.lock`, `go.sum`, `pnpm-lock.yaml` |
| Minified | `*.min.js`, `*.min.css` |
| Source maps | `*.map` |
| Log files | `*.log` |
| Large SVGs | SVG files over 2KB |
| Binary/media | Standard image, audio, video, font extensions |

Pass `--no-ignore` to any command to bypass both `.codecartographerignore` and the built-in filter for that invocation.

## Layer enforcement (`layers.toml`)

Define architectural layers and the allowed import flows between them.

**File location:** `layers.toml` in the project root, or `.codecartographer/layers.toml`.

```toml
[layers]
ui        = ["components", "pages", "views"]
services  = ["api", "auth", "billing"]
db        = ["models", "migrations", "queries"]
shared    = ["utils", "types", "constants"]

[allowed_flows]
ui       -> services
ui       -> shared
services -> db
services -> shared
db       -> shared
```

**Violation types:**
- **BackCall** — a lower layer imports from a higher one (e.g., `db` imports from `services`)
- **SkipCall** — a layer bypasses an intermediate layer (e.g., `ui` directly imports `db`)
- **CircularCrossLayer** — a cycle crosses layer boundaries
- **DirectForeignImport** — a module imports from a layer not in its allowed flows

Check violations:

```bash
codecartographer layers        # show all current violations with severity
codecartographer check         # exit non-zero if any violations exist (for CI)
```

The `check_layers` MCP tool returns violations over the MCP protocol.

## Output target (`--target`)

Controls the format of output files and MCP responses.

| Target | Description |
|--------|-------------|
| `claude` | Formatted XML with token budget metadata — default for Claude Code |
| `cursor` | Cursor-optimized format |
| `raw` | Plain output, no wrappers |

Set globally: `codecartographer config --default-target TARGET`

Override per-command: `codecartographer map --target raw`

## VS Code extension settings

If you are using the CodeCartographer VS Code extension, configure it via `settings.json`:

| Setting | Default | Description |
|---------|---------|-------------|
| `codecartographer.binaryPath` | `"codecartographer"` | Path to the `codecartographer` binary |
| `codecartographer.refreshOnSave` | `false` | Re-run file metrics whenever a source file is saved |
| `codecartographer.hotspotThreshold` | `50` | Minimum hotspot score to show a gutter decoration |
| `codecartographer.commitWindow` | `500` | Commit count for hotspot and churn analysis |
| `codecartographer.showEdgeKinds` | `["runtime"]` | Which import edge kinds to render in diagrams |
| `codecartographer.statusBar` | `true` | Show a health-score item in the VS Code status bar |
| `codecartographer.explorerBadges` | `true` | Show health/hotspot badges on files in the Explorer sidebar |

## MCP server configuration

The MCP server (`codecartographer serve`) uses the same per-repo config. There are no MCP-specific config keys — all behavior is governed by the repo's `.codecartographer/config.toml` and the global config.

Register the server in your MCP client:

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

For a specific working directory:

```json
{
  "mcpServers": {
    "codecartographer": {
      "command": "codecartographer",
      "args": ["serve", "/path/to/my-project"]
    }
  }
}
```

## CKB integration

```bash
codecartographer init-ckb --ckb-url http://localhost:3001 --webhook-url http://localhost:3002
```

Writes CKB connection details to `.codecartographer/config.toml`. CodeCartographer will then:
- Send blast-radius hints to CKB after graph changes
- Register webhooks so CKB can subscribe to change events
- Include `lip_uris` in `get_blast_radius` responses for CKB drill-down

See [Ecosystem](ecosystem.md) for how CodeCartographer and CKB interact.
