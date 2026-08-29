//! What single-pass template assembly promises.
//!
//! The engine assembles a document by walking the template once and swapping in
//! every placeholder as it goes, rather than chaining `String::replace` per
//! placeholder. That is faster, but it is also *different*, and the difference
//! is the part worth pinning: a chained replace re-scans everything it has
//! already written, so a rendered fragment containing the literal text of
//! another placeholder gets that placeholder substituted too. Since the
//! fragment is attacker-influenced on any page that echoes user content, the
//! single-pass behaviour is a correctness property and not just a speed one.
//!
//! These tests exist so that stays true through any rewrite of the scan.

#![cfg(all(feature = "v8-pool", feature = "cache"))]

use std::io::Write;

use rusty_ssr::SsrEngine;

const TEMPLATE: &str = "\
<!doctype html>
<html>
<head>
<title><!--ssr:title--></title>
<!--seo-->
</head>
<body>
<div id=\"root\"><!--ssr:outlet--></div>
<script><!--ssr:state--></script>
</body>
</html>
";

/// An engine whose bundle echoes back whatever the caller puts in `data.body`,
/// so a test can decide exactly what the "rendered fragment" contains.
fn engine_with_template(template: &str) -> (tempfile::TempDir, SsrEngine) {
    let dir = tempfile::tempdir().unwrap();

    let bundle_path = dir.path().join("bundle.js");
    let mut f = std::fs::File::create(&bundle_path).unwrap();
    // `typeof === 'string'`, not truthiness: an empty fragment is a value the
    // tests deliberately pass, and `''` is falsy.
    writeln!(
        f,
        "globalThis.renderPage = (url, data) => (data && typeof data.body === 'string') ? data.body : '<main>' + url + '</main>';"
    )
    .unwrap();
    drop(f);

    let template_path = dir.path().join("index.html");
    std::fs::write(&template_path, template).unwrap();

    let engine = SsrEngine::builder()
        .bundle_path(&bundle_path)
        .html_template(&template_path)
        .pool_size(1)
        .build_engine()
        .expect("engine");

    (dir, engine)
}

fn body_json(body: &str) -> String {
    serde_json::json!({ "body": body }).to_string()
}

// ── the basics ───────────────────────────────────────────────────────────────

#[tokio::test]
async fn the_fragment_lands_in_the_outlet() {
    let (_dir, engine) = engine_with_template(TEMPLATE);

    let html = engine
        .render_to_html_uncached("/home", "{}")
        .await
        .unwrap();

    assert!(html.contains("<div id=\"root\"><main>/home</main></div>"));
    assert!(
        !html.contains("<!--ssr:outlet-->"),
        "the outlet placeholder must be consumed"
    );
}

#[tokio::test]
async fn every_supplied_placeholder_is_replaced() {
    let (_dir, engine) = engine_with_template(TEMPLATE);

    let html = engine
        .render_to_html_uncached_with_replacements(
            "/home",
            "{}",
            &[
                ("<!--ssr:title-->", "Listing #42"),
                ("<!--seo-->", "<meta property=\"og:title\" content=\"x\" />"),
                ("<!--ssr:state-->", "window.__S__={\"a\":1};"),
            ],
        )
        .await
        .unwrap();

    assert!(html.contains("<title>Listing #42</title>"));
    assert!(html.contains("<meta property=\"og:title\" content=\"x\" />"));
    assert!(html.contains("window.__S__={\"a\":1};"));
    assert!(html.contains("<main>/home</main>"));
    assert!(!html.contains("<!--ssr:"), "no placeholder may survive: {html}");
}

/// A placeholder nobody supplied is left exactly as it was — the engine must
/// not silently blank it, because a template author can legitimately keep a
/// comment in the output.
#[tokio::test]
async fn an_unsupplied_placeholder_is_left_alone() {
    let (_dir, engine) = engine_with_template(TEMPLATE);

    let html = engine
        .render_to_html_uncached_with_replacements(
            "/home",
            "{}",
            &[("<!--ssr:title-->", "Only this one")],
        )
        .await
        .unwrap();

    assert!(html.contains("<title>Only this one</title>"));
    assert!(
        html.contains("<!--seo-->"),
        "an unsupplied placeholder must survive verbatim"
    );
}

