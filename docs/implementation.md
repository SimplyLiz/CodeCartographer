# Project Cartographer - Implementation Plan

## Overview

Cartographer is not a separate project - it's a **layer on top of CMP** that adds architectural intelligence. It uses CMP's high-performance Rust core for plumbing while adding unique value for CKB.

## The Pod Architecture

```
┌─────────────────────────────────────────────────────┐
│                    Cartographer                      │
│                                                     │
│  ┌─────────────────┐    ┌────────────────────────┐│
│  │   mapper.rs      │    │    API / MCP Server    ││
│  │ (Skeleton Layer) │    │  (Graph Queries)       ││
│  └────────┬─────────┘    └───────────┬────────────┘│
│           │                          │             │
│  ┌────────▼──────────────────────────▼───────────┐ │
│  │              CMP Core (Rust)                  │ │
│  │                                              │ │
│  │  • uc_sync.rs  - Push/Pull, Versioning       │ │
│  │  • uc_client.rs - Cloud Communication        │ │
│  │  • uc_agents.rs - Agent Registry            │ │
│  │  • scanner.rs   - File Discovery             │ │
│  │  • formatter.rs - Token Optimization        │ │
│  │  • webhooks.rs  - Change Notifications       │ │
│  └──────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────┘
                        │
                        ▼ (via webhook)
                   ┌─────────┐
                   │   CKB   │  ← Uses map to teleport
                   │ (Brain) │    deep analysis to right files
                   └─────────┘
```

## Implementation Priorities

### Priority 1: CMP Core Integration
**Files to port from CMP:**
- `uc_sync.rs` - Handle "Cloud/Local" state of map
- `uc_client.rs` - Communication with UltraContext
- `sync.rs` - Incremental sync with hash-based dirty detection
- `memory.rs` - Versioned map storage

**What this gives us:**
- Incremental sync (only re-map changed files)
- Versioned maps (time travel for historical analysis)
- Background file watcher with debounce

### Priority 2: Enhanced Skeleton Layer
**Enhance `mapper.rs`:**
- Add signature-only extraction (strip bodies)
- Multi-level detail (minimal/standard/extended)
- AI-Lang compression mode (90%+ token savings)
- Bridge module detection

**New fields in MappedFile:**
```rust
pub struct MappedFile {
    pub path: String,
    pub imports: Vec<String>,
    pub signatures: Vec<String>,      // function/class signatures only
    pub docstrings: Option<Vec<String>>,
    pub parameters: Option<Vec<String>>,
    pub return_types: Option<Vec<String>>,
    pub is_bridge: Option<bool>,        // connects disparate subsystems
}
```

### Priority 3: Agent Integration
**Connect `uc_agents.rs`:**
- Multi-agent registry (Cursor, Copilot, Claude)
- Context health scoring (track which modules AI uses most)
- Webhook notifications to agents on map changes

**Agent consumption:**
- Each agent can query the map via API
- Agents see same architectural boundaries
- Health score tells CKB which areas to prioritize

## Key Differentiators from CMP

| Feature | CMP | Cartographer |
|---------|-----|--------------|
| Output | Full source | Skeleton only (signatures) |
| Token cost | Medium | Minimal (90%+ savings) |
| Use case | Context for LLM | Architectural map for CKB |
| Graph | None | `project_graph.json` |
| Bridge detection | No | Yes |

## API Endpoints

```yaml
# For AI Agents / ShellAI
GET /api/v1/module-context?module_id=src/auth/user.rs&detail_level=standard

# For CKB
GET /api/v1/graph                    # Full project graph
GET /api/v1/graph/dependencies?module_id=src/main.rs&depth=1
GET /api/v1/blast-radius?target=src/utils/helper.rs

# For Webhooks
POST /api/v1/webhooks                # Register for graph updates
```

## Next Steps

1. ✅ Architecture defined
2. ⏳ Port CMP core modules
3. ⏳ Enhance mapper.rs
4. ⏳ Build graph generation
5. ⏳ Add bridge detection
6. ⏳ Connect agent registry

**Start with Priority 1?** We can port the CMP core and get the basic sync working, then layer on Cartographer's unique features.