# Changelog

## 0.5.0

### A cached page can carry one derived encoding of itself

`CachedPage::encoded_or_init(f)`, plus `CachedPage::encoded()` to look without
computing, and `CachedPage::new()` because the struct gained a private field and
a literal outside this crate therefore stopped compiling. That private field is
an `Arc<OnceLock<Bytes>>` shared with the entry, so `f` runs **once per cached
page**, not once per hit, and a burst coalesces on it the way a burst on a cold
key already coalesces on the render.

**Why the cell belongs in the entry.** A caller can compress a body perfectly
well on its own; what it cannot do cheaply is decide when the compressed copy
has expired. The page it holds is a `Bytes` clone with no identity, so a twin
kept in the caller's own cache has to be keyed by something, and both choices
are bad:

* **by URL** — goes stale the instant the page is rebuilt, and serves the
  previous document with a correct status and a correct length, which nothing
  downstream can detect;
* **by a hash of the body** — costs a pass over the whole document on every
  request, spending a good part of what compressing once was supposed to save.

Inside the entry the question disappears: the derived form lives exactly as long
as the bytes it came from, and a rebuild replaces both together. A page that the
build marked `uncacheable` gets a cell of its own, so its encoding is made for
that response and dies with it.

**What it was written for.** The consumer of this crate serves a 450 kB
catalogue page and lets nginx compress it. Measured on its production box:
brotli cost **20.8 ms of CPU per request** at quality 5, on bytes this cache had
already rendered and was handing out unchanged — the single most expensive thing
in the whole stack, and paid again for every visitor of the same page. An
upstream response that already carries `Content-Encoding` is passed through by
nginx untouched, so moving the compression here turns per-request work into
per-entry work.

The crate compresses nothing itself and takes no compression dependency: `f` is
the caller's, and returning an empty `Bytes` is how it declines — cached like
any other answer, so a body that will not compress is not re-attempted forever.

## 0.4.0

### A build can now say "serve this, but do not keep it"

`BuiltPage::uncacheable()`, and a `cacheable: bool` on `BuiltPage` that every
constructor sets to `true`. `PageCache` honours it in `store_at`, which is the
one place all three storing paths go through — `store`, the single-flight
leader, and the background revalidation.

**Why it had to exist.** Before this a build had exactly two answers, and
neither one fits a page that is correct enough to send and not correct enough to
repeat:

* `Ok(page)` — served AND pinned for the length of the TTL.
* `Err(_)` — not pinned, but not served either, so the caller needs a second
  degraded answer ready and every visitor in that window gets it instead.

The consumer this crate was written for spent an evening on the gap between
them. Its catalogue page is rendered from a database query; the query timed out
**once**, under a burst of unrelated load on a one-core box; the render
succeeded anyway and produced a page with a header, a result count and an empty
grid. `get_or_build` stored it, because a build that returns `Ok` is by
definition a page worth keeping — and for the next five minutes every request
for that URL was answered from it. Reloading did not help. The database was
healthy the entire time. The only signal anything was wrong was the response
size: 48 kB where a good render is 422 kB.

The workaround was to return `Err` and let the handler fall through to a
client-only shell. That works, and it throws away the server render for a case
where the server render is perfectly good — the page is missing one section, and
the client fills that section in on its own.

```rust
use rusty_ssr::cache::BuiltPage;

let page = match rows_from_the_database() {
    Some(rows) => BuiltPage::ok(render(&rows)),
    // Render it — the client can fill the gap — but do not let the next
    // visitor inherit this one's bad luck.
    None       => BuiltPage::ok(render(&[])).uncacheable(),
};
```

**The stale path is the one worth reading twice.** A stale hit is answered from
the old copy while a rebuild runs behind it, and that rebuild holds a `refreshing`
claim so a burst of stale hits starts exactly one of them. An uncacheable
rebuild replaces nothing, so it must release that claim — otherwise the key
stays marked until eviction and never revalidates again, which is a worse
failure than the one this feature exists to prevent. It also must NOT evict the
stale entry: that copy is a previous good answer, and declining to replace it is
not a reason to throw it away.

Six tests cover it, including the burst case (one build, eight waiters, nothing
kept) and the revalidation case above. Each was checked by breaking the code it
covers: removing the `cacheable` branch in `store_at` fails four of them, and
removing the claim release fails exactly the one that describes it.

### Breaking

