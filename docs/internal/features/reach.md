# Feature: `reach` — semantic graph traversal for AI

## Problem

`search` and `find` return flat results — lines and paths. An AI receiving them must:

1. Classify each match (definition? call site? type annotation? test?) — costs tokens, is lossy
2. Re-derive the graph structure from the flat list — slow and error-prone
3. Decide what detail level is needed for each item — with no signal to go on

This is the wrong abstraction. Code is a graph. The thing an AI actually needs is: *"starting from symbol X, what matters within N hops, at what level of detail?"*

## Proposed feature

Two new primitives: `reach` and `answer`.

### `reach`

```bash
codecartographer reach SYMBOL [--depth N] [--budget TOKENS] [--file FILE]
```

MCP: `reach_symbol` tool.

Walks the call graph + import graph from a starting symbol and returns a **context tree** in compact AI-native format. Detail level is proportional to distance from the root.

**Parameters:**
- `SYMBOL` — function name, type name, or `File::symbol` qualified form
- `--depth N` — hop count (default: 2)
- `--budget TOKENS` — hard token cap; trims leaf nodes first (default: 6000)
- `--file FILE` — disambiguate when the symbol name appears in multiple files

**Output format (the bet):**

```
── verify_token  fn  auth.rs:112  pub
   sig  pub fn verify_token(token: &str, opts: VerifyOpts) -> Result<Claims>
   callers  3 prod · 1 test
     routes.rs:45      [handler]     let claims = verify_token(&token_str)?;
     middleware.rs:23  [middleware]  verify_token(bearer)
     api/v2.rs:91      [handler]     verify_token(req.token)?
     [1 test caller collapsed]
   callees
     decode_jwt       crypto.rs:8   fn decode_jwt(token: &str) -> Result<Payload>
     validate_claims  auth.rs:89    fn validate_claims(c: &Claims) -> bool
   depth-2  [sig only]
     Claims           types.rs:14   struct Claims { sub: UserId, exp: u64, … }
     RedisStore       store.rs:3    struct RedisStore { … }
```

### `answer`

```bash
codecartographer answer "QUESTION"
```

MCP: `answer_question` tool.

BM25 search → graph traversal → evidence-chain assembly. Returns the minimum set of semantic units that, together, answer the question, in reading order, with connections made explicit.

```
Evidence for: "how does rate limiting work?"

1  RateLimiter  struct  middleware/rate.rs:8
   struct RateLimiter { window: Duration, max_req: u32, store: Arc<RedisStore> }

2  RateLimiter::check  fn  middleware/rate.rs:31  [core logic, body shown]
   pub fn check(&self, key: &str) -> Result<(), RateLimitError> { … }

3  apply_rate_limit  fn  middleware/mod.rs:12  [calls #2]
   pub async fn apply_rate_limit(req: Request, limiter: &RateLimiter) -> Result<Response>

4  Router::new  fn  api/router.rs:45  [registers #3]
   .layer(middleware::from_fn_with_state(limiter, apply_rate_limit))
```

---

## Output format design

The format is the core bet. Design goals:

- **Minimal punctuation** — every `{`, `"`, `:` is a token
- **Semantic typing on every node** — `[handler]`, `[middleware]`, `[test]` — so the AI can weight relevance without reading the content
- **Distance-proportional compression** — root=full, d1=sig+call-context, d2=sig-only, d3=name-only
- **Honest truncation** — report what was dropped (`[1 test caller collapsed]`, `[expand with --depth 3]`), never silently omit
- **Position-aware** — most critical content first; matches how transformer attention works

### Token cost estimate (vs equivalent JSON)

| Format | verify_token full reach | Ratio |
|--------|------------------------|-------|
| JSON | ~480 tokens | 1× |
| XML (CodeCartographer current) | ~420 tokens | 0.87× |
| Reach compact | ~180 tokens | 0.37× |

Estimate based on encoding overhead. Will measure against actual tokenizer in experiments.

### Compression rules

| Node type | At depth 0 | At depth 1 | At depth 2 |
|-----------|-----------|-----------|-----------|
| Function | sig + body (up to 30 lines) | sig + one-line call ctx | name + file |
| Struct/type | all fields | field names only | name + file |
| Test caller | sig | collapsed (counted) | omitted |
| Trait impl | all methods | method names | name |
| Module | full skeleton | exports list | name |