/// Repeated placeholders all get replaced, not just the first.
#[tokio::test]
async fn a_placeholder_used_twice_is_replaced_twice() {
    let template = "<a><!--ssr:title--></a><b><!--ssr:outlet--></b><c><!--ssr:title--></c>";
    let (_dir, engine) = engine_with_template(template);

    let html = engine
        .render_to_html_uncached_with_replacements("/x", "{}", &[("<!--ssr:title-->", "T")])
        .await
        .unwrap();

    assert_eq!(html, "<a>T</a><b><main>/x</main></b><c>T</c>");
}

// ── the property that makes single-pass worth having ─────────────────────────

/// A rendered fragment that happens to contain another placeholder's literal
/// text must be emitted as-is. Chaining `String::replace` gets this wrong: the
/// outlet is substituted first, and the next replace then finds the
/// placeholder *inside the fragment* and substitutes there too.
///
/// On any page that echoes user input — a search term, a listing title, a
/// review — the fragment is attacker-influenced, so this is the difference
/// between a comment in someone's text and an injection point.
#[tokio::test]
async fn a_placeholder_inside_the_fragment_is_not_substituted() {
    let (_dir, engine) = engine_with_template(TEMPLATE);

    // The "user" typed the literal text of a placeholder into a search box.
    let fragment = "<main>you searched for: <!--ssr:state--></main>";

    let html = engine
        .render_to_html_uncached_with_replacements(
            "/search",
            &body_json(fragment),
            &[("<!--ssr:state-->", "window.__S__={\"secret\":1};")],
        )
        .await
        .unwrap();

    assert!(
        html.contains("you searched for: <!--ssr:state--></main>"),
        "the fragment must be emitted verbatim, got:\n{html}"
    );
    // The real placeholder, the one in the template, is still replaced.
    assert!(html.contains("<script>window.__S__={\"secret\":1};</script>"));
    assert_eq!(
        html.matches("window.__S__").count(),
        1,
        "the value must be injected exactly once, not once per echo"
    );
}

/// The same guarantee between two caller-supplied replacements: a value
/// containing another needle is not re-scanned either.
#[tokio::test]
async fn a_replacement_value_is_never_rescanned() {
    let template = "<x><!--a--></x><y><!--b--></y><z><!--ssr:outlet--></z>";
    let (_dir, engine) = engine_with_template(template);

    let html = engine
        .render_to_html_uncached_with_replacements(
            "/x",
            "{}",
            &[("<!--a-->", "value-of-a contains <!--b-->"), ("<!--b-->", "B")],
        )
        .await
        .unwrap();

    assert_eq!(html, "<x>value-of-a contains <!--b--></x><y>B</y><z><main>/x</main></z>");
}

/// Order in the replacements slice must not change the output when the needles
/// are distinct — two call sites that list the same pairs differently produce
/// the same document.
#[tokio::test]
async fn replacement_order_does_not_matter() {
    let (_dir, engine) = engine_with_template(TEMPLATE);

    let forwards = engine
        .render_to_html_uncached_with_replacements(
            "/home",
            "{}",
            &[
                ("<!--ssr:title-->", "T"),
                ("<!--seo-->", "S"),
                ("<!--ssr:state-->", "St"),
            ],
        )
        .await
        .unwrap();

    let backwards = engine
        .render_to_html_uncached_with_replacements(
            "/home",
            "{}",
            &[
                ("<!--ssr:state-->", "St"),
                ("<!--seo-->", "S"),
                ("<!--ssr:title-->", "T"),
            ],
        )
        .await
        .unwrap();

    assert_eq!(forwards, backwards);
}