`BuiltPage` has a fourth public field, so a struct literal
`BuiltPage { status, body, headers }` no longer compiles. Every constructor —
`new`, `ok`, `redirect`, `From<(u16, String)>` — is unchanged and fills it in,
so callers that build pages the normal way need no edit. This is why the bump is
0.4.0 rather than 0.3.5.

## 0.3.4

### The prelude's `atob` was the most expensive frame in a real render

Not in the consumer's application — in this crate. `BROWSER_POLYFILLS` resolved
each base64 character with `__RUSTY_B64.indexOf(s.charAt(i))`: a one-character
string allocation and a scan of up to 64 characters, **per byte of input**, with
the result accumulated as one single-character string per output byte and joined
at the end.

A CPU profile of the consumer this crate was written for — an SSR bundle whose
server hands the page a 21 kB base64 protobuf seed on every request — put that
function at the top of the list at **21.9% of sampled time**, ahead of every
frame in the application, ahead of Preact's renderer, ahead of the garbage
collector. It decoded at 9 MB/s.

By a 256-entry lookup table, with the output stitched from 4 kB runs, the same
payload decodes at **308 MB/s — 34×**. End to end that is **1.44× on the whole
page render**, and the rendered document is identical byte for byte.

Against the real server — two binaries from identical application source
differing only in which version of this crate they link, alternating rounds on
pinned cores — it is **1.09× on the whole request** for a route that carries a
26 kB seed: 247 → 270 pages/s, with B ahead in every round. The gap between
1.44× and 1.09× is not disagreement: 8 workers at 247/s is 32.4 ms per request
and at 270/s is 29.6 ms, so **2.8 ms was saved against the 2.9 ms the isolated
decode predicted for a payload that size.** The render is simply a smaller part
of a real request than it is of a render benchmark.

The same run on a route with no seed, where this function is never called, moved
1.002× — which is what makes the seeded number a measurement rather than
drift. Ask for that control before believing any figure in this file: an
earlier attempt at the same comparison reported a 6% "win" on the control and a
46% swing on the page-cache path, because something else on the machine was
using the CPU.

Nothing here could have found it. The crate's own benchmarks render bundles with
no base64 in them, so the polyfill never ran; `BENCHMARK.md`'s figures are all
from pages that never called it. It took profiling a real application's real
payload, which is the general lesson rather than this specific function.

The contract is unchanged, deliberately, down to the error text: whitespace
(`\t\n\f\r` and space) stripped wherever it appears, trailing `=` stripped,
`length % 4 === 1` throwing `atob: invalid base64 length`, anything else
throwing `atob: invalid base64`.

Five tests in `tests/bytes_payload.rs`, of which two cover boundaries the old
implementation did not have:

- **a code point past the table.** An out-of-range read on a typed array is
  `undefined`, and `undefined < 0` is false — so the obvious spelling accepts
  `Ā` as a base64 digit and invents bytes. The `c < 256` guard is why it does
  not.
- **a payload longer than one output run**, which has to stitch. Everything the
  crate decoded before this was a few bytes long and never reached it; the test
  round-trips 20,000 bytes covering every value including NUL.

`btoa` next door has the same per-character shape and is left alone: nothing on
a render path calls it, and a change nobody can measure is a change nobody can
justify.

## 0.3.3

### The pool can now be asked how close it is to capacity

`cache_metrics()` reported eleven numbers about the cache. The pool reported
`worker_count()`, which returns a constant. So the one operational question
about this component — *am I about to fall over?* — had no answer, and
`pool_size` was chosen by guessing.

It matters more here than the cache metrics do, because the pool is the ceiling.
One worker renders one page at a time, so a pool serves at most
`workers / render_time` requests per second; throughput stops rising the moment
every worker is busy, and every further caller becomes queue delay. Measured on
a real Preact bundle rendering a 60 kB page: throughput flattens at
concurrency ≈ `pool_size` and stays flat, while p50 goes 1.7 ms → 4.0 → 7.5 →
14.9 ms as concurrency doubles past it. Nothing in the engine said so.

**`SsrEngine::pool_metrics() -> PoolMetrics`**, with the pair that distinguishes
*busy* from *losing*:

- `saturation` — `busy / workers`. At 100 the pool is the bottleneck, full stop.
- `queue_pressure` — `queued / queue_capacity`. Rising while saturation sits at
  100 is the shape of a queue that will not drain.

