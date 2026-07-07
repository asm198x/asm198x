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

/// The serialized payload carries the contract version (R7/U5), and a payload
/// with no `version` (an older producer's) still loads, defaulting to the current
/// version — the additive-versioning promise.
#[test]
fn payload_carries_version_and_defaults_when_absent() {
    let result: AssemblyResult =
        asm198x::assemble_acme("* = $0400\n    nop\n").expect("acme assembles");
    assert_eq!(
        result.version,
        asm198x::CONTRACT_VERSION,
        "producer stamps it"
    );

    let mut value: serde_json::Value = serde_json::to_value(&result).expect("to value");
    assert_eq!(
        value.get("version").and_then(serde_json::Value::as_u64),
        Some(u64::from(asm198x::CONTRACT_VERSION)),
        "the version is present in the serialized payload"
    );

    // Drop the version, as an older producer would omit it: it defaults back.
    value
        .as_object_mut()
        .expect("result is a JSON object")
        .remove("version");
    let restored: AssemblyResult =
        serde_json::from_value(value).expect("a version-less payload still loads");
    assert_eq!(
        restored.version,
        asm198x::CONTRACT_VERSION,
        "absent version defaults to the current one"
    );
}

// --- U2: diagnostics ---

/// A real assemble failure converts to a public `Diagnostic` that keeps the
/// error's line and message (the failure path U4's JSON mode will emit).
#[test]
fn assemble_error_becomes_diagnostic() {
    let err = asm198x::assemble_acme("\n    frob $10\n").expect_err("unknown mnemonic");
    let d = asm198x::Diagnostic::from(err);
    assert_eq!(d.severity, asm198x::Severity::Error);
    let span = d.span.expect("a line-granular span");
    assert_eq!(span.line, 2, "the error's source line is preserved");
    assert!(!d.message.is_empty(), "the message is carried verbatim");
}

/// The verdict pipeline records a foreign-tool rejection as the thin shared
/// envelope — severity + verbatim text — with none of asm198x's rich fields
/// (R6, AE4).
#[test]
fn foreign_rejection_builds_a_thin_envelope() {
    let env = asm198x::DiagnosticEnvelope::new(
        asm198x::Severity::Error,
        "ca65: error: 'foo' is not a valid mnemonic",
    );
    assert_eq!(env.severity, asm198x::Severity::Error);
    assert!(
        env.message.contains("ca65"),
        "the reference tool's text is kept verbatim"
    );
}

/// A `Diagnostic` serialises to JSON and back to an identical value — the
/// contract's error side is machine-readable too (R2/R3 groundwork).
#[test]
fn diagnostic_json_round_trip() {
    let err = asm198x::assemble_acme("\n    frob $10\n").expect_err("err");
    let d = asm198x::Diagnostic::from(err);
    let json = serde_json::to_string(&d).expect("serialise");
    let back: asm198x::Diagnostic = serde_json::from_str(&json).expect("deserialise");
    assert_eq!(d, back, "diagnostic JSON round-trip is identity");
}

// --- U3: column-accurate spans (AE2) ---

/// AE2: an out-of-range operand in a 6502/acme program yields a diagnostic
/// whose `col` points at the operand token, not the line start.
#[test]
fn acme_out_of_range_operand_reports_operand_column() {
    // Line 2 is `    lda #$1ff` — the operand `#$1ff` starts at column 9.
    let err =
        asm198x::assemble_acme("* = $0800\n    lda #$1ff\n").expect_err("511 overflows a byte");
    let d = asm198x::Diagnostic::from(err);
    let span = d.span.expect("the diagnostic carries a span");
    assert_eq!(span.line, 2, "the error's source line is preserved");
    assert_eq!(span.col, 9, "`col` points at the operand token `#$1ff`");
}

