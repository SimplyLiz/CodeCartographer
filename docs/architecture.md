# Project Cartographer - Architecture Overview

## What is Cartographer?

Cartographer is a lightweight code analysis tool that provides **architectural intelligence** for AI coding assistants like ShellAI, Cursor, Copilot, and for CKB.

It builds a semantic map of your codebase - not the full code, but the **shape** (public APIs, imports, signatures) - and keeps it updated in real-time.

## Core Capabilities

### 1. Skeleton Extraction (The "Header-Only" View)
Extracts only public exports, type definitions, and signatures - completely stripping function bodies.

```
Before (full source):
```python
def authenticate_user(username: str, password: str) -> User:
    """Authenticate a user with username and password."""
    hash = hashlib.sha256(password.encode())
    # ... 50 more lines
```

After (skeleton):
```python
def authenticate_user(username: str, password: str) -> User: ...
```

**Supported Languages**: JS/TS, Rust, Python, Go, Java/Kotlin/Scala, C/C++, Ruby, PHP

### 2. Token Optimization
Achieves **90%+ token savings** compared to full source. Uses AI-Lang compression:
- Strips syntax boilerplate (`public`, `private`, `return`)
- Multiple output formats: Claude XML, Cursor Markdown, JSON

### 3. Project Graph (`project_graph.json`)
Dependency graph at file/module level:
- **Nodes**: Files with their exported signatures
- **Edges**: Import/require/use relationships
- **Metadata**: Language, complexity estimates, change frequency

### 4. Bridge Detection
Identifies "Bridge Modules" - files that connect disparate subsystems. These are architectural bottlenecks worth prioritizing for deep analysis.

## Relationship with CMP

Cartographer **uses CMP's core** for:
- Incremental sync with hash-based dirty file detection
- Versioned maps (time travel for historical analysis)
- Background file watching with debounce
- Agent management (Cursor, Copilot, Claude)
- Cloud sync (push/pull to UltraContext)
- Webhook notifications

Cartographer **provides on top**:
- Skeleton extraction logic (`mapper.rs` enhanced)
- Graph generation and bridge detection
- Token compression optimization
- Architectural health scoring

## Relationship with CKB

**They complement each other:**

| Aspect | Cartographer | CKB |
|--------|-------------|-----|
| Level | File/Module | Symbol |
| Depth | Shape only | Full semantic |
| Speed | Fast (regex) | Deep (AST) |
| Use case | Quick map, LLM context | Analysis, refactoring |

**Workflow:**
1. Cartographer provides the "map" - quick overview of structure
2. CKB uses the map to "teleport" its deep analysis to the right files
3. CKB doesn't scan everything - just the relevant modules

## Output Files

| File | Description |
|------|-------------|
| `project_graph.json` | Full dependency graph at file level |
| `cartographer_map.{xml,md,json}` | Skeleton map (CLI output) |
| `.cartographer_memory.json` | Versioned memory (Cartographer core) |

## CLI Commands

```bash
# Generate skeleton map (one-shot)
cartographer map

# Live watching (auto-update map on file changes)
cartographer watch

# Full source (legacy - for comparison)
cartographer source

# Push map to cloud
cartographer push

# Pull map from cloud
cartographer pull

# Agent management
cartographer agents list
cartographer agents add --name "My Agent" --type cursor --webhook https://...
```

## Integration Points

### For ShellAI / AI Agents
- Use `get_module_context` API to fetch skeleton of any module
- Use project graph for workspace understanding
- Configurable detail levels (minimal/standard/extended)

### For CKB
- Read `project_graph.json` for fast navigation
- Subscribe to webhooks for change notifications
- Use versioned maps for historical analysis