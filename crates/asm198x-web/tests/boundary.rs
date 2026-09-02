//! The surface as JavaScript sees it, run under the wasm32 target by
//! `wasm-pack test --node` — a host `cargo test` registers none of these.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use wasm_bindgen_test::wasm_bindgen_test;

/// A successful assembly is the CLI's `AssemblyResult` object: the bytes the
/// reference assembler produces, under the contract version the CLI prints.
#[wasm_bindgen_test]
fn a_program_assembles_to_the_contract_object() {
    let json =
        asm198x_web::assemble("sjasmplus", " org 32768\n ld a,1\n ret\n").expect("a dialect");
    let value: serde_json::Value = serde_json::from_str(&json).expect("json");
    assert_eq!(value["bytes"], serde_json::json!([0x3E, 1, 0xC9]));
    assert_eq!(value["origin"], 32768);
    assert_eq!(value["version"], asm198x::CONTRACT_VERSION);
}

/// A failure is the diagnostics array, its span naming the placeholder root
/// path — the same array `asm198x --json` prints.
#[wasm_bindgen_test]
fn a_bad_line_is_the_diagnostics_array() {
    let json = asm198x_web::assemble("acme", " lda #$1234\n").expect("a dialect");
    let value: serde_json::Value = serde_json::from_str(&json).expect("json");
    let diagnostics = value.as_array().expect("an array");
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0]["severity"], "Error");
    assert!(diagnostics[0]["message"].as_str().is_some());
    assert_eq!(diagnostics[0]["span"]["path"], "input.asm");
}

/// An include has nowhere to resolve from: the loader is empty by design, and
/// the page learns that from a diagnostic, not a trap.
#[wasm_bindgen_test]
fn an_include_is_a_diagnostic_not_a_trap() {
    let json = asm198x_web::assemble("acme", " !src \"other.asm\"\n").expect("a dialect");
    let value: serde_json::Value = serde_json::from_str(&json).expect("json");
    assert!(value.is_array(), "diagnostics, got {json}");
}

#[wasm_bindgen_test]
fn an_alias_selects_its_dialect_and_nonsense_is_null() {
    assert!(asm198x_web::assemble("6502", " nop\n").is_some());
    assert!(asm198x_web::assemble("cobol", " nop\n").is_none());
}

#[wasm_bindgen_test]
fn the_listing_shows_address_bytes_and_source() {
    let text = asm198x_web::listing("pasmo", " org 100h\n nop\n").expect("assembles");
    assert!(text.contains("0100"), "address column: {text}");
    assert!(text.contains("00"), "bytes column: {text}");
    assert!(text.contains("nop"), "source column: {text}");
    assert!(asm198x_web::listing("pasmo", " bogus\n").is_none());
}

/// The picker list is the dialect table, first row first.
#[wasm_bindgen_test]
fn dialects_is_the_table_in_order() {
    let value: serde_json::Value = serde_json::from_str(&asm198x_web::dialects()).expect("json");
    let rows = value.as_array().expect("an array");
    assert_eq!(rows.len(), asm198x::dialect_table::DIALECTS.len());
    assert_eq!(rows[0]["name"], "acme");
    assert_eq!(rows[0]["aliases"][0], "6502");
    assert!(rows[0]["blurb"].as_str().is_some_and(|b| !b.is_empty()));
}