Test callers are always collapsed by default. The `--include-tests` flag expands them.

---

## Implementation plan

### Phase 1: `reach` (call graph traversal)

**Building blocks that already exist:**

| Needed | Where it lives | Status |
|--------|---------------|--------|
| Call graph (fn → fn) | `call_graph.rs` | Rust + Python only |
| Import graph (file → file) | `api.rs` `ApiState` | All languages |
| Symbol extraction | `extractor.rs` `mapper.rs` | All languages, confidence-graded |
| BM25 search | `search.rs` | Done |
| Token budget trimming | `formatter.rs` | Done |

**What's missing:**
1. Symbol resolver — map a name to a `(file, line, kind)` tuple, handling ambiguity
2. Caller extraction — who calls a given symbol (currently only call graph goes forward)
3. Semantic type tagging — classify call sites as `[handler]`, `[middleware]`, `[test]`, etc.
4. The reach formatter — the compact tree output, with compression rules above

**Caller extraction approach options:**

*Option A — Text search + heuristic classification*
Grep for the symbol name, classify each match by file path and surrounding context. Fast, language-agnostic, imprecise (false positives on variable names that match).

*Option B — Invert the call graph*
`call_graph.rs` already builds fn→fn edges for Rust/Python. Build the reverse index at construction time. Precise for supported languages, no coverage on others.

*Option C — Import graph as proxy*
Use the file-level import graph to find candidate files, then do targeted symbol search inside each. Mid-precision, all languages.

**Recommended starting point:** Option A for the experiment (fast to build, lets us validate the format), with Option B as a follow-on for Rust/Python precision.

### Phase 2: `answer` (evidence chain)

Builds on `query_context` but changes the output shape. The BM25 + PageRank pipeline already exists; the new work is:

1. Rank results by "explanatory position" — entry points before internals, types before functions that use them
2. Annotate inter-item connections (`[calls #2]`, `[registers #3]`) — requires call graph
3. Decide which items get body vs signature — hotness + centrality heuristic
4. Format as the numbered evidence chain

Phase 2 depends on Phase 1 being stable first.

---

## Experiments to run

### Experiment 1: format comparison
Run `reach` on 3–5 representative symbols from this repo, produce the output in JSON, current CodeCartographer XML, and the proposed compact format. Count tokens with `tiktoken`. Target: compact format ≤ 40% of JSON for equivalent information.

### Experiment 2: caller precision
Compare Option A (text search) vs Option B (inverted call graph) on the Rust files in this repo. Measure: false positive rate (lines flagged as callers that aren't), false negative rate (actual callers missed). Hypothesis: Option A has ~10% false positive rate from substring matches; Option B is near-zero for in-file calls but misses cross-file calls not in the import graph.

### Experiment 3: depth calibration
For a symbol of average complexity (10–15 callers, 4–6 callees, 3 depth-2 neighbors), measure token cost at depth 1, 2, 3 with the compact format. Find the default depth where the budget stays under 4000 tokens for 80% of symbols.

### Experiment 4: end-to-end usefulness
Give Claude the output of `reach verify_token --depth 2` and ask it to answer: "Is it safe to add a new parameter to verify_token?" Compare quality and token usage vs giving it `focused_skeleton src/auth.rs --depth 1`. Measure: answer quality (subjective), tokens consumed.

---

## Open questions

1. **Symbol disambiguation UI** — if `verify_token` exists in 3 files, how do we present the choice? Print all candidates and require `--file`? Auto-pick the one with highest call count? Interactive prompt in CLI mode only?

2. **Language coverage** — call graph currently covers Rust + Python. For JS/TS/Go, we'd fall back to Option A (text search). Is that acceptable, or should we block `reach` on unsupported languages? Recommendation: allow with a `[heuristic callers — no call graph for ts]` warning rather than blocking.

3. **Body inclusion threshold** — at depth 0, when do we include the function body vs just the signature? Candidates: always, if body ≤ N lines, if `--detail extended`, if the symbol is the only depth-0 node. Experiment 3 should inform this.

4. **`answer` vs `query_context`** — `answer` is strictly more work than `query_context`. Is the evidence-chain format worth the complexity vs just improving `query_context`'s output format? Defer until Phase 1 is validated.
