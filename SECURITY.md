# Security Policy

## Reporting Security Vulnerabilities

If you discover a security vulnerability in Cartographer, please **DO NOT** open a public GitHub issue.

Instead, email your report to: **security@nyxcore-systems.io**

Include:
- Description of the vulnerability
- Steps to reproduce (if possible)
- Potential impact (who could exploit this and what could they do?)
- Suggested fix (if you have one)
- Your name and affiliation (optional)

We aim to:
- Acknowledge receipt within 24 hours
- Provide an initial assessment within 48 hours
- Release a fix within 5 business days (if severity is critical)
- Coordinate public disclosure timing with you

## Supported Versions

| Version | Status | Security Updates Until |
|---------|--------|------------------------|
| 3.x | Supported | 2027-12-31 |
| 2.x | End-of-life | 2024-12-31 |
| 1.x | End-of-life | 2023-12-31 |

## Security Considerations

### FFI Boundary (C Interop)

Cartographer exposes a C FFI interface consumed by CKB and other tools. **Callers must follow these rules:**

#### Memory Management
- **Always call `cartographer_free_string(ptr)` exactly once per returned pointer.** Calling it twice (double-free) will be detected and logged.
- Do not manually free pointers — use `cartographer_free_string()`.
- Check the return value: `0` = success, `-1` = double-free detected, `-2` = invalid pointer.

#### Path Validation
- Only pass **absolute paths** to FFI functions. Relative paths are rejected.
- Paths containing `..` (parent directory references) are rejected to prevent path traversal attacks.
- Example: `/home/user/project` ✓ | `./project` ✗ | `/home/user/../../etc/passwd` ✗

#### Git Operations
- Commit references (`commit1`, `commit2`) are validated before use.
- Allowed formats:
  - Hex SHA: `abc123...` (40 or 64 chars)
  - `HEAD`, `HEAD~N`, `HEAD@{N}`
  - Branch/tag names: alphanumeric, `_`, `-`, `/`, `.`
- Rejected: Anything with shell metacharacters or `..`

### Resource Limits

To prevent denial-of-service attacks, Cartographer enforces limits:

| Resource | Limit | Rationale |
|----------|-------|-----------|
| Files per project | 100,000 | Prevents unbounded memory allocation |
| Edges per file | 1,000 | Caps import complexity per module |
| Total graph edges | 1,000,000 | Prevents graph algorithms from hanging |
| Regex pattern length | 2,000 chars | Prevents catastrophic backtracking (ReDoS) |
| Regex compilation size | 50 MB | Prevents DFA explosion |
| Per-file regex timeout | 5 seconds | Prevents infinite loops in backtracking |
| Git history limit | 5,000 commits | Prevents excessive git subprocess calls |

If your project exceeds these limits, Cartographer will return an error. Contact us if you need to adjust limits.

### File Permissions

Cache files (`.cartographer_cache.json`, `.cartographer_memory.json`) are created with **restricted permissions**:
- **Unix/Linux/macOS**: `0o600` (owner read/write only)
- **Windows**: Default system permissions (usually owner-readable only)

If you see warnings about world-readable cache files, run:
