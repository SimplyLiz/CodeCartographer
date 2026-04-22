## [3.0.1] - 2026-04-20

### Security Fixes

- **CRITICAL**: Fixed FFI double-free vulnerability. `cartographer_free_string()` now detects and rejects double-free attempts, returning error code `-1`.
- **CRITICAL**: Fixed path traversal vulnerability in FFI. All paths must now be absolute and cannot contain `..` components.
- **CRITICAL**: Added memory limits to graph analysis to prevent DoS. Projects with >100K files, >1M edges, or >1K edges per file are rejected.
- **HIGH**: Added input validation for git commit references to prevent shell injection.
- **HIGH**: Implemented atomic file writes with proper permissions for cache files (Unix: `0o600`).
- **HIGH**: Added ReDoS (Regular Expression Denial of Service) prevention. Regex patterns are size-limited and have per-file 5-second timeout.
- **MEDIUM**: Added bounds to skeleton map output to prevent unbounded memory allocation (max 200K tokens).
- **MEDIUM**: Added audit logging for all FFI calls and validation failures.

### New Features

- `cartographer_ffi_version()` — Returns FFI ABI version for compatibility checking.
- `cartographer_free_string()` now returns `i32` status code instead of void.
- Added `SECURITY.md` with vulnerability reporting policy and security guidelines.

### Breaking Changes

- `cartographer_free_string()` signature changed: now returns `i32` (0 = success, -1 = double-free, -2 = invalid).
- FFI now rejects relative paths and paths with `..` components.
- Git commit references are now validated; malformed refs will be rejected.

### Dependencies

- Pinned all transitive dependencies to specific versions for reproducible builds.
- Added `Cargo.lock` to version control.

---

## [3.0.0] - 2026-04-15

[... previous release notes ...]
