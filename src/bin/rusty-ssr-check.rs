//! `rusty-ssr-check` — run an SSR bundle in the real engine and say what happens.
//!
//! Every project that ships an SSR bundle ends up writing some version of this,
//! usually as a Node `vm` sandbox with the prelude copied into it by hand. That
//! copy is the problem: it drifts from the engine, and a drifted probe stays
//! green while production serves blank pages. This is the same checks against
//! the same V8, the same prelude and the same request boundary the server uses,
//! so it cannot drift.
//!
//! ```text
//! rusty-ssr-check dist/ssr-bundle.js --url / --url /products/42 --min-bytes 200
//! rusty-ssr-check --dump-prelude > prelude.js
//! ```
//!
//! Exit code is 0 when every check passed and 1 when any did not, so it belongs
//! in a deploy script between "build the bundle" and "ship it".

#[cfg(not(feature = "v8-pool"))]
fn main() {
    eprintln!("rusty-ssr-check needs the `v8-pool` feature.");
    std::process::exit(2);
}

#[cfg(feature = "v8-pool")]
fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let opts = match Options::parse(&args) {
        Ok(Some(opts)) => opts,
        Ok(None) => return, // --help or --dump-prelude, already handled
        Err(msg) => {
            eprintln!("{msg}\n");
            eprint!("{USAGE}");
            std::process::exit(2);
        }
    };

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");
    std::process::exit(runtime.block_on(run(opts)));
}

#[cfg(feature = "v8-pool")]
const USAGE: &str = "\
Usage: rusty-ssr-check <bundle.js> [options]
       rusty-ssr-check --dump-prelude

Runs the bundle in the real engine and reports what it does.

Options:
  --url <path>        A URL to render. Repeatable. Default: /
  --data <json>       JSON payload for every render. Default: {}
  --render-fn <name>  Global to call. Default: renderPage
  --min-bytes <n>     Fail a render shorter than this. Default: 1
  --no-polyfills      Load the bundle without the browser prelude
  --seal-globals      Delete globals added by a render, before the next one
  --dump-prelude      Write the prelude to stdout and exit
  --help              This text

Exit code 0 if every check passed, 1 otherwise.
";

#[cfg(feature = "v8-pool")]
struct Options {
    bundle: String,
    urls: Vec<String>,
    data: String,
    render_fn: String,
    min_bytes: usize,
    polyfills: bool,
    seal_globals: bool,
}

#[cfg(feature = "v8-pool")]
impl Options {
    fn parse(args: &[String]) -> Result<Option<Self>, String> {
        if args.iter().any(|a| a == "--help" || a == "-h") {
            print!("{USAGE}");
            return Ok(None);
        }
        if args.iter().any(|a| a == "--dump-prelude") {
            print!("{}", rusty_ssr::v8_pool::BROWSER_POLYFILLS);
            return Ok(None);
        }

        let mut bundle = None;
        let mut urls = Vec::new();
        let mut data = "{}".to_string();
        let mut render_fn = "renderPage".to_string();
        let mut min_bytes = 1usize;
        let mut polyfills = true;
        let mut seal_globals = false;

        let mut i = 0;
        while i < args.len() {
            let arg = args[i].as_str();
            let value = |name: &str| -> Result<String, String> {
                args.get(i + 1)
                    .cloned()
                    .ok_or_else(|| format!("{name} needs a value"))
            };
            match arg {
                "--url" => {
                    urls.push(value("--url")?);
                    i += 2;
                }
                "--data" => {
                    data = value("--data")?;
                    i += 2;
                }
                "--render-fn" => {
                    render_fn = value("--render-fn")?;
                    i += 2;
                }
                "--min-bytes" => {
                    min_bytes = value("--min-bytes")?
                        .parse()
                        .map_err(|_| "--min-bytes needs a number".to_string())?;
                    i += 2;
                }
                "--no-polyfills" => {
                    polyfills = false;
                    i += 1;
                }
                "--seal-globals" => {
                    seal_globals = true;
                    i += 1;
                }
                other if other.starts_with('-') => {
                    return Err(format!("unknown option {other}"))
                }
                other => {
                    if bundle.is_some() {
                        return Err(format!("unexpected argument {other}"));
                    }
                    bundle = Some(other.to_string());
                    i += 1;
                }
            }
        }

        if urls.is_empty() {
            urls.push("/".to_string());
        }
        Ok(Some(Self {
            bundle: bundle.ok_or("no bundle given")?,
            urls,
            data,
            render_fn,
            min_bytes,
            polyfills,
            seal_globals,
        }))
    }
}

