//! What the bundle actually receives as its `data` argument.
//!
//! The engine hands JSON to the render function as a native V8 value, and the
//! route it takes to get there is an implementation detail — but the *value* is
//! not. A payload that arrives with its numbers turned into strings, its
//! nesting flattened, or its non-ASCII mangled is a rendering bug that surfaces
//! as wrong output rather than as an error, so it is worth stating exactly what
//! survives the crossing.
//!
//! These tests exist because that route was changed: the payload used to be
//! parsed into a `serde_json::Value` and then walked to build the V8 objects,
//! and is now parsed by V8 directly. Everything below is what had to stay true
//! across that change — plus one thing that got better, in
//! `a_proto_key_in_the_payload_does_not_reach_the_prototype`.

#![cfg(feature = "v8-pool")]

mod common;

/// An engine whose bundle answers with `expr` evaluated against `data`, so a
/// test can ask the *JavaScript side* what it got rather than guessing.
fn probe(expr: &str) -> common::Fixture {
    common::engine(&format!(
        "globalThis.renderPage = (url, data, bytes) => String({expr});"
    ))
}

async fn ask(expr: &str, data: &str) -> String {
    let engine = probe(expr);
    engine.render_uncached("/probe", data).await.unwrap()
}

// ── types cross intact ───────────────────────────────────────────────────────

