//! Binary payloads reach the bundle as a `Uint8Array`, with nothing to decode.
//!
//! Own test binary by habit, not necessity: since 0.2 the bundle belongs to the
//! pool, so `two_engines.rs` covers several in one file. Kept separate because
//! each case here wants a whole engine of its own anyway.

#![cfg(all(feature = "v8-pool", feature = "cache"))]

mod common;

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
    let engine = common::engine(BYTES_BUNDLE);

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
    let engine = common::engine(BYTES_BUNDLE);

    let payload: Vec<u8> = (0..90_000).map(|i| (i % 251) as u8).collect();
    let expected_sum: u64 = payload.iter().map(|b| *b as u64).sum();
    let out = engine.render_with_bytes("/binary", payload).await.unwrap();
    assert_eq!(out, format!("len=90000 sum={expected_sum} first=0"));
}

/// The JSON channel still behaves — bytes are an addition, not a replacement.
#[tokio::test]
async fn json_still_arrives_as_an_object() {
    let engine = common::engine(
        r#"globalThis.renderPage = (url, data) =>
             (data instanceof Uint8Array) ? "bytes" : ("json:" + data.city);"#,
    );

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
    let engine = common::engine(
        r#"globalThis.renderPage = function(url, data, bytes) {
               if (typeof data !== "object" || data === null) return "no-envelope";
               if (!(bytes instanceof Uint8Array)) return "no-bytes";
               let sum = 0;
               for (let i = 0; i < bytes.length; i++) sum += bytes[i];
               return data.city + "/" + data.page + " rows=" + bytes.length + " sum=" + sum;
           };"#,
    );

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
    let engine = common::engine(
        r#"globalThis.renderPage = () => "should not have been called";"#,
    );

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
    let engine = common::engine(
        r#"globalThis.renderPage = (url, data) =>
             (data instanceof Uint8Array) ? ("bytes:" + data.length) : ("other:" + typeof data);"#,
    );

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
    let engine = common::engine(
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
    );

    assert_eq!(
        engine.render_uncached("/", "{}").await.unwrap(),
        "Blumenau · SC|foobar|Zm9vYmFy|1|0x0|96"
    );
}

/// Invalid base64 throws rather than returning quiet nonsense — a decoder that
/// invents bytes is worse than one that stops.
#[tokio::test]
async fn atob_refuses_junk() {
    let engine = common::engine(
        r#"globalThis.renderPage = () => {
               try { atob("not!base64"); return "accepted junk"; }
               catch (e) { return "refused: " + e.message; }
           };"#,
    );

    let out = engine.render_uncached("/", "{}").await.unwrap();
    assert!(out.starts_with("refused:"), "got: {out}");
}

/// The four things a decode table and a chunked output buffer can get wrong
/// that a one-character-at-a-time decoder could not.
///
/// `atob` was rewritten in 0.3.4 for speed — it was 22% of a real consumer's
/// SSR render — and every case below is a boundary the old implementation did
/// not have. Padding and whitespace it did handle, and they are here because
/// they are the contract the rewrite had to preserve, not because they were at
/// risk. The other two were:
///
///   * a code point past the 256-entry table. An out-of-range read on a typed
///     array is `undefined`, and `undefined < 0` is false, so the obvious
///     spelling accepts `Ā` as a base64 digit and invents bytes.
///   * a payload longer than one output run, which has to stitch. Everything
///     the crate's own tests decode is a few bytes long and never reaches it.
#[tokio::test]
async fn atob_handles_padding_whitespace_high_code_points_and_long_payloads() {
    let engine = common::engine(
        r#"globalThis.renderPage = () => {
               const out = [];
               // Every padding shape, one two and no '=' characters.
               out.push(atob("TWE=") + "/" + atob("TWFu") + "/" + atob("TQ=="));
               // Whitespace is stripped wherever it appears, not just at the ends.
               out.push(atob(" Zm9v\nYmFy ") === "foobar" ? "ws-ok" : "ws-BAD");
               // A length that no base64 can have.
               try { atob("A"); out.push("len-accepted"); }
               catch (e) { out.push("len-" + (e.message.indexOf("length") >= 0 ? "refused" : "wrong")); }
               // Past the table. Four characters, so it survives the length
               // check and reaches the lookup, which is the point.
               try { atob("QUJĀ"); out.push("hi-accepted"); }
               catch (e) { out.push("hi-refused"); }
               // Longer than one output run, so the result is stitched from
               // several. Every byte value appears, including NUL.
               let big = "";
               for (let i = 0; i < 20000; i++) big += String.fromCharCode(i % 256);
               out.push(atob(btoa(big)) === big ? "big-ok(" + big.length + ")" : "big-BAD");
               return out.join("|");
           };"#,
    );

    assert_eq!(
        engine.render_uncached("/", "{}").await.unwrap(),
        "Ma/Man/M|ws-ok|len-refused|hi-refused|big-ok(20000)"
    );
}