Below saturation the load test now prints `pool 50% busy (8/16), q=0`; past it,
`pool 100% busy (16/16), q=40`. That is the whole diagnosis, in two numbers.

Also `renders`, `failed` and `timeouts` — a timeout being the one outcome that
never becomes a render and is therefore invisible in every other count — and
`render_p50/p95/p99` from a histogram of four buckets per octave, which is what
`pool_size` should actually be chosen against.

**It costs nothing measurable.** Two clock reads and a few relaxed atomics per
render: 8070/7949/8089 req/s instrumented against 7331–8396 before, on the same
machine. That is deliberately the opposite of the decision taken for the
fragment cache, where the clock came *off* the hot path — a cache lookup costs
tens of nanoseconds and timing it doubled the work, while a render costs at
least tens of microseconds. Cost is relative to the thing being measured, and
the histogram never allocates.

Sixteen tests: eight in `tests/pool_metrics.rs` driving the gauges to their
extremes (saturation reaching exactly 100 with every worker held, queued
requests visible while they wait and draining to nothing, a throw counted as a
failed render rather than a timeout, a request that never got a worker counted
as a timeout), and eight on the histogram arithmetic. Those eight caught a real
bug on their first run: the exact sub-8 µs buckets collided with the octave
scheme, so 8 µs and 4 µs shared a bucket and every percentile above it was
quietly wrong.

### Building an engine now means the bundle loads

A bundle with a **syntax error** used to produce a successfully built
`SsrEngine`. Each worker tried to load it, failed, logged to `tracing::error!`,
and exited. The pool was then left with zero workers, `build_engine()` returned
`Ok`, and every request that followed waited out the whole `request_timeout` —
thirty seconds by default — before failing with `Render timeout`. The one fact
that explained all of it went to a log nobody sees without a subscriber
installed, and the error the caller got pointed at the render.

In production that is: ship a bad build, the service comes up healthy, accepts
traffic, and every request hangs for half a minute and then blames the wrong
thing. `rusty-ssr-check` — the tool whose whole purpose is to catch this before
a deploy — reported **`ok bundle loads`**.

- **`V8Pool::new` returns `Result<Self, String>`** and waits for every worker to
  report the outcome of loading the bundle. `SsrEngine::new` surfaces a failure
  as `SsrError::V8Init`, carrying V8's own message and source position.
  *Breaking* for anyone constructing a `V8Pool` directly; `SsrEngine` callers
  see only a build that now fails when it should.
- **A syntax error is caught in 75 ms instead of 30 s**, and the message is
  `Uncaught SyntaxError: Unexpected identifier 'error' at <ssr-bundle>:520:43`
  rather than `Render timeout`.
- **The first request no longer pays for start-up.** Waiting for the workers
  moves isolate creation and bundle compilation into `build_engine()`, where it
  belongs: about 70 ms for a 19 kB bundle and 100 ms for a 1.7 MB one, per
  isolate, in parallel across the pool.

`tests/bundle_loading.rs` covers it: a syntax error and a top-level throw both
fail the build with V8's message intact, the failure arrives in under five
seconds rather than as a per-request timeout, sixteen workers report one failure
without turning it into a long wait, and a good bundle is ready to render on the
first call. A missing render function stays a *render* error, which is right —
the bundle loaded, the contract was not met.

Found by pointing the load test at a deliberately malformed 1.7 MB bundle. No
unit test would have: the engine reported success, and the failure only appeared
as a wait.

## 0.3.2

### The fragment cache could grow without limit, and now cannot

A `cache_size(300)` cache was found holding **twenty million entries** — every
byte of every page it had ever rendered — under a stream of unique URLs. Any
traffic that does not repeat itself reaches it: a crawler, cache-busting query
strings, `?utm_*` links, or anyone who notices. The process grows until it is
killed.

Eviction scans the whole map to find the oldest entries, and a per-scan cap
limited what one scan could then remove to 25% of capacity — 75 entries for the
default 300. The cap was documented as bounding the work of a scan. It does not:
the scan is O(n) whether 75 entries are removed afterwards or 75,000, so the cap
bounded only the result. As soon as the inserts arriving during one pass
outnumbered the cap, each pass ended further behind than it began; the map grew,
the next pass took longer, and the gap widened on its own. There is no rate at
which it recovers.

It survived 0.3.0 only because the same `insert` also called `DashMap::len()`,
summing 128 shards on every single insert. That was slow enough to hold the
insert rate below the cliff — so removing it, as an unrelated optimisation two
sections below, is what made a live bug visible. It was reachable before.