/// The single most important property: the bundle gets an *object*, not a
/// string it has to parse itself.
#[tokio::test]
async fn data_arrives_as_an_object_not_a_string() {
    assert_eq!(ask("typeof data", r#"{"a":1}"#).await, "object");
    assert_eq!(ask("data.a", r#"{"a":1}"#).await, "1");
}

#[tokio::test]
async fn every_json_type_keeps_its_javascript_type() {
    let data = r#"{
        "num": 42,
        "float": 1.5,
        "str": "hello",
        "yes": true,
        "no": false,
        "nothing": null,
        "list": [1, 2, 3],
        "obj": {"nested": "yes"}
    }"#;

    assert_eq!(ask("typeof data.num", data).await, "number");
    assert_eq!(ask("typeof data.float", data).await, "number");
    assert_eq!(ask("typeof data.str", data).await, "string");
    assert_eq!(ask("typeof data.yes", data).await, "boolean");
    assert_eq!(ask("data.no", data).await, "false");
    assert_eq!(ask("data.nothing === null", data).await, "true");
    assert_eq!(ask("Array.isArray(data.list)", data).await, "true");
    assert_eq!(ask("typeof data.obj", data).await, "object");
    assert_eq!(ask("data.obj.nested", data).await, "yes");
}

#[tokio::test]
async fn numbers_keep_their_values() {
    let data = r#"{"i":42,"neg":-7,"f":1.5,"tiny":0.000001,"exp":1e21,"zero":0}"#;

    assert_eq!(ask("data.i", data).await, "42");
    assert_eq!(ask("data.neg", data).await, "-7");
    assert_eq!(ask("data.f", data).await, "1.5");
    assert_eq!(ask("data.tiny", data).await, "0.000001");
    assert_eq!(ask("data.exp", data).await, "1e+21");
    assert_eq!(ask("1 / data.zero", data).await, "Infinity");
}

/// A price in cents, an id from a 64-bit sequence: the values a catalogue page
/// actually carries. JSON has one number type and JavaScript agrees with it, so
/// what matters is that nothing turns them into strings or BigInts on the way.
#[tokio::test]
async fn large_integers_arrive_as_numbers() {
    let data = r#"{"id":9007199254740991,"price":450000137}"#;

    assert_eq!(ask("typeof data.id", data).await, "number");
    assert_eq!(ask("data.id", data).await, "9007199254740991");
    assert_eq!(ask("data.price", data).await, "450000137");
}

#[tokio::test]
async fn non_ascii_and_escapes_survive() {
    let data = r#"{"city":"Blumenau — Velha","ru":"Разместите","emoji":"🏠","quote":"say \"hi\"","tab":"a\tb","nl":"a\nb"}"#;

    assert_eq!(ask("data.city", data).await, "Blumenau — Velha");
    assert_eq!(ask("data.ru", data).await, "Разместите");
    assert_eq!(ask("data.emoji", data).await, "🏠");
    assert_eq!(ask("data.emoji.length", data).await, "2", "a surrogate pair, as in any JS string");
    assert_eq!(ask("data.quote", data).await, "say \"hi\"");
    assert_eq!(ask("data.tab", data).await, "a\tb");
    assert_eq!(ask("data.nl", data).await, "a\nb");
}

#[tokio::test]
async fn escaped_unicode_is_decoded() {
    // é and a surrogate pair written the long way.
    let data = r#"{"e":"café","pair":"🏠"}"#;

    assert_eq!(ask("data.e", data).await, "café");
    assert_eq!(ask("data.pair", data).await, "🏠");
}

#[tokio::test]
async fn empty_containers_are_preserved() {
    assert_eq!(ask("Object.keys(data).length", "{}").await, "0");
    assert_eq!(ask("data.a.length", r#"{"a":[]}"#).await, "0");
    assert_eq!(ask("Object.keys(data.o).length", r#"{"o":{}}"#).await, "0");
    assert_eq!(ask("data.s === ''", r#"{"s":""}"#).await, "true");
}

/// Not just an object at the top: a real payload nests, and every level has to
/// arrive as an object rather than collapsing into something stringified.
#[tokio::test]
async fn deep_nesting_survives() {
    let data = r#"{"a":{"b":{"c":{"d":{"e":[{"f":"bottom"}]}}}}}"#;
    assert_eq!(ask("data.a.b.c.d.e[0].f", data).await, "bottom");
}

/// The size a catalogue page actually sends. This is the case the conversion
/// route was changed for, so it had better still be correct.
#[tokio::test]
async fn a_large_payload_arrives_complete() {
    let rows: Vec<serde_json::Value> = (0..400)
        .map(|i| {
            serde_json::json!({
                "id": i,
                "title": format!("Apartamento {i} — 2 quartos"),
                "price": 450_000 + i * 137,
                "photos": ["a.jpg", "b.jpg", "c.jpg"],
            })
        })
        .collect();
    let data = serde_json::json!({ "city": "Blumenau", "rows": rows }).to_string();

    assert_eq!(ask("data.rows.length", &data).await, "400");
    assert_eq!(ask("data.rows[0].id", &data).await, "0");
    assert_eq!(ask("data.rows[399].id", &data).await, "399");
    assert_eq!(
        ask("data.rows[399].title", &data).await,
        "Apartamento 399 — 2 quartos"
    );
    assert_eq!(ask("data.rows[399].price", &data).await, (450_000 + 399 * 137).to_string());
    assert_eq!(ask("data.rows[123].photos[2]", &data).await, "c.jpg");
    assert_eq!(
        ask("data.rows.reduce((s, r) => s + r.price, 0)", &data).await,
        (0..400).map(|i| 450_000 + i * 137).sum::<i64>().to_string()
    );
}

/// A top-level array is valid JSON and some callers send one.
#[tokio::test]
async fn a_top_level_array_is_accepted() {
    assert_eq!(ask("Array.isArray(data)", "[1,2,3]").await, "true");
    assert_eq!(ask("data.length", "[1,2,3]").await, "3");
}

/// So are the top-level scalars, which are also valid JSON documents.
#[tokio::test]
async fn top_level_scalars_are_accepted() {
    assert_eq!(ask("data", "42").await, "42");
    assert_eq!(ask("data", r#""just a string""#).await, "just a string");
    assert_eq!(ask("data === null", "null").await, "true");
    assert_eq!(ask("data", "true").await, "true");
}

// ── malformed input is refused, and says why ─────────────────────────────────

#[tokio::test]
async fn malformed_json_is_refused() {
    let engine = probe("typeof data");

    for bad in [
        "not valid json",
        "{",
        "{\"a\":}",
        "{'a':1}",     // single quotes are not JSON
        "{\"a\":1,}",  // trailing comma
        "",
    ] {
        let err = engine
            .render_uncached("/probe", bad)
            .await
            .expect_err(&format!("{bad:?} is not JSON and must be refused"));
        let msg = err.to_string();
        assert!(
            msg.contains("Invalid JSON data"),
            "the error must say the payload was the problem, got: {msg}"
        );
    }
}

/// A refused payload must not leave the worker unusable: the next render on the
/// same isolate has to succeed. A parse that throws inside V8 leaves a pending
/// exception, and failing to clear it would poison every later request on that
/// worker.
#[tokio::test]
async fn a_worker_still_renders_after_a_malformed_payload() {
    let engine = probe("data.ok");

    for _ in 0..3 {
        assert!(engine.render_uncached("/probe", "{oops").await.is_err());
        assert_eq!(
            engine
                .render_uncached("/probe", r#"{"ok":"fine"}"#)
                .await
                .unwrap(),
            "fine",
            "the worker must recover from a rejected payload"
        );
    }
}

// ── prototype pollution ──────────────────────────────────────────────────────

/// `__proto__` in a JSON document is an ordinary key, and `JSON.parse` treats
/// it as one: it becomes an own property and does *not* reach the prototype.
///
/// This matters because payloads are attacker-influenced whenever any part of
/// them comes from user input, and a conversion that assigns keys onto a fresh
/// object one at a time takes the other path — `obj.__proto__ = x` invokes the
/// setter and changes the object's prototype. Letting a payload do that is
/// prototype pollution, so the property is worth a test of its own.
#[tokio::test]
async fn a_proto_key_in_the_payload_does_not_reach_the_prototype() {
    let data = r#"{"__proto__":{"polluted":"yes"},"normal":1}"#;

    // Nothing leaked onto Object.prototype.
    assert_eq!(
        ask("({}).polluted === undefined", data).await,
        "true",
        "a payload must not be able to write to Object.prototype"
    );
    // And the payload object itself did not have its prototype swapped.
    assert_eq!(
        ask("Object.getPrototypeOf(data) === Object.prototype", data).await,
        "true"
    );
    // The key is present as plain data, which is what JSON.parse does.
    assert_eq!(
        ask("Object.prototype.hasOwnProperty.call(data, '__proto__')", data).await,
        "true"
    );
    assert_eq!(ask("data.normal", data).await, "1");
}

#[tokio::test]
async fn a_constructor_key_is_also_just_data() {
    let data = r#"{"constructor":{"prototype":{"polluted":"yes"}}}"#;
    assert_eq!(ask("({}).polluted === undefined", data).await, "true");
}

// ── the other payload shapes still work ──────────────────────────────────────

#[tokio::test]
async fn bytes_arrive_as_a_uint8array() {
    let engine = probe("data instanceof Uint8Array ? data.length + ':' + data[0] : 'wrong'");

    let out = engine
        .render_with_bytes("/probe", vec![7u8, 8, 9])
        .await
        .unwrap();
    assert_eq!(out, "3:7");
}

#[tokio::test]
async fn json_and_bytes_arrive_side_by_side() {
    let engine = probe(
        "data.city + '|' + (bytes instanceof Uint8Array ? bytes.length : -1) + '|' + bytes[1]",
    );

    let out = engine
        .render_with_json_and_bytes("/probe", r#"{"city":"Blumenau"}"#, vec![1u8, 2, 3, 4])
        .await
        .unwrap();
    assert_eq!(out, "Blumenau|4|2");
}

/// The envelope in the combined shape is validated too — a bad one must not
/// reach the bundle just because the bytes beside it were fine.
#[tokio::test]
async fn a_malformed_envelope_beside_good_bytes_is_refused() {
    let engine = probe("'unreachable'");

    let err = engine
        .render_with_json_and_bytes("/probe", "{nope", vec![1, 2, 3])
        .await
        .expect_err("a malformed envelope must be refused");
    assert!(err.to_string().contains("Invalid JSON data"), "got: {err}");
}

/// An empty byte payload is a legitimate thing to send.
#[tokio::test]
async fn empty_bytes_are_accepted() {
    let engine = probe("data instanceof Uint8Array ? 'len' + data.length : 'wrong'");
    let out = engine.render_with_bytes("/probe", Vec::new()).await.unwrap();
    assert_eq!(out, "len0");
}