#[cfg(feature = "v8-pool")]
async fn run(opts: Options) -> i32 {
    use rusty_ssr::v8_pool::RenderFnShape;
    use rusty_ssr::SsrEngine;

    let mut failed = false;
    let mut check = |ok: bool, label: &str, detail: String| {
        if !ok {
            failed = true;
        }
        println!("{} {label}{}", if ok { "ok  " } else { "FAIL" }, detail);
    };

    // One worker on purpose. With more, two renders can land on two isolates
    // and the state-reuse check below passes for the wrong reason.
    let engine = match SsrEngine::builder()
        .bundle_path(&opts.bundle)
        .pool_size(1)
        .polyfills(opts.polyfills)
        .render_function(&opts.render_fn)
        .min_render_bytes(opts.min_bytes)
        .seal_globals(opts.seal_globals)
        .build_engine()
    {
        Ok(engine) => {
            check(true, "bundle loads", format!(" ({})", opts.bundle));
            engine
        }
        Err(e) => {
            println!("FAIL bundle loads: {e}");
            return 1;
        }
    };

    let mut rendered: Vec<(String, String)> = Vec::new();
    for url in &opts.urls {
        match engine.render_uncached(url, &opts.data).await {
            Ok(html) => {
                check(true, "renders", format!(" {url} — {} bytes", html.len()));
                rendered.push((url.clone(), html));
            }
            Err(e) => check(false, "renders", format!(" {url} — {e}")),
        }
    }

    // Sync or async — reported, never failed on. Both render correctly, and a
    // bundle that code-splits nothing has no reason to be async.
    //
    // It earns a line of output because the engine awaits whatever the render
    // function returns, which makes the two indistinguishable from outside — so
    // a bundle can be written against a synchronous contract that was never
    // required, and nothing anywhere contradicts it. The cost of believing it
    // lands on exactly the pages worth server-rendering: a sync renderer throws
    // when a component suspends, so a lazily-loaded route cannot render and
    // gets swapped for whatever placeholder the bundle falls back to. That
    // placeholder is then what crawlers read. Nothing errors; somebody has to
    // look at a page to find out.
    match engine.render_fn_shape() {
        RenderFnShape::Async => println!(
            "ok   {}() is async — a suspending component can render",
            opts.render_fn
        ),
        RenderFnShape::Sync => println!(
            "note {}() is synchronous, returning HTML directly.\n\
             \x20    Fine if nothing in this bundle suspends. If it code-splits, a lazy\n\
             \x20    route cannot render here and its placeholder is what crawlers get —\n\
             \x20    the engine awaits the render function, so it may return a Promise\n\
             \x20    (renderToStringAsync and the equivalents in React/Vue/Solid).",
            opts.render_fn
        ),
        // Nothing rendered, so every URL above already failed and said so.
        RenderFnShape::Unknown => {}
    }

    // Does this bundle carry state between requests? The engine reuses its
    // isolate, so a module-level cache in the bundle outlives a render and the
    // next request — a different visitor — can read it. Rendering A, then
    // something else, then A again is the cheapest way to see it: if the two A
    // renders differ, something persisted.
    if let Some((url, first)) = rendered.first().cloned() {
        for other in opts.urls.iter().skip(1).take(1) {
            let _ = engine.render_uncached(other, &opts.data).await;
        }
        match engine.render_uncached(&url, &opts.data).await {
            Ok(again) if again == first => {
                check(true, "no state carried between renders", format!(" ({url})"))
            }
            Ok(again) => check(
                false,
                "no state carried between renders",
                format!(
                    " ({url} rendered {} bytes, then {} bytes — define globalThis.onSsrRequest \
                     to clear what your bundle keeps)",
                    first.len(),
                    again.len()
                ),
            ),
            Err(e) => check(false, "no state carried between renders", format!(" ({url} — {e})")),
        }
    }

    if failed {
        println!("\nSomething is wrong with this bundle. It would render in production the way it rendered here.");
    } else {
        println!("\nAll checks passed.");
    }
    i32::from(failed)
}