Two changes:

- **A scan now removes the entire overshoot**, not a fixed slice of it. The
  cache settles at `target + (inserts arriving during one pass)` — a fixed
  point, rather than a quantity that only grows.
- **Far above capacity, nothing is ranked.** Ranking picks which few entries to
  lose; when the cache holds millions against a cap of hundreds there is nothing
  to pick, and paying to rank it is what turned "behind" into "hopelessly
  behind" — a max-heap of eleven million entries took **sixteen seconds** to
  build, during which thirteen million more arrived. Above one capacity's worth
  of overshoot the pass instead drops by age against the insert clock, in a
  single sweep with no extra memory. The heap remains for the ordinary
  near-capacity case, where it is small and exact.

Under sixteen threads rendering nothing but unique URLs for thirty seconds, a
300-entry cache now holds 291 entries and evicts once per insert. Throughput
went from 542,000 to 957,000 renders per second on the same machine, and the
worst request went from sixteen seconds to seven milliseconds.

`size_stays_within_capacity_under_concurrent_inserts` in
`tests/cache_semantics.rs` is the regression test, and it is worth saying why
the sixteen tests beside it did not catch this: they drive the cache from one
thread. Eviction has a guard admitting one thread at a time, so a
single-threaded test exercises the one case where that guard never turns anybody
away. The bug lives entirely in the case where it does.

`examples/loadtest.rs` is what found it, which none of the unit tests could
have — the failure needs sustained concurrent pressure and only shows up in an
aggregate nobody asserts on. It grew two scenarios (`--hit-ratio 0` for the V8
path, `--hit-ratio 100` for the cache) and lost two flaws of its own: it pushed
every latency through one shared mutex, which serialises the drivers and hid
exactly this, and it deep-copied a 500,000-entry URL vector into each of 32
tasks — sixteen million strings allocated before the first request, never read.

## 0.3.1

### The render function may be async, and now it says so

Nothing changed in the engine: `render_html` has always driven whatever the
render function returned to completion, and every example bundle and
integration test in this repository declares `renderPage` as `async`. What
changed is that the fact was stated only in the README, and a consumer reading
the *signature* — `render_uncached(&self, url, data) -> SsrResult<String>` — has
no way to tell.

That gap is expensive, because sync-versus-async is really the suspense
question. A synchronous renderer throws whenever a component suspends, and a
code-split route awaiting its chunk is the ordinary case. A bundle that believes
the contract is synchronous therefore has to swap every lazy route for a
placeholder, and then serves crawlers an empty body under a correct-looking
`<title>` — no error, no log, nothing that fails. It was found in a consumer
whose most SEO-valuable page type, the one its own sitemap lists, had been
shipping an empty `<div>` on the strength of one wrong code comment.

- **`SsrEngine::render` grew a "render-function contract" section** covering all
  three shapes, what a rejected promise does (an `Err`, exactly like a
  synchronous throw), and the cost — the worker thread is occupied for the whole
  await, so awaiting real I/O saturates a `pool_size` pool far sooner than
  awaiting a microtask. `render_uncached` and `render_function` link to it.
- **`SsrEngine::render_fn_shape() -> RenderFnShape`** — `Sync`, `Async`, or
  `Unknown` before anything has rendered. Observed at the one point in the
  process where the answer exists (the returned V8 value, before it is
  resolved), and deliberately not something to branch on: both shapes render
  correctly and the engine treats them identically. It exists to be reported.
- **`rusty-ssr-check` reports it.** Async passes with a note; sync prints what a
  synchronous render function costs a code-splitting bundle. Never a failure —
  a bundle that splits nothing has no reason to be async.

Five tests cover it, including a promise resolved from a `setTimeout` (so the
event loop is genuinely driven, not merely microtasks drained) and a rejection
arriving as an `Err` rather than as a blank page.

### Payload delivery, and a prototype-pollution hole closed with it

A JSON payload used to reach the bundle by being parsed twice: `serde_json`
built a `serde_json::Value` — a Rust tree nobody ever read, with an allocation
per string, array and object — and `serde_v8` then walked that tree to build the
V8 objects the bundle actually receives. It goes to `v8::json::parse` now, which
does it in one pass inside the isolate and validates while it is there.

