# Cartographer — Roadmap

Opened 2026-09-22. Two sources: a read-only audit of the `lip://` URIs this crate emits,
checked against the LIP daemon that consumes them (`~/Work/Protocols/LIP`, `develop` at
`932dd9d`), and the structural findings of the 2026-09-21 survey. Items are marked
**[checked]** where the evidence was re-verified by hand, **[survey]** where it comes from
the earlier read and has not been re-verified.

Existing planning documents stay where they are: `docs/plan.md` for direction,
`docs/gaps.md` for known holes. This file is the short list of work that follows from the
audit.

---

## 1. `lip_uri` exists twice, and the two differ **[checked]**

Two independent implementations build the same URI:

```rust
// src/extractor.rs:277
let p = path.to_string_lossy().replace('\\', "/");
let p = p.trim_start_matches("./").trim_start_matches('/');
format!("lip://local/{}#{}", p, qualified)

// src/mapper.rs:129
let norm = path.trim_start_matches("./").trim_start_matches('/');
format!("lip://local/{}#{}", norm, qualified)
```

`mapper.rs` does not normalise backslashes. On Windows, the two call sites
(`extractor.rs:298` and `mapper.rs:101`, both filling `ckb_id`) produce different URIs for
the same symbol — one with `/`, one with `\`. Consumers match these strings exactly, so
the mismatch is a silent empty result, not an error.

**To do.** One function, one module, both call sites on it.

## 2. Verify the URI form against LIP's canonical form **[checked, conclusion open]**

Cartographer always emits the relative form, `lip://local/<relative-path>#<qualified>`.
LIP's own SCIP importer produces `lip://local//<abs>/<rel>` when the SCIP metadata carries
a `project_root`, and `lip://local/<rel>` only when it does not
(`LIP/tools/lip-cli/src/cmd/import.rs:30-35`) — the comment there records that the
pre-2.3.1 form "silently mismatched CKB queries", so this class of bug has already
happened once in this pipeline.

Whether Cartographer's output matches what the daemon stores for the same file is **not
established** — it depends on how the file entered LIP's index. It is also not covered by
a test on either side.

**To do.** Take ten symbols from a repo indexed both ways and compare the two URI strings
directly. If they differ, the fix is a form decision, not a code decision, and it belongs
upstream in `LIP/spec/symbol-uri.md` — which currently gives the grammar but no
normalisation rules (no statement on absolute vs relative, case, or escaping).

## 3. We produce identifiers for a protocol we do not speak **[checked]**

There is no socket code in this crate — no `UnixStream`, no `lip.sock`, no `LIP_SOCKET`
anywhere under `mapper-core/`. Cartographer writes `lip://` IDs into `ckb_id` and stops;
CKB and NyxCode are the ones that open a connection. The MCP tool docs say as much
("`lip_uris` so CKB can …", `src/mcp.rs:513`).

That is a legitimate design — emitting stable IDs for someone else to resolve — but it is
currently undocumented as a contract. Nothing in this repo states which LIP version the
URI form targets, and nothing fails if it drifts.

**To do.** One paragraph in `docs/architecture.md` naming the form, the consumer, and the
upstream document it follows. Then §2's comparison becomes a repeatable check rather than
an investigation.

## 4. The six core tools have no tests **[survey]**

202 tests in `src/`, six of them in `mcp.rs` — none covering the six core MCP tools that
`explore`-style callers actually use. `answer_question` is labelled EXPERIMENTAL and was
noisy in the single sample taken. The count and the file sizes are re-verified
(`main.rs` 6 174 lines, `mcp.rs` 3 971, `api.rs` 3 416, v4.0.2); which tests cover what is
from the survey and has not been re-derived.

**To do.** Before anything is built on these tools: twenty real questions as an eval, not
one.

## 5. Library and server are not cleanly separated **[survey]**

`main.rs` and `mcp.rs` carry logic of their own rather than delegating to a library
surface, so there is no clean boundary between "the graph and its queries" and "the
process that serves them". MCP is stdio-only, which means one process per session and one
graph per process in RAM.

**To do.** Nothing here on its own — this is the shape any embedding of Cartographer as a
library would have to deal with first, and it is worth knowing before that decision, not
after.
