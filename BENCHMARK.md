# Benchmarks

Every figure here was measured, and each one says what it was measured on. The
file it replaced quoted competitor throughput with a `~` and no method, and
projected AWS bills from a number that never rendered anything; none of that
survived being checked, so none of it is here.

## The one rule for reading SSR benchmarks

**A cached response and a rendered page differ by fifty times.** Quote one
without saying which, and the number means nothing:

| | pages/s | p50 |
|---|---|---|
| Served from the page cache | 48,029 | 1.04 ms |
| Actually rendered | 950 | 17.1 ms |

Both are real, from the same server, same page, minutes apart. The first is a
memory copy and a socket write; the second is V8 executing your bundle. Almost
every "N requests per second" claim about an SSR engine — including the ones
this project used to make — is the first number wearing the second's name.

This matters practically because general-purpose load tools cannot tell them
apart. `wrk`, `bombardier` and `ab` all hammer one URL, so against any engine
with a page cache they measure the cache. `examples/loadgen.rs` exists for that
reason: `--distinct` appends a unique query to every request, so the cache
misses every time and you measure what the server can build.

## Hardware and method

- **CPU**: AMD Ryzen 7 260, 8 physical cores / 16 logical
- **RAM**: 16 GB
- **OS**: Windows 11 for the synthetic runs; WSL2 (Arch, kernel 6.6) for the
  application runs
- **Load generator on the same machine.** It competes for the same cores, which
  depresses every figure below and depresses the high-`pool_size` ones most.
- Runs are sequential, never concurrent. Each is preceded by a discarded warm-up.
- V8 tiers up from its interpreter after a few hundred calls, so **every run
  warms first**. Skipping that reported a 45 µs render as 315 µs.

## 1. A real application

The most useful numbers here, because nothing is synthetic: a deployed Rust
service (axum + sqlx/Postgres) embedding this crate, rendering a 2.5 MB Preact
bundle into 200–320 kB pages, with database queries behind them.
`pool_size = 16`, page cache of 150 entries with a 5-minute TTL and a stale
window.

**Served from the page cache** — one hot URL:

| route | page size | pages/s | p50 | p99 |
|---|---|---|---|---|
| `/` | 323 kB | 48,029 | 1.04 ms | 5.61 ms |
| `/venda/blumenau` | 195 kB | 25,100 | 2.36 ms | 6.96 ms |
| `/aluguel/blumenau` | 308 kB | 20,892 | 2.93 ms | 7.83 ms |

15.5 GB/s on the first row: at that point the bottleneck is `memcpy` and the
socket, not this crate.

**Actually rendering** — a distinct URL every request:

| connections | pages/s | p50 | p99 |
|---|---|---|---|
| 16 | 901 | 17.1 ms | 31.2 ms |
| 64 | 947 | 66.2 ms | 94.6 ms |
| 128 | 981 | 128.4 ms | 165.5 ms |

**Throughput saturates at `connections = pool_size`.** Going from 16 to 128
bought 9% more throughput and made p50 seven times worse: past saturation,
added concurrency is queue delay and nothing else. The arithmetic closes —
16 workers ÷ 950 pages/s = 16.8 ms per page, and p50 at 16 connections was
17.05 ms.

### Where the 17 ms goes

The same route with `SSR_DISABLE=1`, which serves the static shell and skips
the render, isolates what rendering costs:

| | one connection | 16 connections |
|---|---|---|
| SSR on | p50 7.91 ms | 825 pages/s |
| SSR off | p50 5.38 ms | 2,570 pages/s |

**The render is 2.53 ms of a 7.9 ms request** — under a third. The rest is
routing, database queries, SEO tags and shell assembly. But it is the part that
serialises: removing it triples throughput under concurrency, because the render
is CPU-bound across a fixed pool while the queries are not.

Two consequences. Making the render free would improve a single request by about
1.5×, not 3×. And optimising the engine underneath it is chasing a fraction of
a fraction — at 17 ms per page, this crate's own overhead is fractions of a
percent.

### How many workers are worth having

Measured by pinning the server to a subset of cores, which is also what
`num_cpus::get()` reads:

