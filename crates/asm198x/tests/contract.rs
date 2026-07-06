//! The core-contract result shape (plan U1). These exercise the one structured
//! `AssemblyResult` every entry point returns, its flat-vs-linked distinction
//! (`origin: Some` vs `None`), and its serde round-trip — the R1 contract. They
//! use the native assemblers (no reference tools), so they run in the normal
//! suite.

use asm198x::AssemblyResult;

/// A clean 6502 (acme) assemble returns bytes + symbols + a real flat load
/// origin (AE1 clean path, AE6).
#[test]
fn acme_returns_flat_result_with_symbols() {
    let src = "* = $0200\nstart:\n    lda #$01\n    rts\n";
    let r: AssemblyResult = asm198x::assemble_acme(src).expect("acme assembles");
    assert_eq!(
        r.origin,
        Some(0x0200),
        "a flat 6502 assemble carries its `*=` origin"
    );
    assert!(
        !r.bytes.is_empty(),
        "the flat image carries the assembled bytes"
    );
    assert_eq!(r.symbols.get("start"), Some(&0x0200), "symbols are exposed");
}

/// A ca65 assemble links a `.nes` ROM, whose flat origin is meaningless — its
/// `origin` is `None` (a linked image), not a fabricated `0` forced into the
/// flat shape (R1).
#[test]
fn ca65_returns_linked_image_with_no_origin() {
    let src = ".segment \"CODE\"\n    lda #$01\n    rts\n";
    let r: AssemblyResult = asm198x::assemble_ca65(src).expect("ca65 links");
    assert_eq!(r.origin, None, "a linked ROM has no single flat origin");
    assert!(!r.bytes.is_empty(), "the ROM image carries bytes");
}

/// A second, unrelated CPU returns the very same result shape with no per-CPU
/// handling — the shape is CPU-agnostic (R8, AE6).
#[test]
fn second_cpu_returns_same_shape() {
    let r: AssemblyResult = asm198x::assemble_pasmo("ld a, 0\n").expect("pasmo assembles");
    assert!(r.origin.is_some());
    assert!(!r.bytes.is_empty());
}

/// A vasm-warned assemble carries its non-fatal advisories in the unified
/// `warnings` field of the one shape (replacing the old tuple return).
#[test]
fn vasm_warned_carries_warnings_in_the_unified_shape() {
    let r: AssemblyResult =
        asm198x::assemble_vasm_warned("\tandi #$1234,ccr\n").expect("vasm assembles");
    // The oversize immediate to CCR is a vasm advisory, surfaced on the result.
    assert!(
        !r.warnings.is_empty(),
        "the CCR oversize advisory rides the unified warnings field"
    );
    assert_eq!(r.origin, None, "a vasm linked image has no flat origin");
}

/// The result serialises to JSON and back to an identical value — the contract
/// is machine-readable and its round-trip is lossless (AE7 mechanism; the
/// version/skip-unknown checks land in U5).
#[test]
fn assembly_result_json_round_trip_is_identity() {
    let original: AssemblyResult =
        asm198x::assemble_acme("* = $c000\n    lda #$00\n    rts\n").expect("acme assembles");
    let json = serde_json::to_string(&original).expect("serialises to JSON");
    let restored: AssemblyResult = serde_json::from_str(&json).expect("deserialises from JSON");
    assert_eq!(original, restored, "JSON round-trip is identity");
}

/// An unknown extra field is ignored on deserialise (serde's default
/// skip-unknown), so a newer producer's payload still loads — the additive
/// posture the versioning work (U5) builds on.
#[test]
fn unknown_fields_are_skipped_on_deserialise() {
    let original: AssemblyResult =
        asm198x::assemble_acme("* = $0400\n    nop\n").expect("acme assembles");
    let mut value: serde_json::Value = serde_json::to_value(&original).expect("to value");
    value
        .as_object_mut()
        .expect("result is a JSON object")
        .insert("a_future_field".into(), serde_json::json!("ignored"));
    let restored: AssemblyResult =
        serde_json::from_value(value).expect("unknown field is skipped, not rejected");
    assert_eq!(
        original, restored,
        "skipping the unknown field preserves the value"
    );
}
