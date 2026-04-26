# reach + answer — follow-up work

Shipped as experimental in `feat/reach`. These are the things worth coming back to if the feature gets traction.

---

## Known rough edges

### answer: tail noise from single-term private fn matches

Private functions in large files (e.g. `health_graph_at_ref` in `main.rs`) occasionally appear at position #6 because their name contains one query term ("graph") and `main.rs` is a BM25 hit for almost everything. The current gate (`name_score >= 3.0` for private fns) catches doc-only matches but not single-term name collisions.

**Fix:** require private fns to match at least two distinct query terms, or penalise fns from files with high import fan-in (main.rs, api.rs) unless those fns score on two terms. Alternatively, cap private fns at one slot per file.

### answer: companion implementations confuse ordering

"How does the call graph work" returns `build_class_graph` at #1 and `build_file_call_graph` at #3 because `class_graph.rs` and `call_graph.rs` score nearly identically. Both are correct answers, but the ordering surprises users who expected the original implementation first.

**Fix:** when two fns from different files score within 10% of each other and do the same conceptual job, prefer the older file (earlier creation date in git) as #1. Or detect that they share a conceptual prefix ("call_graph" vs "class_graph") and group them.

### callees empty for Go / TypeScript / JavaScript

The tree-sitter call graph in `call_graph.rs` only covers Rust and Python. For other languages, the callee section is empty with a note. Cross-file callers still work (text search is language-agnostic), but callee precision is worse.

**Fix:** extend `call_graph.rs` to cover Go (straightforward — tree-sitter-go is already a dependency) and TypeScript (harder — requires handling method chaining, closures, and arrow functions).

---

## Option B: inverted call graph for caller precision

Currently `reach` finds callers via regex text search (Option A). This gives 0% false positives on the test corpus but has ~10% false negative rate from the definition-line filter.

Option B: build a reverse call graph by running `build_file_call_graph` on every Rust/Python file at startup and inverting the edges. `get_callers(symbol)` would then return precise results from the AST rather than text matches.

**Cost:** O(n) tree-sitter parses at startup. On a 50-file Rust project: ~50 × 5ms = 250ms. Acceptable if cached per-session.

**Why deferred:** Option A has no observed false positives in practice, making the precision gain marginal. Worth revisiting if users report missed callers.

---

## Iterative refinement for answer

`answer` currently returns a fixed 6-item chain. A natural follow-on is letting the AI ask for more detail on a specific item:

```
navigator answer "how does rate limiting work?" --then 2
```

Which would call `reach` on item #2 and append its context tree below the original chain. This doesn't require new infrastructure — it's just `answer` + `reach` composed.

**MCP version:** `answer_question` returns item indices; client calls `reach_symbol` on the desired index.

---

## Multi-symbol reach

`reach` currently starts from one symbol. For code review or refactoring tasks you often want the intersection of two symbols' neighborhoods:

```
navigator reach "verify_token" "decode_jwt" --depth 1
```

Returns the union of both trees with shared items deduplicated and shared depth-2 types promoted to depth-1. Implementation: run `build_reach` twice, merge results, re-rank.

---

## answer quality metrics

We measured token efficiency (1–3% of focused_skeleton) but not answer quality in a controlled way. Experiment 4 was manual inspection on 4 queries.

A proper quality eval would:
1. Take 20 questions about the Navigator codebase with known correct answers
2. Score `answer` output on: correct item in top 3, noise items ≤ 1, connection annotations correct
3. Baseline against `query_context` on the same questions
4. Track as a regression suite when the scoring or ranking changes

Low priority until the feature is used in anger and we have real signal on where it fails.
