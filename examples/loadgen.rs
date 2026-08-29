//! A load generator that varies the URL, which is the one thing `wrk` and
//! `bombardier` cannot do.
//!
//! ```text
//! cargo run --release --example loadgen -- <url> [options]
//!
//!   --connections <n>   Concurrent keep-alive connections. Default: 64
//!   --duration <secs>   How long to drive load. Default: 20
//!   --distinct          Append a unique `?u=N` to every request, so a page
//!                       cache misses every time. Default: hammer one URL.
//! ```
//!
//! Both modes are needed and they measure different things. Hammering one URL
//! measures the *cache* and the HTTP stack — a hot page in production. Varying
//! the URL measures what the server can actually *build*, which is what a
//! crawler does, what a cache-busting query string does, and what decides
//! whether a cold cache after a deploy is survivable. Every general-purpose
//! HTTP benchmark tool measures only the first, which is why an SSR engine
//! benchmarked with one tends to report its cache.
//!
//! No dependencies on purpose: raw `TcpStream` and hand-written HTTP/1.1 with
//! keep-alive. A load generator that pulls in an async runtime and a full HTTP
//! client spends its own CPU on the machine it is measuring, and on localhost
//! that shows up in the result. This spends almost none, and it compiles with a
//! bare `rustc` anywhere, which matters when the server under test lives in a
//! VM or a container that has no toolchain set up.
//!
//! Responses must carry `Content-Length` (axum does for a `String` body);
//! `Transfer-Encoding: chunked` is reported as an error rather than guessed at.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

struct Options {
    host: String,
    port: u16,
    path: String,
    connections: usize,
    duration: Duration,
    distinct: bool,
}

fn parse_options() -> Options {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let url = args
        .first()
        .filter(|a| !a.starts_with("--"))
        .cloned()
        .unwrap_or_else(|| {
            eprintln!("usage: loadgen <url> [--connections N] [--duration S] [--distinct]");
            std::process::exit(2);
        });

    let value = |name: &str, default: usize| -> usize {
        args.iter()
            .position(|a| a == name)
            .and_then(|i| args.get(i + 1))
            .and_then(|v| v.parse().ok())
            .unwrap_or(default)
    };

    let rest = url.strip_prefix("http://").unwrap_or(&url);
    let (authority, path) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, "/"),
    };
    let (host, port) = match authority.rsplit_once(':') {
        Some((h, p)) => (h.to_string(), p.parse().unwrap_or(80)),
        None => (authority.to_string(), 80),
    };

    Options {
        host,
        port,
        path: path.to_string(),
        connections: value("--connections", 64),
        duration: Duration::from_secs(value("--duration", 20) as u64),
        distinct: args.iter().any(|a| a == "--distinct"),
    }
}

fn main() {
    let opts = parse_options();
    let addr = format!("{}:{}", opts.host, opts.port);

    println!(
        "{} — {} connections, {}s, {}",
        addr,
        opts.connections,
        opts.duration.as_secs(),
        if opts.distinct {
            "a distinct URL every request (the cache always misses)"
        } else {
            "one URL (the cache always hits)"
        }
    );

    let counter = Arc::new(AtomicU64::new(0));
    let failed = Arc::new(AtomicU64::new(0));
    let bytes = Arc::new(AtomicU64::new(0));
    let deadline = Instant::now() + opts.duration;

    let started = Instant::now();
    let workers: Vec<_> = (0..opts.connections)
        .map(|_| {
            let (addr, path, distinct) = (addr.clone(), opts.path.clone(), opts.distinct);
            let host = opts.host.clone();
            let (counter, failed, bytes) =
                (Arc::clone(&counter), Arc::clone(&failed), Arc::clone(&bytes));
            std::thread::spawn(move || {
                let mut latencies: Vec<Duration> = Vec::with_capacity(1 << 14);
                let mut stream = None;

                while Instant::now() < deadline {
                    // Reconnect on first use and after any protocol error: a
                    // dropped connection must not end the run, it must be
                    // counted and retried, the way a real client behaves.
                    if stream.is_none() {
                        match TcpStream::connect(&addr) {
                            Ok(s) => {
                                let _ = s.set_nodelay(true);
                                stream = Some(BufReader::new(s));
                            }
                            Err(_) => {
                                failed.fetch_add(1, Ordering::Relaxed);
                                continue;
                            }
                        }
                    }
                    let reader = stream.as_mut().expect("connected above");

                    let url = if distinct {
                        let n = counter.fetch_add(1, Ordering::Relaxed);
                        let sep = if path.contains('?') { '&' } else { '?' };
                        format!("{path}{sep}u={n}")
                    } else {
                        path.clone()
                    };
                    let request = format!(
                        "GET {url} HTTP/1.1\r\nHost: {host}\r\nConnection: keep-alive\r\nAccept: text/html\r\n\r\n"
                    );

                    let start = Instant::now();
                    if reader.get_mut().write_all(request.as_bytes()).is_err() {
                        failed.fetch_add(1, Ordering::Relaxed);
                        stream = None;
                        continue;
                    }

                    match read_response(reader) {
                        Ok(n) => {
                            latencies.push(start.elapsed());
                            bytes.fetch_add(n as u64, Ordering::Relaxed);
                        }
                        Err(_) => {
                            failed.fetch_add(1, Ordering::Relaxed);
                            stream = None;
                        }
                    }
                }

                latencies
            })
        })
        .collect();

    let mut latencies: Vec<Duration> = Vec::new();
    for w in workers {
        if let Ok(mut l) = w.join() {
            latencies.append(&mut l);
        }
    }
    let elapsed = started.elapsed();
    latencies.sort_unstable();

    let ok = latencies.len() as f64;
    let at = |p: usize| -> Duration {
        if latencies.is_empty() {
            Duration::ZERO
        } else {
            latencies[(latencies.len() - 1) * p / 100]
        }
    };

    println!();
    println!("  pages/s : {:.0}", ok / elapsed.as_secs_f64());
    println!(
        "  latency : p50 {:?}  p95 {:?}  p99 {:?}  max {:?}",
        at(50),
        at(95),
        at(99),
        latencies.last().copied().unwrap_or_default()
    );
    println!(
        "  volume  : {:.1} MB/s, {} responses, {} failed",
        bytes.load(Ordering::Relaxed) as f64 / elapsed.as_secs_f64() / 1e6,
        latencies.len(),
        failed.load(Ordering::Relaxed)
    );
}

/// Read one HTTP/1.1 response, returning the body length.
///
/// Only `Content-Length` is understood. Guessing at a chunked body would make
/// the numbers quietly wrong rather than loudly absent, so it is an error.
fn read_response(reader: &mut BufReader<TcpStream>) -> Result<usize, String> {
    let mut length: Option<usize> = None;
    let mut line = String::new();

    loop {
        line.clear();
        let n = reader
            .read_line(&mut line)
            .map_err(|e| format!("read: {e}"))?;
        if n == 0 {
            return Err("connection closed".into());
        }
        if line == "\r\n" || line == "\n" {
            break;
        }
        let lower = line.to_ascii_lowercase();
        if let Some(v) = lower.strip_prefix("content-length:") {
            length = v.trim().parse().ok();
        } else if lower.starts_with("transfer-encoding:") && lower.contains("chunked") {
            return Err("chunked responses are not measured".into());
        }
    }

    let len = length.ok_or("no content-length")?;
    let mut body = vec![0u8; len];
    reader
        .read_exact(&mut body)
        .map_err(|e| format!("body: {e}"))?;
    Ok(len)
}
