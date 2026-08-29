//! Binary payloads reach the bundle as a `Uint8Array`, with nothing to decode.
//!
//! Own test binary by habit, not necessity: since 0.2 the bundle belongs to the
//! pool, so `two_engines.rs` covers several in one file. Kept separate because
//! each case here wants a whole engine of its own anyway.

#![cfg(all(feature = "v8-pool", feature = "cache"))]

use rusty_ssr::SsrEngine;

const BYTES_BUNDLE: &str = r#"
    globalThis.renderPage = function(url, data) {
        if (!(data instanceof Uint8Array)) {
            return "not-bytes:" + Object.prototype.toString.call(data);
        }
        let sum = 0;
        for (let i = 0; i < data.length; i++) sum += data[i];
        return "len=" + data.length + " sum=" + sum + " first=" + data[0];
    };
"#;

#[tokio::test]
async fn bytes_arrive_as_a_uint8array() {
    let dir = tempfile::tempdir().unwrap();
    let bundle_path = dir.path().join("bytes.js");
    std::fs::write(&bundle_path, BYTES_BUNDLE).unwrap();

    let engine = SsrEngine::builder()
        .bundle_path(&bundle_path)
        .pool_size(1)
        .build_engine()
        .unwrap();

    // Includes bytes that are not valid UTF-8 and not printable — the whole
    // point is that this needs no encoding to survive the trip.
    let payload = vec![0x00, 0x01, 0xff, 0x80];
    let out = engine.render_with_bytes("/binary", payload).await.unwrap();
    assert_eq!(out, "len=4 sum=384 first=0");
}

/// A payload of real size, to prove nothing along the way turns it into a
/// string. 90 kB is the order of a page's worth of serialised rows; as base64
/// inside JSON it would be 120 kB, and the bundle would have to decode it.
#[tokio::test]
async fn a_large_payload_survives_intact() {
    let dir = tempfile::tempdir().unwrap();
    let bundle_path = dir.path().join("bytes-large.js");
    std::fs::write(&bundle_path, BYTES_BUNDLE).unwrap();

    let engine = SsrEngine::builder()
        .bundle_path(&bundle_path)
        .pool_size(1)
        .build_engine()
        .unwrap();

    let payload: Vec<u8> = (0..90_000).map(|i| (i % 251) as u8).collect();
    let expected_sum: u64 = payload.iter().map(|b| *b as u64).sum();
    let out = engine.render_with_bytes("/binary", payload).await.unwrap();
    assert_eq!(out, format!("len=90000 sum={expected_sum} first=0"));
}

/// The JSON channel still behaves — bytes are an addition, not a replacement.
#[tokio::test]
async fn json_still_arrives_as_an_object() {
    let dir = tempfile::tempdir().unwrap();
    let bundle_path = dir.path().join("json.js");
    std::fs::write(
        &bundle_path,
        r#"globalThis.renderPage = (url, data) =>
             (data instanceof Uint8Array) ? "bytes" : ("json:" + data.city);"#,
    )
    .unwrap();

    let engine = SsrEngine::builder()
        .bundle_path(&bundle_path)
        .pool_size(1)
        .build_engine()
        .unwrap();

    let out = engine
        .render_uncached("/x", r#"{"city":"Blumenau"}"#)
        .await
        .unwrap();
    assert_eq!(out, "json:Blumenau");
}

/// The shape most page data actually has: a small envelope plus one large
/// blob. Both arrive natively — object and `Uint8Array` — so nothing has to be
/// base64'd on the way in or decoded on the way out.
#[tokio::test]
async fn a_json_envelope_and_bytes_arrive_side_by_side() {
    let dir = tempfile::tempdir().unwrap();
    let bundle_path = dir.path().join("both.js");
    std::fs::write(
        &bundle_path,
        r#"globalThis.renderPage = function(url, data, bytes) {
               if (typeof data !== "object" || data === null) return "no-envelope";
               if (!(bytes instanceof Uint8Array)) return "no-bytes";
               let sum = 0;
               for (let i = 0; i < bytes.length; i++) sum += bytes[i];
               return data.city + "/" + data.page + " rows=" + bytes.length + " sum=" + sum;
           };"#,
    )
    .unwrap();

    let engine = SsrEngine::builder()
        .bundle_path(&bundle_path)
        .pool_size(1)
        .build_engine()
        .unwrap();

    let out = engine
        .render_with_json_and_bytes(
            "/venda/blumenau",
            r#"{"city":"Blumenau","page":1}"#,
            vec![10, 20, 30],
        )
        .await
        .unwrap();
    assert_eq!(out, "Blumenau/1 rows=3 sum=60");
}