| cores | pool_size | pages/s | p50 | RSS |
|---|---|---|---|---|
| 4 | 4 | 587 | 53.8 ms | 513 MB |
| 8 | 8 | 882 | 35.7 ms | 933 MB |
| 12 | 12 | 973 | 32.3 ms | 1,338 MB |
| 16 | 16 | 977 | 32.1 ms | 1,738 MB |

**Throughput saturates at about 12 workers on 8 physical cores; memory does
not.** Going from 12 to 16 bought 0.4% more throughput for 400 MB more RSS, and
8 workers held 90% of peak throughput on 54% of the memory. Oversubscription is
not the problem — the extra workers simply stop paying for themselves while
continuing to cost.

Pick `pool_size` against that curve rather than against the core count.

### Sizing from these numbers

The pool is the ceiling, so capacity is set by how often you miss the cache:

```
sustained req/s  ≤  950 / (1 − hit_rate)
```

| hit rate | sustained req/s |
|---|---|
| 90% | ~9,500 |
| 95% | ~19,000 |
| 99% | ~95,000 |

A cold cache after a deploy is therefore the dangerous moment: whatever the
steady state, the first seconds run at 950/s.

**Memory**: 507 MB at boot, 1.65 GB after a six-second warm-up, 1.77 GB at the
end and flat — roughly 70 MB per isolate with a 2.5 MB bundle. That is the
working set, not a leak, and the page cache is not in it (150 entries ≈ 45 MB).
Cap each isolate with `max_heap_mb` so a runaway render fails instead of growing.

## 2. Against Node and Next.js

Same machine, same 20-card page, and — where it is shared — the same bundle.
20 s at 64 connections, plain HTML both sides (Next's compression disabled so
neither trades CPU for bytes).

| | pages/s | p50 | p99 | bytes/page |
|---|---|---|---|---|
| rusty-ssr + Preact, `pool_size=16` | 18,475 | 3.38 ms | 6.88 ms | 24,483 |
| rusty-ssr + React, `pool_size=16` | 19,802 | 3.13 ms | 6.41 ms | 24,894 |
| rusty-ssr + Preact, `pool_size=1` | 2,421 | 25.8 ms | 33.7 ms | 24,483 |
| rusty-ssr + React, `pool_size=1` | 2,560 | 24.5 ms | 31.4 ms | 24,894 |
| Node + the same Preact bundle | 2,250 | 28.1 ms | 42.9 ms | 24,483 |
| Node + React `renderToString` | 2,206 | 30.7 ms | 69.0 ms | 24,894 |
| Next.js 15.5 App Router (RSC) | 125 | 492 ms | 638 ms | 77,102 |

Three things follow, and only the third is about this crate:

1. **Per render thread, this engine is at parity with Node.** 2,421 against
   2,250 with the identical bundle — about 1.1×, which run-to-run variance can
   account for most of. Rendering is V8 executing bytecode, and it is the same
   V8; there is no mechanism by which Rust makes it faster.
2. **React and Preact render at the same speed.** 2,206 against 2,250 on Node,
   2,560 against 2,421 here. The 17.7× gap between Node + React and Next.js is
   therefore *not* React — it is the App Router's RSC pipeline and the 77 kB
   hydration payload it emits against 24 kB of markup.
3. **The advantage is 7.6× of in-process scaling.** 2,421 → 18,475 by running
   16 isolates in one process. Matching that with Node means about nine
   processes, each with its own heap, its own compiled copy of the bundle and
   its own fragmented cache.

Multiplying out: 17.7 × 1.16 × 7.7 ≈ 158, against the 148× measured end to end
between Next.js and this at `pool_size=16`. Two of those three factors belong to
somebody else.

**Caveats.** Next.js was measured in App Router / RSC mode, which is React's
slowest server path and emits a hydratable payload — it is doing strictly more
work than the other rows. Next was also run as one process, as it ships;
scaling it means running several, which is the memory comparison rather than
the throughput one. Pages Router would be faster and was not measured.

## 3. Engine microbenchmarks

`cargo bench --bench hotpath_benchmark --all-features` walks the paths a request
takes, through the public API.