/// AE2: an out-of-range branch in a Z80 (pasmo) program yields a diagnostic
/// whose `col` points at the operand token.
#[test]
fn z80_out_of_range_branch_reports_operand_column() {
    // Line 2 is `    jr far` — the operand `far` starts at column 8.
    let src = "    org 0\n    jr far\n    ds 200\nfar:\n";
    let err = asm198x::assemble_pasmo(src).expect_err("jr target beyond -128..=127");
    let d = asm198x::Diagnostic::from(err);
    let span = d.span.expect("the diagnostic carries a span");
    assert_eq!(span.line, 2, "the error's source line is preserved");
    assert_eq!(span.col, 8, "`col` points at the operand token `far`");
}

/// A field-packed CPU's operand error stays line-granular — its dialect does
/// not yet populate operand spans (contract KTD1: documented, not a
/// regression; column accuracy arrives with an AST-span migration).
#[test]
fn field_packed_error_stays_line_granular() {
    let err =
        asm198x::assemble_pdp11("    mov #70000, r0\n").expect_err("immediate overflows a word");
    let d = asm198x::Diagnostic::from(err);
    let span = d.span.expect("a line-granular span");
    assert_eq!(span.line, 1, "the error's source line is preserved");
    assert_eq!(
        span.col, 0,
        "no operand column — the diagnostic is line-granular"
    );
}

/// One AE2 case: a dialect name, its assemble entry point, a program whose
/// operand is out of range, and the expected (line, col) of the diagnostic.
type ColumnCase = (
    &'static str,
    fn(&str) -> Result<AssemblyResult, asm198x::AsmError>,
    &'static str,
    u32,
    u32,
);

/// AE2 across the remaining span-carrying dialects: each of the six other
/// AST-routed front-ends (8080, 6800, 1802, SC/MP, rgbasm, lwasm) reports the
/// operand-field column on an out-of-range operand, not the line start.
#[test]
fn every_span_carrying_dialect_reports_operand_column() {
    let cases: [ColumnCase; 6] = [
        (
            "i8080",
            asm198x::assemble_i8080,
            "        org 0\n        mvi a, 300H\n",
            2,
            13,
        ),
        (
            "m6800",
            asm198x::assemble_m6800,
            "        org 0\n        ldaa #$1ff\n",
            2,
            14,
        ),
        (
            "cdp1802",
            asm198x::assemble_1802,
            "        org 0\n        ldi 300H\n",
            2,
            13,
        ),
        ("scmp", asm198x::assemble_scmp, "        ldi 0x1ff\n", 1, 13),
        (
            "rgbasm",
            asm198x::assemble_rgbasm,
            "SECTION \"a\", ROM0\n        ld a, 300\n",
            2,
            12,
        ),
        (
            "lwasm",
            asm198x::assemble_lwasm,
            "        org 0\n        ldb #$1ff\n",
            2,
            13,
        ),
    ];
    for (dialect, assemble, src, line, col) in cases {
        let err = assemble(src)
            .err()
            .unwrap_or_else(|| panic!("{dialect}: program should fail"));
        let d = asm198x::Diagnostic::from(err);
        let span = d
            .span
            .unwrap_or_else(|| panic!("{dialect}: diagnostic carries a span"));
        assert_eq!(span.line, line, "{dialect}: source line");
        assert_eq!(span.col, col, "{dialect}: operand column");
    }
}

/// AE2 for acme's labelled-line path: the operand column is measured past the
/// label, so `loop: lda #$1ff` points at `#$1ff`, not at the label or line start.
#[test]
fn acme_labeled_line_reports_operand_column() {
    let err =
        asm198x::assemble_acme("* = $0800\nloop: lda #$1ff\n").expect_err("511 overflows a byte");
    let d = asm198x::Diagnostic::from(err);
    let span = d.span.expect("the diagnostic carries a span");
    assert_eq!(span.line, 2);
    assert_eq!(span.col, 11, "`col` points at `#$1ff`, past the label");
}

/// AE2 for acme's inline conditional body — the one caller that hands the parse
/// a mid-line slice. The column stays file-accurate (measured from the original
/// line start, not the body start).
#[test]
fn acme_inline_conditional_body_reports_file_accurate_column() {
    let err = asm198x::assemble_acme("* = $0800\n!if 1 { lda #$1ff }\n")
        .expect_err("511 overflows a byte");
    let d = asm198x::Diagnostic::from(err);
    let span = d.span.expect("the diagnostic carries a span");
    assert_eq!(span.line, 2);
    assert_eq!(
        span.col, 13,
        "`col` is measured from the line start, not the `{{`"
    );
}