The second parse was not only slow, it was **the wrong way to build objects from
untrusted input**. Assigning an object's keys one at a time means `__proto__`
goes through the prototype setter, so a payload containing
`{"__proto__": {"x": 1}}` wrote to `Object.prototype` — for every later render
on that worker, for every visitor it served. `JSON.parse` semantics, which V8's
parser implements, make `__proto__` an ordinary own property. Payloads are
attacker-influenced on any page that echoes user input, so this was reachable.

- **A 60 kB envelope is delivered about three times faster** — 966 µs → 300 µs
  end to end, render included, reproduced within three points across three
  independent runs. Small payloads are unchanged, as expected: there the
  measurement is cross-thread dispatch, not conversion.
- `tests/render_payload_fidelity.rs` — 19 tests pinning what crosses into the
  bundle: every JSON type keeps its JavaScript type, numbers keep their values,
  non-ASCII and escapes survive, deep nesting survives, a 400-row payload
  arrives complete, malformed input is refused with a message that says so, a
  worker still renders after refusing one, and `__proto__`/`constructor` keys
  are data rather than reaching the prototype.

### The fragment cache stopped paying for its own instrumentation

- **The clock came off the hit path.** Every `try_get` called `Instant::now()`
  to fill `CacheMetrics::last_access_ns` — tens of nanoseconds to time a lookup
  documented as taking one to three, and on a miss the reading was taken and
  then thrown away. One lookup in 256 is timed now. The field is a latency
  gauge, and a sample says what the population does.
- **`insert` stopped counting the whole map.** It called `DashMap::len()` — a
  sum over all 128 shards — on every insert, purely to ask whether it was full.
  The count is maintained alongside the map instead.
- **Reads stopped writing to a shared counter.** The LRU stamp came from a
  `fetch_add` on one global atomic, so every read from any core bounced that
  cache line — on the read path, in a read-mostly structure, which is precisely
  what 128 shards were there to avoid. Readers *load* a clock that only inserts
  advance. Eviction is therefore "least recently used, to the nearest insert",
  which is the granularity at which the answer is used anyway.
- **Two `Arc`s that nothing ever cloned** became plain fields, and
  `HotCache::get`/`peek` stopped keeping two copies of the same two-tier search.

Every lookup and insert measured is faster, in every run, by margins between a
third and two thirds — a hot hit goes from 139 ns to somewhere between 42 and
69 ns depending on the run, an insert from 1.2 µs to 0.4–0.5 µs, and concurrent
readers gain 19–49% at one, four and eight threads. The spread is the rig, not
the change: see the note on the benchmark below.

`tests/cache_semantics.rs` — 16 tests stating what the cache promises without
reference to its tiers, including that its reported size never drifts from
reality (overwrites, invalidations, prefix invalidations, clears, concurrent
inserts of the same keys) and that a working set which is read survives churn.

### The page cache hit stopped rebuilding its own key

`RenderKey` was composed into its string three or four times per request — once
to look up, once to claim a refresh, once for single-flight, once to store —
sorting the variants and allocating afresh each time to produce the identical
string. It is composed once now, and borrowed rather than built at all when the
key has no variants.

Strictly less work per request — three or four fewer sorts and allocations, and
none at all for a key without variants — but this tier's benchmarks sit at a few
hundred nanoseconds, which is below what this machine can resolve. No figure is
quoted because none survived the noise check.

### The pool hands out work without a lock convoy

Workers shared one `std::sync::mpsc::Receiver` behind a `Mutex`, and called the
*blocking* `recv()` while holding it. Only one worker was ever really waiting;
the rest were queued on the lock, so every task handed out cost a mutex
hand-off and a wake-up chain, and the workers took their turns in
lock-acquisition order rather than whichever was free. The queue is a `VecDeque`
with a condvar now: the lock is held for a push or a pop, never across a wait.

Backpressure changed with it. A full queue used to spin on `try_send` +
`yield_now()` until the request's deadline — a runtime thread at full tilt for
the whole timeout, burning the CPU the workers needed to drain the queue it was
waiting on. Callers wait on a semaphore permit and are woken by the worker that
frees the slot. A worker releases its slot when it *takes* the task, so
`queue_capacity` still bounds the queue and not the concurrency.

Also: the render-function name is no longer a `String` cloned into every request
(it is fixed for the life of the pool, and read once per worker, since the
resolved handle is then cached on the isolate); `worker_count` is an atomic
rather than a mutex; core pinning is keyed on the worker's own index rather than
a counter the workers race to increment; the per-render `Vec` of V8 arguments is
gone; and `prefetch_data` — which prefetched one cache line before a
millisecond-long render, and whose `cfg` named `core::arch::x86_64` on 32-bit
x86 — is gone with it.