/// When two needles could match at the same place, the one starting earliest
/// wins; on a tie, the scan must still terminate and consume input.
#[tokio::test]
async fn overlapping_needles_take_the_earliest_match() {
    let template = "[<!--ab-->][<!--ssr:outlet-->]";
    let (_dir, engine) = engine_with_template(template);

    let html = engine
        .render_to_html_uncached_with_replacements(
            "/x",
            "{}",
            &[("<!--ab-->", "LONG"), ("<!--a", "SHORT")],
        )
        .await
        .unwrap();

    // Both needles begin at the same offset; whichever is chosen, exactly one
    // substitution happens there and the rest of the template is intact.
    assert!(
        html == "[LONG][<main>/x</main>]" || html == "[SHORTb-->][<main>/x</main>]",
        "unexpected assembly: {html}"
    );
    assert!(html.ends_with("[<main>/x</main>]"));
}

/// An empty needle would match everywhere and never advance the cursor. It has
/// to be ignored rather than hang the render.
#[tokio::test]
async fn an_empty_needle_is_ignored() {
    let (_dir, engine) = engine_with_template("<a><!--ssr:outlet--></a>");

    let html = engine
        .render_to_html_uncached_with_replacements("/x", "{}", &[("", "NOPE")])
        .await
        .unwrap();

    assert_eq!(html, "<a><main>/x</main></a>");
}

// ── shapes that are easy to get wrong at the edges ───────────────────────────

#[tokio::test]
async fn a_placeholder_at_the_very_start_and_end_is_handled() {
    let (_dir, engine) = engine_with_template("<!--ssr:outlet--><!--ssr:title-->");

    let html = engine
        .render_to_html_uncached_with_replacements("/x", "{}", &[("<!--ssr:title-->", "END")])
        .await
        .unwrap();

    assert_eq!(html, "<main>/x</main>END");
}

#[tokio::test]
async fn an_empty_fragment_still_assembles_the_document() {
    let (_dir, engine) = engine_with_template(TEMPLATE);

    let html = engine
        .render_to_html_uncached(
            "/x",
            &body_json(""),
        )
        .await
        .unwrap();

    assert!(html.contains("<div id=\"root\"></div>"));
    assert!(html.starts_with("<!doctype html>"));
}

/// Non-ASCII around a placeholder must survive: the scan works on byte offsets
/// and a boundary error here would panic or corrupt the document.
#[tokio::test]
async fn multibyte_text_around_placeholders_survives() {
    let (_dir, engine) =
        engine_with_template("<p>Разместите объявление <!--ssr:outlet--> в Blumenau 🏠</p>");

    let html = engine
        .render_to_html_uncached("/венда", "{}")
        .await
        .unwrap();

    assert_eq!(
        html,
        "<p>Разместите объявление <main>/венда</main> в Blumenau 🏠</p>"
    );
}

/// A large fragment and several placeholders together — the shape a real page
/// has, and the one where a per-placeholder full-document copy would show up.
#[tokio::test]
async fn a_large_document_assembles_correctly() {
    let (_dir, engine) = engine_with_template(TEMPLATE);

    let fragment = format!("<main>{}</main>", "<p>row</p>".repeat(5000));
    let html = engine
        .render_to_html_uncached_with_replacements(
            "/big",
            &body_json(&fragment),
            &[
                ("<!--ssr:title-->", "Big"),
                ("<!--seo-->", "<meta />"),
                ("<!--ssr:state-->", "S"),
            ],
        )
        .await
        .unwrap();

    assert_eq!(html.matches("<p>row</p>").count(), 5000);
    assert!(html.contains("<title>Big</title>"));
    assert!(!html.contains("<!--ssr:"));
}

// ── no template configured ───────────────────────────────────────────────────

#[tokio::test]
async fn without_a_template_the_fragment_is_returned_untouched() {
    let dir = tempfile::tempdir().unwrap();
    let bundle_path = dir.path().join("bundle.js");
    std::fs::write(
        &bundle_path,
        "globalThis.renderPage = (url) => '<main>' + url + '</main>';",
    )
    .unwrap();

    let engine = SsrEngine::builder()
        .bundle_path(&bundle_path)
        .pool_size(1)
        .build_engine()
        .unwrap();

    assert!(!engine.has_template());

    let html = engine
        .render_to_html_uncached_with_replacements("/x", "{}", &[("<!--ssr:title-->", "ignored")])
        .await
        .unwrap();

    assert_eq!(html, "<main>/x</main>");
}