// --- language-surface U1: the FileId→path table (KTD2) + the span path leg ---

/// A populated `files` table (index ⇔ `FileId`, entry 0 = the root input)
/// serialises into the JSON payload and round-trips to an identical value.
#[test]
fn files_table_serialises_and_round_trips() {
    let mut result: AssemblyResult =
        asm198x::assemble_acme("* = $0400\n    nop\n").expect("acme assembles");
    result.files = vec!["main.s".to_string(), "inc/defs.inc".to_string()];

    let value: serde_json::Value = serde_json::to_value(&result).expect("to value");
    assert_eq!(
        value.get("files"),
        Some(&serde_json::json!(["main.s", "inc/defs.inc"])),
        "the file table is present, in FileId order"
    );

    let restored: AssemblyResult = serde_json::from_value(value).expect("from value");
    assert_eq!(result, restored, "files round-trip is identity");
}

/// An older payload with no `files` (a pre-multi-file producer's) still loads —
/// the field is additive, defaulting to empty (the U5 version-default pattern).
#[test]
fn payload_without_files_still_deserialises() {
    let result: AssemblyResult =
        asm198x::assemble_acme("* = $0400\n    nop\n").expect("acme assembles");
    let mut value: serde_json::Value = serde_json::to_value(&result).expect("to value");
    value
        .as_object_mut()
        .expect("result is a JSON object")
        .remove("files");
    let restored: AssemblyResult =
        serde_json::from_value(value).expect("a files-less payload still loads");
    assert!(
        restored.files.is_empty(),
        "absent `files` defaults to empty"
    );
}

/// A span serialises without the resolved-path field when unresolved (skipped
/// when `None`, so existing fixtures stay valid), carries it once resolved from
/// a file table, and round-trips identically either way (KTD2's failure-path
/// leg: a bare Diagnostic-array consumer can name the file with no table).
#[test]
fn span_json_round_trips_with_and_without_path() {
    use asm198x::{FileId, Span};

    let bare = Span::in_file(FileId(1), 12, 8);
    let json = serde_json::to_string(&bare).expect("serialise bare span");
    assert!(
        !json.contains("\"path\""),
        "an unresolved span omits `path`: {json}"
    );
    let back: Span = serde_json::from_str(&json).expect("deserialise bare span");
    assert_eq!(bare, back, "bare round-trip is identity");

    let files = vec!["main.s".to_string(), "that-file.inc".to_string()];
    let resolved = asm198x::resolve_span_path(bare, &files);
    assert_eq!(
        resolved.path.as_deref(),
        Some("that-file.inc"),
        "the helper resolves the span's file from the table"
    );
    let json = serde_json::to_string(&resolved).expect("serialise resolved span");
    assert!(json.contains("that-file.inc"), "path serialises: {json}");
    let back: Span = serde_json::from_str(&json).expect("deserialise resolved span");
    assert_eq!(resolved, back, "resolved round-trip is identity");

    // An older payload (no `path` at all) still loads, path defaulting to None.
    let mut value: serde_json::Value = serde_json::to_value(&resolved).expect("to value");
    value
        .as_object_mut()
        .expect("span is a JSON object")
        .remove("path");
    let older: Span = serde_json::from_value(value).expect("a path-less span still loads");
    assert_eq!(older.path, None, "absent `path` defaults to None");
}

/// A `FileId` outside the table leaves the span unresolved rather than
/// panicking — the helper is total over malformed input.
#[test]
fn resolve_span_path_out_of_table_stays_unresolved() {
    use asm198x::{FileId, Span};
    let span = asm198x::resolve_span_path(Span::in_file(FileId(9), 1, 1), &["main.s".to_string()]);
    assert_eq!(span.path, None);
}