64 concurrent renders came out 5–10% faster in both clean runs, through one
worker and through four. That is inside this machine's noise floor, so treat it
as a direction rather than a number; what is certain is that the work removed —
a mutex hand-off per task, and a spinning thread under backpressure — is work
that is no longer done.

`tests/pool_queue.rs` — 9 tests: no request is answered with another request's
page across 400 concurrent callers, every request is delivered exactly once, a
one-slot queue does not serialise four workers, backpressure resolves on
schedule and lifts afterwards, and a throwing render leaves its worker usable.

### Template assembly walks the document once per placeholder, not once per pair

The single-pass scan re-searched *every* placeholder across the rest of the
document after *every* substitution — k·m passes over the whole page for k
placeholders and m matches. Each placeholder's next position is now found once
and re-derived only when the cursor passes it; "no more occurrences" is
remembered rather than re-discovered.

Measured within a single run, so machine drift cannot flatter it: assembling a
600 kB document with five placeholders used to cost 5.9× what one placeholder
cost, and now costs 3.3×. In wall-clock terms, 700 µs → 390 µs, while the
one-placeholder case stays at 119 µs in both — which is the arithmetic working
out, since with one placeholder there was never a second pass to save.

`tests/template_assembly.rs` — 14 tests, including the property that makes
single-pass assembly worth having: a rendered fragment containing the literal
text of another placeholder is emitted verbatim. Chained `String::replace` gets
that wrong, and on any page that echoes user input the fragment is
attacker-influenced.

### Brotli middleware

- **Compression moved off the runtime thread.** It ran inline, so for the whole
  of a large page it blocked every other task that thread was driving — on a
  single-threaded runtime, the accept loop included.
- **A response that is already encoded is left alone.** Sitting above a
  `tower-http` compression layer, this produced a body that had been through
  brotli twice under a header claiming once, which no client can undo.
- **A blocking `Path::exists()` came out of an async function.** It was also
  redundant: the read that follows answers the same question in one syscall,
  without a window in between for the answer to change.
- **Buffering is bounded** at 32 MiB rather than `usize::MAX`.
- The two doctests in this module never compiled. They do now.

Eleven tests, where there were none.

### Documentation that had drifted from the code

- Four places said the prelude ships no `atob`/`btoa`. It has shipped both since
  0.3.0.
- `compose()` described "the `init_bundle*` functions above", which 0.3.0
  removed.
- `seal_globals` opened with four lines documenting `max_heap_mb`.

### Tests

`tests/common/mod.rs` — the tempdir-plus-bundle-plus-engine preamble was written
out about thirty times across eight files. It holds the temporary directory
alongside the engine, which two of those copies had been dropping early and
getting away with only because the bundle happens to be read once, at build
time.

`benches/hotpath_benchmark.rs` — the paths a request actually walks, measured
through the public API. Every figure quoted above comes from it; the module docs
carry the before/after recipe.

**On trusting its numbers.** Run the same unchanged code twice and criterion
will report a 32% improvement, with `p = 0.00`, because a developer laptop's
clock speed depends on how warm it is. Anything under about a third is therefore
a statement about the machine and not about the code, which is why some sections
above quote a range, and some quote nothing at all. The figures that are quoted
either clear that bar by a wide margin and reproduce across independent runs, or
come from comparing two benchmarks inside a single run, where the drift cancels.

## 0.3.0

### Request isolation, as far as this stack allows

0.2 gave the pooled isolate a request boundary through `onSsrRequest`, which
only helps a bundle that knows to define it — and the code that actually leaks
is usually a dependency that does not. `.seal_globals(true)` closes that half
without asking the bundle for anything: the engine records the own property
names of `globalThis` once the bundle has loaded, and every render begins by
deleting whatever was added since.

Off by default, because "delete every global you did not have at startup" is
right for correctness and wrong for a bundle that caches across renders on
purpose — a compiled-template cache, a warmed lookup table. It does not reach
state held in module closures (nothing outside the bundle can), so it composes
with `onSsrRequest` rather than replacing it.

