//! AST-seam regression tests that need whole dialects, so they live here
//! rather than beside the types in `asm198x-ir`.

/// AE6 — pasmo and sjasmplus lower an equivalent program to identical bytes
/// (they share the AST lowering; full structural equality is proven by the
/// reference differential in `tests/conformance.rs`).
#[test]
fn pasmo_and_sjasmplus_share_the_lowering() {
    let src = "  ld a, 5\n  ld b, a\n  ret\n";
    let p = crate::assemble_pasmo(src).expect("assembles");
    let s = crate::assemble_sjasmplus(src).expect("assembles");
    assert_eq!(p.bytes, s.bytes);
}

/// U3 regression — sjasmplus scoped locals still resolve through the AST
/// refactor (two `.loop`s in two scopes assemble), and pasmo still rejects a
/// reused `.loop` (its non-scoping meaning is preserved).
#[test]
fn scoped_locals_survive_the_ast_refactor() {
    let src = "\
first:
  ld b, 2
.loop:
  djnz .loop
second:
  ld b, 3
.loop:
  djnz .loop
";
    assert!(crate::assemble_sjasmplus(src).is_ok(), "sjasmplus scopes");
    assert!(
        crate::assemble_pasmo(src).is_err(),
        "pasmo still rejects reused `.loop`"
    );
}

/// KTD6 seam — a non-migrated CPU (6502 via acme) still assembles through
/// the direct-lowering path; it never touches the AST.
#[test]
fn seam_leaves_other_cpus_on_direct_lowering() {
    let src = "*=$c000\n  lda #$05\n  rts\n";
    assert!(crate::assemble_acme(src).is_ok(), "6502 direct path intact");
}