| | |
|---|---|
| Fragment-cache hit (hot tier) | 42–69 ns |
| Fragment-cache miss | 34–52 ns |
| Fragment-cache insert | 0.39–0.53 µs |
| Page-cache `get` hit | 130–300 ns |
| 60 kB JSON payload delivered to the bundle | ~300 µs |

### On trusting these numbers

**Run the same unchanged code twice and criterion will report a 32%
improvement, with `p = 0.00`.** A laptop's clock speed depends on how warm it
is, and these benchmarks are short enough to sit inside that drift. Anything
under about a third is a statement about the machine.

The ranges above are the spread across independent runs, not error bars. Where a
figure has to be trusted, get it one of two ways: a large effect reproduced
across separate runs, or a ratio between two benchmarks *inside one run*, where
the drift cancels.

## 4. Pool sizing

Measured with a synthetic bundle so the render cost could be dialled.

**Render cost against page size** (single-threaded, warmed):

| listings | HTML | per render | ceiling per worker |
|---|---|---|---|
| 1 | 2 kB | 44.9 µs | 22,282/s |
| 20 | 24 kB | 287.9 µs | 3,473/s |
| 50 | 60 kB | 692.2 µs | 1,445/s |
| 200 | 238 kB | 3.15 ms | 318/s |

**Scaling with workers** (2 kB page, 64 connections):

| pool_size | pages/s | vs one worker |
|---|---|---|
| 1 | 42,601 | 1.0× |
| 2 | 84,208 | 2.0× |
| 4 | 153,351 | 3.6× |
| 8 | 218,959 | 5.1× |
| 16 | 265,144 | 6.2× |

Sub-linear, and it should be: 16 workers on 8 physical cores is a 2×
oversubscription, and the runtime feeding them needs CPU too. Past roughly 1.5×
the physical core count, throughput barely moves while p99 degrades sharply — at
`pool_size=24` throughput rose 15% and p99 went from 12 ms to 32 ms.

**Memory per isolate**: ~36 MB baseline, plus about four times the bundle's size
again for compiled code — 1.7 MB of JS added ~7 MB per isolate. Start-up is
~70 ms per isolate for a small bundle and ~100 ms for a 1.7 MB one, in parallel
across the pool, so V8 snapshots would not buy much.

**`num_cpus::get()` is the wrong default inside a container.** It reads CPU
affinity, not the CFS quota, so on a 32-core host it returns 32 however small
your container's share. At ~36 MB per isolate that is over a gigabyte allocated
before the first request. Take `pool_size` from your platform's variable.

## Reproducing

```bash
# Engine microbenchmarks. Compare against a saved baseline, and read the
# note above about the noise floor before believing a small change.
cargo bench --bench hotpath_benchmark --all-features -- --save-baseline before
cargo bench --bench hotpath_benchmark --all-features -- --baseline before

# An HTTP server around the engine, for comparing against other stacks.
cargo run --release --example http_server --features axum-integration -- \
    --bundle path/to/ssr-bundle.js --rows 20 --pool-size 16

# Load, both ways. --distinct is the one that measures rendering.
cargo run --release --example loadgen -- http://127.0.0.1:3001/render/1 \
    --connections 64 --duration 20
cargo run --release --example loadgen -- http://127.0.0.1:3001/render/1 \
    --connections 64 --duration 20 --distinct

# In-process, with a payload-size dial and no HTTP in the way.
cargo run --release --example loadtest -- \
    --bundle path/to/ssr-bundle.js --rows 50 --hit-ratio 0 --duration 30
```

`engine.pool_metrics()` reports saturation, queue depth and render percentiles
while a run is in flight — worth checking, so that a benchmark can be shown to
have reached saturation rather than assumed to have.

## What is not measured here

- **Clustered Node.** The nine-process figure is linear extrapolation, which
  flatters Node's contention; only the memory comparison is measured.
- **Linux for the synthetic rows.** Node's HTTP stack is weaker on Windows, so
  the gap there is probably narrower on Linux.
- **Any cloud instance.** There are no projections in this file, and the AWS
  cost tables that used to be here were built on numbers that never rendered a
  page.
- **Streaming.** This crate returns a whole document; nothing here says anything
  about time-to-first-byte under a streaming renderer.