**A fresh V8 context per request was investigated and is not reachable through
this dependency stack.** The cheap way to do it is a context snapshot, and
`v8::SnapshotCreator::add_context` / `set_default_context` are `pub(crate)` in
the `v8` crate, while `deno_core`'s `JsRuntime` always enters its main context.
The alternatives are forking `rusty_v8`, or dropping `deno_core` and driving
raw V8 — a rewrite of the runtime layer, giving up module loading and promise
resolution, for a benefit that sealing plus `onSsrRequest` largely already
deliver. If that changes upstream, this is the note to revisit.

### `location` comes from the render URL

The engine is the only thing that knows the URL, and a router reading
`location.pathname` is how most applications decide what to render — so every
consumer was writing that assignment by hand before calling the bundle. Missing
it is silent: the router sees `/`, every URL renders the home page, and it
reads as a bug in the application rather than a gap in the harness.

The prelude now sets `pathname`, `search`, `hash` and `href` per request, plus
`origin`/`protocol`/`host`/`port` when the URL is absolute. Identity-checked, so
a bundle that installed its own `location` keeps it. `onSsrRequest` receives the
URL too.

### An empty render can be a failure

`.min_render_bytes(n)` turns a render shorter than `n` (trimmed) into
`SsrError::EmptyRender`. This is the SSR failure that does not announce itself:
every real bundle wraps its render in a `try/catch`, the catch returns `""`, and
the caller serves a blank page under a 200 with nothing anywhere saying so. With
a floor set the caller gets an `Err` it can fall back from, and the blank result
is not cached.

### `rusty-ssr-check`

A binary that runs a bundle in the real engine and reports what happens.

```text
rusty-ssr-check dist/ssr-bundle.js --url / --url /products/42 --min-bytes 200
rusty-ssr-check --dump-prelude > prelude.js
```

It checks that the bundle loads, that each URL renders and how many bytes it
produced, and — by rendering one URL, then another, then the first again —
whether the bundle carries state between requests. Exit code 0 or 1, so it
belongs in a deploy script between building the bundle and shipping it.

Every project otherwise writes some version of this as a Node `vm` sandbox with
the prelude copied into it by hand, and that copy drifts: a drifted probe stays
green while production serves blank pages.

### `atob`, `btoa`, `screen`, `devicePixelRatio`

Web APIs bare V8 does not have. A bundle that reaches a missing `atob` throws,
and on this path a throw is an empty page rather than an error anyone sees. For
payloads prefer `render_with_bytes` — there is nothing to decode at all.

### axum is no longer a default feature

`default = ["v8-pool", "cache"]`. Every consumer used to compile axum + tower +
tower-http whether or not they touched the single middleware behind that flag,
and got a *second* axum in their dependency graph as soon as their own version
moved past ours. An SSR engine has no business pinning anyone's web framework.
Opt in with `features = ["axum-integration"]`.

### The example no longer teaches two bugs

`examples/build-preact-bundle.js` wrote `window.__INITIAL_DATA__ =
${JSON.stringify(...)}` unescaped — a `</script>` anywhere in the data ends the
tag and everything after it becomes markup — and caught render errors to return
a page containing the stack trace, which the engine cannot distinguish from a
successful render and therefore caches and serves. Both are gone: the data goes
into a `type="application/json"` tag with `<` escaped, and the render is not
wrapped in a catch, with a comment explaining why that is deliberate.

### Breaking changes

- `default` no longer includes `axum-integration`.
- `V8PoolConfig` gained a required `seal_globals` field.
- `v8_pool::runtime::init_runtime` takes a `seal_globals` argument.
- `SsrError` gained an `EmptyRender` variant.

## 0.2.0

Everything here came out of running 0.1 in production for a season. The theme
is that an SSR engine's job does not end at "V8 returned a string" — the caller
still has a response to assemble, a key to decide, and an isolate full of the
last request's state to worry about.

### The page cache (`rusty_ssr::cache::page`)

A second cache tier, above the existing fragment cache, holding **finished
documents**. `SsrEngine::page(&key, build)` is the new entry point.

The fragment cache is keyed on `url + data` and stores what the render
returned. Three things go wrong when an application tries to use that as its
page cache:

1. **The fragment is not the page.** `<head>` tags from a database, a serialised
   store for the client to hydrate from, a status code — all of it is added
   after the render, and a hit redoes all of it.
2. **The URL is not the key.** One path under two hostnames is two documents the
   moment a canonical URL or `og:url` is built from the Host header. Locale,
   device class and currency do the same.
