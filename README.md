# Code Cartographer for Architectural Intelligence

> The "GPS with Traffic Data" for your codebase - warns you about roadblocks before you even start driving.

## What is Cartographer?

Cartographer is a **structural intelligence engine** that maps your codebase's architecture, monitors its health, and predicts the ripple effects of changes before you make them.

It answers questions like:
- "What files are architectural bottlenecks?"
- "What happens if I change this function?"
- "Is my codebase getting healthier or more tangled?"
- "Who can I legally import from?"

## Quick Start

```bash
# Build
cd mapper-core/cargo && cargo build --release

# Generate architectural map
cartographer map

# Check health score
cartographer health

# Predict impact of a change
cartographer simulate --module src/auth/user.rs --new-signature "fn login(u: User)"

# See architectural trends
cartographer evolution --days 30
```

## Core Features

### 🗺️ The Map (Dependency Graph)
Generates `project_graph.json` - a complete dependency map at file/module level:
- Nodes: Files with their public API signatures
- Edges: Import/require/use relationships
- Metadata: Language, complexity estimates, bridge detection

### 🏛️ Bridge Detection
Identifies "Global Bridges" - files that connect disparate subsystems. Using **Bridgeness Centrality** (betweenness filtered to exclude noisy utility hubs), Cartographer finds the true architectural bottlenecks.

### 🛡️ Layer Enforcement
Prevents architectural drift with `layers.toml`:
```toml
[layers]
ui = ["components", "pages"]
services = ["api", "auth"]
db = ["models"]

[allowed_flows]
ui -> services
services -> db
```
Detects: BackCalls (db→ui), SkipCalls (ui→db without business layer)

### 📊 Health Scoring
Calculates architectural health from 0-100:
```
health = 100 - (cycles × 5) - (bridges × 2) - (god_modules × 3) - (violations × 4)
```

### 🔮 Predictive Simulation
Before you write code, Cartographer simulates the ripple effect:
- Will this create a cycle?
- Which modules will be affected?
- What layer violations will this cause?
- What's the health impact?

### 📈 Historical Evolution
Track architecture over time - see debt indicators, health trends, and get recommendations.

## Architecture

```
┌─────────────────────────────────────────────────────────┐
│                    Cartographer                          │
├─────────────────────────────────────────────────────────┤
│  mapper.rs     │ Skeleton extraction (10+ languages) │
│  api.rs         │ Graph generation, health scoring    │
│  layers.rs      │ Layer config, violation detection    │
│  webhooks.rs    │ Change notifications                 │
│  mcp.rs         │ MCP server for AI tool integration  │
└─────────────────────────────────────────────────────────┘
                        │
                        ▼ (webhook / API)
                   ┌─────────┐
                   │   CKB   │  ← Deep semantic analysis
                   └─────────┘
```

## Cartographer vs CKB

| Aspect | Cartographer | CKB |
|--------|--------------|-----|
| View | Macro (file/module) | Micro (symbol/AST) |
| Speed | Fast (regex) | Deep (AST) |
| Purpose | Map, warn, predict | Analyze, refactor |
| Output | `project_graph.json` | Call graphs, refs |

**The handoff:** Cartographer identifies "where to look," CKB does the deep analysis there.

## CLI Commands

| Command | Description |
|---------|-------------|
| `cartographer map` | Generate skeleton map |
| `cartographer watch` | Live file watching |
| `cartographer health` | Show health score |
| `cartographer simulate` | Predict change impact |
| `cartographer evolution` | Architectural trends |
| `cartographer init-ckb` | Setup CKB integration |

## Token Efficiency

Cartographer achieves **90%+ token reduction** vs full source code:
- Full source: ~5,000 tokens/module
- Cartographer skeleton: ~200 tokens/module
- AI-Lang compression strips `pub`, `private`, `async`, etc.

## Integrations

- **MCP Server** - AI tools can query via Model Context Protocol
- **Webhooks** - Notify CKB when graph changes
- **CKB** - Uses Cartographer as a filter for deep analysis

## Version History

- **v1.1.0** - Predictive simulation, historical evolution
- **v1.0.0** - CKB integration, symbol mapping
- **v0.5.0** - Layer enforcement, border patrol
- **v0.4.0** - Health monitoring, cycle/god detection
- **v0.3.0** - Bridge detection, AI-Lang compression
- **v0.2.0** - API, MCP server

## Author

SimplyLiz