/// The envelope is still JSON, so it is still checked. Junk must be refused at
/// the boundary rather than handed to the bundle.
#[tokio::test]
async fn a_malformed_envelope_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let bundle_path = dir.path().join("strict.js");
    std::fs::write(
        &bundle_path,
        r#"globalThis.renderPage = () => "should not have been called";"#,
    )
    .unwrap();

    let engine = SsrEngine::builder()
        .bundle_path(&bundle_path)
        .pool_size(1)
        .build_engine()
        .unwrap();

    let err = engine
        .render_with_json_and_bytes("/x", "{not json", vec![1, 2, 3])
        .await
        .expect_err("invalid JSON must not reach the bundle");
    assert!(err.to_string().contains("JSON"), "unhelpful error: {err}");

    // The worker is fine afterwards.
    let ok = engine
        .render_with_json_and_bytes("/x", "{}", vec![1])
        .await
        .unwrap();
    assert_eq!(ok, "should not have been called");
}

/// An empty payload is a payload: zero-length `Uint8Array`, not `undefined`.
#[tokio::test]
async fn an_empty_byte_payload_is_still_a_uint8array() {
    let dir = tempfile::tempdir().unwrap();
    let bundle_path = dir.path().join("empty.js");
    std::fs::write(
        &bundle_path,
        r#"globalThis.renderPage = (url, data) =>
             (data instanceof Uint8Array) ? ("bytes:" + data.length) : ("other:" + typeof data);"#,
    )
    .unwrap();

    let engine = SsrEngine::builder()
        .bundle_path(&bundle_path)
        .pool_size(1)
        .build_engine()
        .unwrap();

    assert_eq!(
        engine.render_with_bytes("/x", Vec::new()).await.unwrap(),
        "bytes:0"
    );
}

/// `atob`/`btoa` are Web APIs, not ECMAScript ones, so bare V8 has neither —
/// and a bundle that hits a missing `atob` throws, which on this path means an
/// empty page rather than an error anyone sees.
#[tokio::test]
async fn base64_and_screen_are_available_to_the_bundle() {
    let dir = tempfile::tempdir().unwrap();
    let bundle_path = dir.path().join("web-apis.js");
    std::fs::write(
        &bundle_path,
        r#"globalThis.renderPage = () => {
               const round = atob(btoa("Blumenau · SC"));
               return [
                   round,
                   atob("Zm9vYmFy"),
                   btoa("foobar"),
                   String(globalThis.devicePixelRatio),
                   String(globalThis.screen.width) + "x" + String(globalThis.screen.height),
                   String(globalThis.screen.deviceXDPI),
               ].join("|");
           };"#,
    )
    .unwrap();

    let engine = SsrEngine::builder()
        .bundle_path(&bundle_path)
        .pool_size(1)
        .build_engine()
        .unwrap();

    assert_eq!(
        engine.render_uncached("/", "{}").await.unwrap(),
        "Blumenau · SC|foobar|Zm9vYmFy|1|0x0|96"
    );
}

/// Invalid base64 throws rather than returning quiet nonsense — a decoder that
/// invents bytes is worse than one that stops.
#[tokio::test]
async fn atob_refuses_junk() {
    let dir = tempfile::tempdir().unwrap();
    let bundle_path = dir.path().join("bad-b64.js");
    std::fs::write(
        &bundle_path,
        r#"globalThis.renderPage = () => {
               try { atob("not!base64"); return "accepted junk"; }
               catch (e) { return "refused: " + e.message; }
           };"#,
    )
    .unwrap();

    let engine = SsrEngine::builder()
        .bundle_path(&bundle_path)
        .pool_size(1)
        .build_engine()
        .unwrap();

    let out = engine.render_uncached("/", "{}").await.unwrap();
    assert!(out.starts_with("refused:"), "got: {out}");
}