3. **`data` is the wrong thing to put in a key.** It is the payload. Hashing tens
   of kilobytes on every lookup, to store an entry no second request will hit,
   is not caching.

So the caller says what the key is, and the cache handles what is tedious:

- **`RenderKey`** — the URL plus caller-declared *variants* (`host`, `locale`,
  …), order-independent. `variant_digest` keys on a payload without storing it.
- **`CachePolicy`** — `Off` / `ttl(capacity, d)` / `forever(capacity)`, plus
  `stale_while_revalidate(window)`. Replaces two settings that both lied:
  `cache_ttl_secs(0)` meant "never expires" rather than "do not cache", and
  `cache_size(0)` was refused outright, so "off" could not be said at all.
- **Single-flight** — forty concurrent requests for one cold key run one build.
- **Stale-while-revalidate** — an expired page is answered immediately while its
  replacement is built behind the visitor.
- **`BuiltPage` / `CachedPage`** carry status *and* headers, so a redirect is
  cacheable like anything else, and bodies are `Bytes` — a hit is a refcount
  bump, not a copy of the document.

### A request boundary in the pooled isolate

`globalThis` outlives a render, and the prelude's `localStorage` is *real*
in-memory storage. Without a boundary, request B starts inside request A's
leftovers — a correctness problem for any module-level cache in the bundle, and
a privacy one, because A and B are different people.

The prelude now installs `__rustySsrReset()`, called before every render. It
re-creates the storage objects it owns (tracked by identity, so a bundle that
installs its own storage later keeps it) and then calls
**`globalThis.onSsrRequest`** if the bundle defines one — the hook for a bundle
to clear its own state. A throw there fails the render rather than being
swallowed: a request that could not be isolated must not be served.

### Binary payloads

- `SsrEngine::render_with_bytes(url, Vec<u8>)` — the payload arrives as a
  `Uint8Array` over the very buffer passed in. No copy in Rust, no decode in JS.
- `SsrEngine::render_with_json_and_bytes(url, json, bytes)` — a small JSON
  envelope *and* a binary payload: `renderPage(url, data, bytes)`. This is the
  shape most page data actually has, and it removes the base64-into-a-JSON-field
  round trip that otherwise obliges every bundle to carry its own `atob`.

A bundle written as `function(url, data)` ignores the third argument, so this
can be adopted one side at a time.

### The bundle is no longer process-global

`SSR_BUNDLE`, `init_bundle`, `init_bundle_with`, `init_bundle_from_string`,
`init_bundle_raw`, `get_bundle` and `is_initialized` are **removed**, replaced by
`v8_pool::compose(path, polyfills)`. The composed source is owned by the pool.

The global meant the *second* engine built in a process silently rendered the
*first* one's bundle, and that a test binary could hold only one bundle however
many cases it had. Engines are now independent.

### `BROWSER_POLYFILLS` is public

Testing an SSR bundle means running it in the engine's environment. Without the
constant exported, projects re-type the prelude into a Node `vm` sandbox by
hand — and that copy drifts silently, because a drifted probe still passes. Write
the real thing out instead:

```rust
std::fs::write("prelude.js", rusty_ssr::v8_pool::BROWSER_POLYFILLS)?;
```

### Breaking changes

- The `init_bundle*` / `get_bundle` / `is_initialized` family is gone (see
  above). `SsrEngine` never needed them; it composed the bundle itself.
- `V8PoolConfig` gained a required `bundle: Arc<str>` field.
- `v8_pool::renderer::render_html` takes a `RenderPayload` instead of
  `Option<&str>`.

Nothing else changed shape: `render`, `render_with_data`, `render_to_html*`,
`render_uncached*` and the fragment cache behave exactly as in 0.1.1.

## 0.1.1

- Cache key covers `data` as well as the URL (0.1.0 checked the cache *before*
  it looked at `data`, so a render with different data could return another
  request's body).
- Render arguments are passed as native V8 values with the function handle
  cached, instead of interpolating the URL and data into freshly compiled JS on
  every request.
- A throw inside the render propagates as an error instead of being turned into
  a successful "SSR Error" page carrying a stack trace.
- Watchdog terminates runaway renders; per-isolate heap cap; panics in a render
  no longer kill the worker.
- `URL` / `URLSearchParams` added to the prelude; `setTimeout` defers to a
  microtask rather than running inline.
- Polyfills are non-clobbering.

## 0.1.0

Initial release.
