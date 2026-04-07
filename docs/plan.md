# Project Cartographer Improvement Plan

This document outlines the proposed improvements for Project Cartographer to optimize its usage with ShellAI, other tools via MCP or HTTP API, and to enhance its integration with CKB.

---

## Implementation Status

### ✅ Completed Items
- [x] **Section 1.1**: Enhanced `get_module_context` Detail - Added `DetailLevel` enum to `mapper.rs` (Minimal, Standard, Extended) with docstrings, parameters, and return types
- [x] **Section 2.4**: Formal API Schema and Versioning - Created OpenAPI 3.0 spec at `docs/api/openapi.yaml`
- [x] **Section 2.1**: Webhook Notifications - Implemented `webhooks.rs` with event types and delivery system
- [x] **Section 2.2**: API for Graph Querying - Implemented in `api.rs` (get_dependencies, get_dependents, search_graph)
- [x] **Section 2.3**: Configurable Compression Levels - Added `CompressionLevel` enum to `api.rs`
- [x] **MCP Server**: Created `mcp.rs` exposing Cartographer via Model Context Protocol

### 🔄 In Progress
- [ ] **Section 1.2**: Symbol-Level Context Retrieval - Need to add `symbol_name` parameter support to API
- [ ] **Section 1.3**: "Blast Radius" Context for Changes - Need to implement endpoint
- [ ] **Section 3**: CKB Integration - Need to integrate Cartographer with CKB's indexing pipeline

---

## 1. For ShellAI and other AI Coding Tools

### 1.1. Enhanced `get_module_context` Detail ✅
Introduce a configurable `detail_level` parameter to the `get_module_context` API.
- `minimal` (current): Only public API signatures.
- `standard`: Include brief parameter descriptions (from docstrings/comments if available and concise), return types, and 1-2 lines of introductory docstring for functions/classes.
- `extended`: Include enum definitions, constant values, and simple interface properties.
**Benefit**: Allows AI agents to understand the *intent* and basic usage of a symbol without needing to read entire implementations, significantly improving their ability to generate or modify code correctly.

### 1.2. Symbol-Level Context Retrieval 🔄
Add a new API endpoint or extend `get_module_context` to accept a `symbol_name` in addition to `moduleId`. This would return the compressed context *only* for that specific symbol and its immediate dependencies/references.
**Benefit**: Enables more granular and highly targeted context retrieval, further reducing token usage for specific queries.

### 1.3. "Blast Radius" Context for Changes 🔄
Implement an API endpoint that, given a file or symbol, returns a list of *related* files/symbols (e.g., direct callers, direct callees, files in the same logical module, recently co-changed files) along with their compressed context.
**Benefit**: Helps AI agents understand the implications of a change and ensure consistency across interdependent parts of the codebase.

## 2. For MCP/Other Tools (HTTP API)

### 2.1. Implement Webhook Notifications ✅
Prioritize implementing webhook notifications for `project_graph.json` updates, allowing other services to react in real-time rather than polling.
**Benefit**: Reduces latency for tools that depend on the graph and conserves resources.

### 2.2. API for Graph Querying ✅
Introduce API endpoints to query the generated `project_graph.json` for specific data:
- `GET /api/v1/graph/dependencies?moduleId=<id>`: Get direct/transitive dependencies of a module.
- `GET /api/v1/graph/dependents?moduleId=<id>`: Get modules that depend on a given module.
- `GET /api/v1/graph/search?query=<pattern>&type=<node|edge>`: Search for nodes or edges matching a pattern.
**Benefit**: Prevents consumers from having to download and parse the entire graph, making integration more efficient for specific use cases.

### 2.3. Configurable Compression Levels ✅
Implement configuration options for compression levels in both `get_module_context` and `project_graph.json` generation. This could involve options like stripping comments, variable renaming, or more aggressive summarization.
**Benefit**: Provides flexibility for various downstream tools, from highly token-constrained LLMs to more robust analysis tools.

### 2.4. Formal API Schema and Versioning ✅
Publish an OpenAPI (Swagger) specification for the HTTP API and a JSON Schema for `project_graph.json`. Implement API versioning (`/api/v1/`).
**Benefit**: Ensures predictable consumption by external tools, simplifies client development, and allows for future API evolution.

## 3. How CKB Can Profit

### 3.1. Leverage Cartographer's Change Detection 🔄
CKB could subscribe to Project Cartographer's internal file system change monitoring and incremental update mechanisms, being notified of changed files directly.
**Benefit**: Reduces redundant file system operations, speeding up CKB's re-indexing process and improving overall system responsiveness.

### 3.2. Pre-computation for CKB Analysis 🔄
CKB can directly consume `project_graph.json` to get pre-computed metadata (complexity, language, high-level dependencies).
**Benefit**: Offloads computational burden from CKB, allowing it to focus on deeper semantic analysis and speeding up initial CKB indexing.

### 3.3. Augmenting CKB's Code Intelligence 🔄
Integrate Cartographer's compressed semantic skeleton directly into CKB's knowledge base as a lightweight, always-available overview layer for faster contextual lookups.
**Benefit**: Enhances CKB's ability to provide quick, high-level answers and navigate the codebase, potentially improving the performance of tools like `ckb_explore` and `ckb_understand`.

### 3.4. "Golden Source" for Public API Surface 🔄
Position Project Cartographer as the "golden source" for public API surface definitions and module dependencies, with CKB deferring to its output for this information.
**Benefit**: Ensures a consistent and authoritative view of the codebase's external contracts and structure, reducing potential discrepancies between tools.

---

## New Files Created

| File | Description |
|------|-------------|
| `mapper-core/cmp/src/api.rs` | HTTP API service with graph querying, compression levels |
| `mapper-core/cmp/src/mcp.rs` | Model Context Protocol server implementation |
| `mapper-core/cmp/src/webhooks.rs` | Webhook notifications for graph updates |
| `docs/api/openapi.yaml` | OpenAPI 3.0 specification for HTTP API |