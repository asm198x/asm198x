//! Source-direction differential: does asm198x accept what the reference tool does?
//!
//! The [`conformance`](tests/conformance) audit is *disassembler-anchored* — it
//! synthesises bytes, disassembles them, and checks the **encode** direction. By
//! construction it can never catch the opposite failure: **source the reference
//! assembler accepts but asm198x rejects** (an unparsed operator, a missing
//! mnemonic, an addressing-mode syntax we don't handle). Every parser gap logged
//! against this project belongs to that class, and it slips past the sweep.
//!
//! This audit closes the gap. Each [`Probe`] is a snippet of source; we assemble
//! it with **our** library entry point and, in parallel, with the **reference**
//! tool, then require: *if the reference accepts it, so must we, byte-for-byte.*
//! The reference gets an origin prefix so it loads at `$0000` (our fixed origin);
//! a probe the reference rejects is out of scope (skipped, not a failure).
//!
//! A probe carries an optional `gap: Some(issue)` marker — a **known** parser gap
//! tracked by that GitHub issue, expected to fail today. The test stays green
//! while gaps are open, but the ledger is kept honest two ways:
//!   * a probe with **no** marker that stops matching is a **regression**;
//!   * a probe **with** a marker that starts matching means the bug is fixed —
//!     the test fails asking you to delete the marker, so the list can't rot.
//!
//! `#[ignore]`d like the other cross-checks — it shells out to the reference
//! assemblers. Run:
//!
//! ```text
//! cargo test --test differential -- --ignored --nocapture
//! ```

use std::fs;
use std::path::Path;

mod support;

use std::process::Command;
use verdict_corpus::Suite;

fn have(bin: &str) -> bool {
    Command::new(bin).output().is_ok()
}

/// One source snippet, assembled by both sides.
struct Probe {
    /// `acme` | `pasmo` | `sjasmplus` | `lwasm` | `vasm` | `ca65-816`.
    dialect: &'static str,
    note: &'static str,
    /// Source body — no origin directive (we assemble at `$0000`; the reference
    /// gets the origin it needs prepended).
    body: &'static str,
    /// `Some(issue)` if this is a *known* parser gap tracked by that issue and
    /// expected to fail today; `None` if it must pass (a regression guard).
    gap: Option<u32>,
}

const fn ok(dialect: &'static str, note: &'static str, body: &'static str) -> Probe {
    Probe {
        dialect,
        note,
        body,
        gap: None,
    }
}
// The open ledger: U8's sjasmplus conditional forms are closed (#67) — ELSEIF
// chains and the dotted spellings are adopted; the two that remain were not
// conditional syntax at all and carry their own issues, `:` as a statement
// separator (#98) and multi-pass forward-symbol conditions (#99). Earlier
// batches (acme
// `!pet`/`!align`/`!zone`/`!set`, ca65 `.dword`/`.dbyt`/`.asciiz`, sjasmplus
// `byte`, lwasm `fill`/`zmb`/`fqb` — issue #26) are closed.
const fn gap(dialect: &'static str, note: &'static str, body: &'static str, issue: u32) -> Probe {
    Probe {
        dialect,
        note,
        body,
        gap: Some(issue),
    }
}

/// The reference tool for a dialect (also the PATH gate).
fn tool(dialect: &str) -> &'static str {
    match dialect {
        "acme" => "acme",
        "pasmo" => "pasmo",
        "sjasmplus" => "sjasmplus",
        "z80n" => "sjasmplus",
        "lwasm" => "lwasm",
        "vasm" => "vasmm68k_mot",
        // The 68000 multipass path (U6): the flat (-Fbin) and hunk-exe legs.
        "vasm-exe" => "vasmm68k_mot",
        "ca65-816" => "ca65",
        "ca65-huc6280" => "ca65",
        // The NES assemble+link path (U5): ca65 + ld65 with the fixed nes.cfg.
        "ca65-nes" => "ca65",
        "rgbasm" => "rgbasm",
        // The asl chips (U4): one arbiter for the family (asl + p2bin).
        "8080" | "tms9900" | "cp1610" => "asl",
        other => panic!("no reference tool for dialect `{other}`"),
    }
}

/// Assemble `body` with the reference tool at origin `$0000`; `None` if it
/// rejects the source (out of scope).
fn reference(tmp: &Path, dialect: &str, body: &str) -> Option<Vec<u8>> {
    let out = tmp.join("ref.out");
    let _ = fs::remove_file(&out);
    let run = |cmds: Vec<Command>| -> Option<()> {
        for mut c in cmds {
            if !c.current_dir(tmp).output().ok()?.status.success() {
                return None;
            }
        }
        Some(())
    };
    match dialect {
        "acme" => {
            let src = tmp.join("ref.a");
            fs::write(&src, format!("* = $0000\n{body}")).ok()?;
            let mut c = Command::new("acme");
            c.args(["-f", "cbm", "-o"]).arg(&out).arg(&src);
            run(vec![c])?;
            // acme `cbm` output is a 2-byte load address then the data.
            let r = fs::read(&out).ok()?;
            (r.len() >= 2).then(|| r[2..].to_vec())
        }
        "pasmo" => {
            let src = tmp.join("ref.z80");
            fs::write(&src, body).ok()?;
            let mut c = Command::new("pasmo");
            c.arg(&src).arg(&out);
            run(vec![c])?;
            fs::read(&out).ok()
        }
        "sjasmplus" | "z80n" => {
            // Z80N opcodes are gated behind a device selection in sjasmplus.
            let src = tmp.join("ref.asm");
            let source = if dialect == "z80n" {
                format!("\tDEVICE ZXSPECTRUMNEXT\n{body}")
            } else {
                body.to_string()
            };
            fs::write(&src, source).ok()?;
            let mut c = Command::new("sjasmplus");
            c.arg("--nologo")
                .arg(format!("--raw={}", out.display()))
                .arg(&src);
            run(vec![c])?;
            fs::read(&out).ok()
        }
        "lwasm" => {
            let src = tmp.join("ref.asm");
            fs::write(&src, body).ok()?;
            let mut c = Command::new("lwasm");
            c.args(["--6809", "--raw", "-o"]).arg(&out).arg(&src);
            run(vec![c])?;
            fs::read(&out).ok()
        }
        "vasm" => {
            let src = tmp.join("ref.s");
            fs::write(&src, body).ok()?;
            let mut c = Command::new("vasmm68k_mot");
            c.args(["-Fbin", "-no-opt", "-quiet", "-o"])
                .arg(&out)
                .arg(&src);
            run(vec![c])?;
            fs::read(&out).ok()
        }
        // rgbasm assembles to an object file that rgblink turns into a binary,
        // like ca65/ld65 below. `-x` keeps the image unpadded so the bytes are
        // the program's and not a ROM's trailing fill.
        "rgbasm" => {
            let src = tmp.join("ref.asm");
            let obj = tmp.join("ref.o");
            fs::write(&src, body).ok()?;
            let _ = fs::remove_file(&obj);
            let mut a = Command::new("rgbasm");
            a.arg("-o").arg(&obj).arg(&src);
            let mut l = Command::new("rgblink");
            l.arg("-x").arg("-o").arg(&out).arg(&obj);
            run(vec![a, l])?;
            fs::read(&out).ok()
        }
        "ca65-816" => {
            let src = tmp.join("ref.s");
            let obj = tmp.join("ref.o");
            let cfg = tmp.join("flat816.cfg");
            fs::write(
                &cfg,
                "MEMORY { MAIN: start=$0000, size=$10000, fill=no, file=%O; }\n\
                 SEGMENTS { CODE: load=MAIN, type=ro; }\n",
            )
            .ok()?;
            fs::write(&src, format!(".p816\n.segment \"CODE\"\n{body}")).ok()?;
            let _ = fs::remove_file(&obj);
            let mut a = Command::new("ca65");
            a.args(["--cpu", "65816"]).arg(&src).arg("-o").arg(&obj);
            let mut l = Command::new("ld65");
            l.arg("-C").arg(&cfg).arg(&obj).arg("-o").arg(&out);
            run(vec![a, l])?;
            fs::read(&out).ok()
        }
        _ => None,
    }
}

/// Assemble `body` with our library; `None` if we reject it.
fn ours(dialect: &str, body: &str) -> Option<Vec<u8>> {
    // Framing lives in `support::verdicts` so live arbitration and the tool-free
    // replay assemble a probe identically. Two copies drifting by one line would
    // make every replay lookup miss and leave that suite green while checking
    // nothing (#61).
    support::verdicts::assemble_probe(dialect, body)?.ok()
}

/// Which CPU's corpus a probe's verdict belongs in. `z80n` is kept apart from
/// `Z80`: the Next's extension is a different instruction set, and filing its
/// facts under the base CPU would make the corpus claim more than it knows.
fn probe_cpu(dialect: &str) -> &'static str {
    match dialect {
        "acme" => "6502",
        "pasmo" | "sjasmplus" => "Z80",
        "z80n" => "Z80N",
        "lwasm" => "6809",
        "vasm" => "68000",
        "ca65-816" => "65816",
        "rgbasm" => "SM83",
        other => panic!("no corpus CPU for dialect `{other}`"),
    }
}

/// The probe corpus: a spread of forms each dialect handles today (regression
/// guards, `gap: None`) plus every known parser gap (`gap: Some(issue)`), so the
/// file doubles as a live ledger of the open front-end issues.
#[rustfmt::skip]
const PROBES: &[Probe] = &[
    // ---- acme / 6502 --------------------------------------------------------
    ok ("acme", "immediate + absolute",  " lda #$01\n sta $d020\n rts\n"),
    ok ("acme", "lo/hi byte < >",        " lda #<$1234\n ldx #>$1234\n"),
    ok ("acme", "indexed + indirect",    " lda ($10,x)\n sta ($10),y\n jmp ($1234)\n"),
    ok ("acme", "!byte / !word",         " !byte 1,2,3\n !word $1234\n"),
    ok ("acme", "binary literal %",      " lda #%1010\n"),
    ok ("acme", "operator &",            " lda #7&3\n"),
    ok ("acme", "operator |",            " lda #1|2\n"),
    ok ("acme", "operator ^ (power)",    " lda #5^3\n"),
    ok ("acme", "keyword XOR",           " lda #5 XOR 1\n"),
    ok ("acme", "keyword EOR",           " lda #5 EOR 1\n"),
    ok ("acme", "operator <<",           " lda #1<<3\n"),
    ok ("acme", "operator >>",           " lda #16>>2\n"),
    ok ("acme", "directive !pet",        " !pet \"hi\"\n"),
    ok ("acme", "directive !align",      " !align 255,0\n lda #1\n"),
    ok ("acme", "directive !zone",       " !zone main\n rts\n"),
    ok ("acme", "directive !set",        " !set n=5\n lda #n\n"),
    // U7: `!zone` scopes `.`-locals — reuse across named/bare zones, the
    // `{ … }` block form restoring the enclosing zone, a fresh scope on
    // re-entering a title, and zone-scoped constants driving `!ifdef`.
    ok ("acme", "zone scopes local reuse",
        "!zone one\n.loop   lda #1\n        bne .loop\n!zone\n.loop   lda #2\n        bne .loop\n"),
    ok ("acme", "zone block form restores the enclosing zone",
        ".out    lda #1\n!zone inner {\n.loop   lda #2\n        bne .loop\n}\n        bne .out\n"),
    ok ("acme", "zone title re-entry is a fresh scope",
        "!zone one\n.x      lda #1\n!zone two\n        lda #2\n!zone one\n.x      lda #3\n"),
    ok ("acme", "zone-scoped constant + !ifdef",
        "!zone one\n.flag = 1\n!ifdef .flag {\n        lda #1\n}\n!zone two\n!ifdef .flag {\n        lda #2\n}\n        nop\n"),
    ok ("acme", "locals cross globals within one zone",
        "first   lda #1\n        bne .later\nsecond  lda #2\n.later  rts\n"),
    // U4: an anon defined in an untaken `!if` branch does not exist — the
    // later `-` reference resolves to the live definition (the evaluation-order
    // collection; the old textual prescan failed this probe).
    ok ("acme", "anon skips untaken branch",
        "FLAG = 0\n-       lda #1\n!if FLAG {\n-       lda #2\n}\n        bne -\n"),

    // ---- pasmo / z80 --------------------------------------------------------
    ok ("pasmo", "hex $ / binary %",     " ld a,$10\n ld b,%1010\n"),
    ok ("pasmo", "ix/iy displacement",   " ld a,(ix+5)\n ld b,(iy-3)\n"),
    ok ("pasmo", "ld (nn),hl",           " ld ($1234),hl\n"),
    ok ("pasmo", "bit / set / im / rst", " bit 7,a\n set 0,(hl)\n im 1\n rst 38\n"),
    ok ("pasmo", "hex 0x prefix",        " ld a,0x10\n"),
    ok ("pasmo", "hex h suffix",         " ld a,10h\n"),
    ok ("pasmo", "binary b suffix",      " ld a,1010b\n"),
    ok ("pasmo", "octal o/q suffix",     " ld a,17o\n ld b,17q\n"),
    ok ("pasmo", "operator <<",          " ld a,1<<2\n"),
    ok ("pasmo", "operator &",           " ld a,5 & 3\n"),
    ok ("pasmo", "operator |",           " ld a,4 | 1\n"),
    ok ("pasmo", "operator >>",          " ld a,16 >> 2\n"),

    // ---- sjasmplus / z80 ----------------------------------------------------
    ok ("sjasmplus", "hex $ / 0x / h",   " ld a,$10\n ld b,0x10\n ld c,10h\n"),
    ok ("sjasmplus", "binary 0b / %",    " ld a,0b1010\n ld b,%1010\n"),
    ok ("sjasmplus", "db / dw / defb",   " db 1,2,3\n dw $1234\n defb 4\n"),
    ok ("sjasmplus", "hex # prefix",     " ld a,#10\n"),
    ok ("sjasmplus", "operator <<",      " ld a,1<<2\n"),
    ok ("sjasmplus", "operator &",       " ld a,5 & 3\n"),
    ok ("sjasmplus", "operator ^",       " ld a,6 ^ 3\n"),
    ok ("sjasmplus", "directive byte",   " byte 1,2\n"),
    // U8: conditional assembly + DEFINE (probe set u8-probes, sjasmplus 1.21.0).
    ok ("sjasmplus", "IF taken/untaken + ELSE",
        " IF 1\n ld a,1\n ELSE\n ld a,2\n ENDIF\n IF 0\n ld b,1\n ELSE\n ld b,2\n ENDIF\n"),
    ok ("sjasmplus", "IF comparisons + logicals",
        "V equ 5\n IF V = 5\n ld a,1\n ENDIF\n IF V == 5\n ld a,2\n ENDIF\n IF V > 3\n ld a,3\n ENDIF\n IF V != 4\n ld a,4\n ENDIF\n IF V && 0\n ld a,5\n ENDIF\n IF V || 0\n ld a,6\n ENDIF\n IF !V\n ld a,7\n ENDIF\n"),
    ok ("sjasmplus", "IF parenthesised logicals",
        "A1 equ 1\nB1 equ 0\n IF (A1 = 1) && (B1 = 0)\n ld a,1\n ENDIF\n IF !(A1 && B1)\n ld a,2\n ENDIF\n"),
    ok ("sjasmplus", "nested conditionals, lowercase",
        " if 1\n if 0\n ld a,1\n else\n ld a,2\n endif\n ifdef NOPE\n ld a,3\n endif\n endif\n"),
    ok ("sjasmplus", "nesting tracked while skipping",
        " IF 0\n IF 1\n ld a,1\n ENDIF\n ld a,2\n ENDIF\n ld a,3\n"),
    ok ("sjasmplus", "IFDEF namespace is DEFINEs only",
        " DEFINE DFLAG\nCONST equ 7\nLBL: nop\n IFDEF DFLAG\n ld a,1\n ENDIF\n IFDEF CONST\n ld a,2\n ENDIF\n IFDEF LBL\n ld a,3\n ENDIF\n IFNDEF NOPE\n ld a,4\n ENDIF\n"),
    ok ("sjasmplus", "skipped branch defines nothing",
        " IF 0\n DEFINE SKDEF\n ENDIF\n IFDEF SKDEF\n ld a,1\n ENDIF\n IFNDEF SKDEF\n ld a,2\n ENDIF\n"),
    ok ("sjasmplus", "DEFINE substitutes operands + lines",
        " DEFINE X 5\n ld a,X\n DEFINE Y ld b,2\n Y\n DEFINE N 3\n db N,N*2,\"N\"\n"),
    ok ("sjasmplus", "chained DEFINEs expand at use",
        " DEFINE A1 3\n DEFINE B1 A1+1\n db B1\n"),
    ok ("sjasmplus", "DEFINE renames a label definition",
        " DEFINE L mylab\nL: nop\n jr mylab\n"),
    ok ("sjasmplus", "equ in taken branch feeds bit/ds forms",
        " IF 1\nBITN equ 5\nPAD equ 2\n ENDIF\n bit BITN,a\n ds PAD\n ld a,1\n"),
    ok ("sjasmplus", "conditional keeps local scoping intact",
        "first:\n.l: nop\n IF 1\nsecond:\n.l: nop\n jr .l\n ENDIF\n jr .l\n"),
    ok ("sjasmplus", "label on the IF line binds",
        "lbl: IF 1\n ld a,1\n ENDIF\n jr lbl\n"),
    // #67, stage 1 (2026-08-18): ELSEIF chains and the dotted spellings are
    // adopted and arbitrated here.
    ok ("sjasmplus", "ELSEIF chain",
        "V equ 2\n IF V = 1\n ld a,1\n ELSEIF V = 2\n ld a,2\n ELSE\n ld a,3\n ENDIF\n"),
    ok ("sjasmplus", "dotted .IF/.ENDIF spelling",
        " .IF 1\n ld a,1\n .ENDIF\n"),
    ok ("sjasmplus", "dotted chain, lower case",
        " .if 0\n ld a,1\n .elseif 1\n ld a,2\n .endif\n"),
    // Re-filed out of #67 (2026-08-19): neither is conditional syntax. `:`
    // fails between plain instructions too, so it is a line-model change
    // (#98); the forward label is a resolution-order property needing the
    // reference's multi-pass convergence (#99).
    gap("sjasmplus", "colon-inline conditional",
        " IF 1 : ld a,1 : ENDIF\n", 98),
    gap("sjasmplus", "IF on a forward label (multi-pass)",
        " IF later\n ld a,1\n ENDIF\nlater: nop\n", 99),

    // Modules (#93's third item, 2026-08-23). The error cases — no walk-up, a
    // dotted module name, `ENDMODULE` with nothing open — are not here: this
    // harness skips a body the reference rejects, so they live in the unit
    // tests where the rejection itself is the assertion.
    ok ("sjasmplus", "module qualifies its labels",
        " MODULE foo\nbar: db 1\n ENDMODULE\n db foo.bar\n"),
    ok ("sjasmplus", "nested modules concatenate",
        " MODULE foo\n MODULE baz\nbar: db 1\n ENDMODULE\n ENDMODULE\n db foo.baz.bar\n"),
    ok ("sjasmplus", "reference falls back to the global",
        "top: db 9\n MODULE foo\n db top\n ENDMODULE\n"),
    ok ("sjasmplus", "qualified candidate shadows the global",
        "x equ $AA\n MODULE foo\nx equ $BB\n db x\n ENDMODULE\n"),
    ok ("sjasmplus", "@ escapes the module scope",
        " MODULE foo\n@bar: db 1\n ENDMODULE\n db bar\n MODULE baz\n db @bar\n ENDMODULE\n"),
    ok ("sjasmplus", "locals qualify under modules",
        " MODULE foo\nglob:\n.loc: db 1\n ENDMODULE\n db foo.glob.loc\n"),
    ok ("sjasmplus", "forward reference inside a module",
        " MODULE foo\n db bar\nbar equ $DD\n ENDMODULE\n db g\ng equ $EE\n"),
    ok ("sjasmplus", "a module may be reopened",
        " MODULE foo\nbar: db 1\n ENDMODULE\n MODULE foo\nbaz: db 2\n ENDMODULE\n \
         db foo.bar, foo.baz\n"),
    ok ("sjasmplus", "ENDMOD closes as well as ENDMODULE",
        " MODULE foo\nbar: db 1\n ENDMOD\n db foo.bar\n"),
    ok ("sjasmplus", "lowercase module spelling",
        " module foo\nbar: db 1\n endmodule\n db foo.bar\n"),
    ok ("sjasmplus", "a macro expands into the invoking module",
        " MACRO mk\nlbl: db 1\n ENDM\n MODULE foo\n mk\n ENDMODULE\n db foo.lbl\n"),
    ok ("sjasmplus", "DEFINE is not module-scoped",
        " MODULE foo\n DEFINE V 5\n ENDMODULE\n db V\n"),

    // #128 gaps 1 and 3 (2026-08-23). The comparison operators `<`, `>` and
    // `<>` were missing because the first two collide with the byte prefixes;
    // a one-character string is a value wherever acme wants a number.
    ok ("acme", "!if relational operators",
        "!if 5 > 3 {\n lda #1\n}\n!if 3 < 5 {\n lda #2\n}\n!if 5 <> 3 {\n lda #3\n}\n"),
    ok ("acme", "!if relations that are false",
        "!if 5 < 3 {\n lda #1\n}\n!if 3 > 5 {\n lda #2\n}\n!if 5 <> 5 {\n lda #3\n}\n lda #9\n"),
    ok ("acme", "a byte prefix is not a comparison",
        "!if <$1234 > 3 {\n lda #1\n}\n lda #<$1234\n lda #>$1234\n"),
    ok ("acme", "!if on a whole left expression",
        "!if 1 + 2 > 2 {\n lda #1\n}\n"),
    ok ("acme", "a one-character string is a value",
        " !byte \"a\"\n !byte (\"a\")\n !byte \"a\", \"b\"\n !word \"a\"\n lda #\"a\"\n lda \"a\"\n"),
    ok ("acme", "a bare string condition is testable",
        "!if \"a\" {\n lda #1\n}\n"),
    // #128 gap 3: the one that changed bytes rather than rejecting source.
    // A backward label with a low address sizes to zero page; a high one, a
    // forward one, and a forced-absolute literal do not.
    ok ("acme", "backward label sizes to zero page",
        "lbl lda #5\n lda lbl\n"),
    ok ("acme", "the counter follows data too",
        " !byte 1,2,3\nlbl lda #5\n lda lbl\n"),
    ok ("acme", "a forward label stays absolute",
        " lda fwd\nfwd lda #5\n"),
    ok ("acme", "zero-page sizing follows the mode",
        "lbl lda #5\n lda lbl,x\n lda lbl,y\n"),
    ok ("acme", "a 4-digit literal is 16-bit",
        "lbl lda #5\n lda $0000\n"),

    // ---- macros (#93) -------------------------------------------------------
    // sjasmplus and pasmo have macros; the rest are still gaps — the reference
    // accepts the body and we reject it. Recording a gap puts that reference's
    // *actual* macro output in the corpus, so implementing the form has ground
    // truth to build against rather than a reading of its manual — and the
    // marker fails the suite the moment the form starts working, which is when
    // it should be deleted. Both pasmo markers came out that way.
    //
    // Every body below was verified accepted by its reference before being
    // added. A body the reference rejects is silently skipped by this harness,
    // so an unverified probe would contribute nothing while looking like
    // coverage.
    //
    // The spellings do not converge, which is why #93 insists on per-dialect
    // fidelity rather than a house macro system:
    //   MACRO name  / ENDM      sjasmplus, pasmo, rgbasm
    //   name MACRO  / ENDM      asl, lwasm, vasm, pasmo, sjasmplus — all
    //                           implemented
    //   .macro name / .endmacro ca65 (implemented)
    //   !macro name { }         acme (implemented)
    ok ("acme", "macro definition and invocation",
        "!macro nop2 {\n\tnop\n\tnop\n}\n+nop2\n"),
    ok ("acme", "macro with a parameter",
        "!macro ldav .v {\n\tlda #.v\n}\n+ldav 5\n"),
    ok ("acme", "two parameters",
        "!macro ldav .v, .w {\n\tlda #.v\n\tldx #.w\n}\n+ldav 5, 7\n"),
    ok ("acme", "the keyword is case-insensitive",
        "!MACRO nop2 {\n\tnop\n}\n+nop2\n"),
    ok ("acme", "substitution precedes evaluation",
        "!macro ldav .v {\n\tlda #.v*2\n}\n+ldav 5\n"),
    // A `.dotted` label in a body is scoped to the expansion — sjasmplus's rule
    // exactly, and the only thing the five other dialects made look universal.
    ok ("acme", "a dotted label is scoped to its expansion",
        "!macro delay {\n.spin\tdex\n\tbne .spin\n}\n+delay\n+delay\n"),
    // The two structural properties. Braces nest inside a body, and both braces
    // may share a line with code — an acme body is delimited at character
    // level, not by a line the collector can recognise on its own.
    ok ("acme", "braces nest inside a body",
        "!macro m {\n\t!if 1 {\n\t\tnop\n\t}\n\tlda #1\n}\n+m\n"),
    ok ("acme", "both braces may share a line with code",
        "!macro nop2 { nop\n\tnop }\n+nop2\n"),
    ok ("acme", "a brace inside a string closes nothing",
        "!macro m {\n\t!text \"a}b\"\n\tnop\n}\n+m\n"),
    // Arity is part of the macro's identity: two definitions of one name
    // coexist, and the call site picks by count.
    ok ("acme", "one name may carry two arities",
        "!macro ldav .v {\n\tlda #.v\n}\n!macro ldav .v, .w {\n\tlda #.v\n\tldx #.w\n}\n+ldav 5\n+ldav 5, 7\n"),
    ok ("acme", "nested macro invocation",
        "!macro outer {\n\t+inner\n}\n!macro inner {\n\tnop\n}\n+outer\n"),
    ok ("acme", "an indented invocation is still an invocation",
        "!macro ldav .v {\n\tlda #.v\n}\n\t+ldav 5\n"),
    ok ("pasmo", "macro definition and invocation",
        " MACRO nop2\n nop\n nop\n ENDM\n nop2\n"),
    // pasmo wants a comma after the name before parameters, where sjasmplus
    // takes a space — the same keyword, a different grammar.
    ok ("pasmo", "macro with a parameter",
        " MACRO ldav, val\n ld a,val\n ENDM\n ldav 5\n"),
    // ...and it takes the definition the other way round too. So does
    // sjasmplus — the claim that it does not stood here unprobed until the
    // module work measured it, which is why nothing covered the form. Both are
    // arbitrated now (#205).
    ok ("pasmo", "macro defined name-first",
        "ldav MACRO val\n ld a,val\n ENDM\n ldav 5\n"),
    ok ("sjasmplus", "macro defined name-first",
        "ldav MACRO val\n ld a,val\n ENDM\n ldav 5\n"),
    ok ("sjasmplus", "macro defined name-first, colon",
        "ldav: MACRO val\n ld a,val\n ENDM\n ldav 5\n"),
    ok ("sjasmplus", "macro defined name-first, no params",
        "nop2 MACRO\n nop\n nop\n ENDM\n nop2\n"),
    // A macro with a loop is most of what macros are for. pasmo scopes nothing
    // by spelling: the label repeats cleanly only because `LOCAL` declares it.
    // The macro is called `delay`, not `m`: pasmo also knows the 8080 mnemonic
    // set, where `M` names `(HL)`, so a macro called `m` is never invoked — it
    // is `Unexpected 'M' used as instruction`.
    ok ("pasmo", "LOCAL label, invoked twice",
        "delay MACRO\n LOCAL spin\nspin djnz spin\n ENDM\n delay\n delay\n"),
    ok ("pasmo", "LOCAL label with a parameter",
        "delay MACRO v\n LOCAL spin\nspin djnz spin\n ld a,v\n ENDM\n delay 5\n delay 6\n"),
    // Substitution is textual, word-bounded and string-safe: `v` must not
    // touch `val` or the letter inside the string.
    ok ("pasmo", "substitution respects words and strings",
        "val equ 7\n MACRO m1, v\n ld a,v\n ld hl,val\n defb \"v\"\n ld a,v*2\n ENDM\n m1 9\n"),
    ok ("pasmo", "nested macro passes a parameter through",
        " MACRO inner, v\n ld a,v\n ENDM\n MACRO outer, v\n inner v\n inner v+1\n ENDM\n outer 3\n"),
    ok ("pasmo", "macro invokes one defined later",
        " MACRO outer\n inner\n ENDM\n MACRO inner\n nop\n ENDM\n outer\n"),
    // pasmo checks no arity: the extra argument is dropped rather than
    // rejected. Recorded because it is surprising, and because the quiet
    // alternative — rejecting it — would emit nothing where pasmo emits code.
    // A label in front of an invocation binds at the expansion's first address.
    // Missing this rejects the line outright — the label reads as the mnemonic
    // — so it is worth a probe in both dialects and in both spellings.
    ok ("pasmo", "label in front of an invocation",
        " MACRO m1, v\n ld a,v\n ENDM\nlbl: m1 9\n ld hl,lbl\n"),
    ok ("pasmo", "label without a colon in front of an invocation",
        " MACRO m1, v\n ld a,v\n ENDM\nlbl m1 9\n ld hl,lbl\n"),
    ok ("pasmo", "extra arguments are dropped",
        " MACRO m1, v\n ld a,v\n ENDM\n m1 1,2\n"),
    ok ("sjasmplus", "macro definition and invocation",
        " MACRO nop2\n nop\n nop\n ENDM\n nop2\n"),
    ok ("sjasmplus", "macro with a parameter",
        " MACRO ldav val\n ld a,val\n ENDM\n ldav 5\n"),
    // A macro with a loop is most of what macros are for, and it needs the
    // dot-local to be scoped per expansion or the second invocation collides.
    ok ("sjasmplus", "macro local label, invoked twice",
        " MACRO m\n.loc djnz .loc\n ENDM\n m\n m\n"),
    ok ("sjasmplus", "macro local label with a parameter",
        " MACRO m v\n.l djnz .l\n ld a,v\n ENDM\n m 5\n m 6\n"),
    // Composition: a macro may invoke another, and may invoke one defined
    // later in the file — the reference resolves names when it expands.
    ok ("sjasmplus", "nested macro invocation",
        " MACRO inner\n nop\n ENDM\n MACRO outer\n inner\n ENDM\n outer\n"),
    ok ("sjasmplus", "nested macro passes a parameter through",
        " MACRO inner v\n ld a,v\n ENDM\n MACRO outer w\n inner w\n ENDM\n outer 5\n"),
    ok ("sjasmplus", "label in front of an invocation",
        " MACRO m1 v\n ld a,v\n ENDM\nlbl: m1 9\n ld hl,lbl\n"),
    ok ("sjasmplus", "label without a colon in front of an invocation",
        " MACRO m1 v\n ld a,v\n ENDM\nlbl m1 9\n ld hl,lbl\n"),
    ok ("sjasmplus", "macro invokes one defined later",
        " MACRO outer\n inner\n ENDM\n MACRO inner\n nop\n ENDM\n outer\n"),
    // Repetition. The count is an expression over the environment, which is
    // why it is evaluated with the conditionals rather than expanded with the
    // macros.
    ok ("sjasmplus", "DUP repeats its body",
        " DUP 3\n nop\n EDUP\n"),
    ok ("sjasmplus", "REPT/ENDR is the same block",
        " REPT 3\n nop\n ENDR\n"),
    ok ("sjasmplus", "DUP count is an expression",
        "n equ 2\n DUP n+1\n nop\n EDUP\n"),
    ok ("sjasmplus", "DUP nests",
        " DUP 2\n DUP 2\n nop\n EDUP\n EDUP\n"),
    ok ("sjasmplus", "a macro inside DUP",
        " MACRO m\n nop\n ENDM\n DUP 2\n m\n EDUP\n"),
    ok ("sjasmplus", "DUP inside a macro",
        " MACRO m\n DUP 2\n nop\n EDUP\n ENDM\n m\n"),
    ok ("lwasm", "macro definition and invocation",
        "nop2\tmacro\n nop\n nop\n endm\n nop2\n"),
    ok ("lwasm", "macro with a positional parameter",
        "ldav\tmacro\n lda #\\1\n endm\n ldav 5\n"),
    ok ("lwasm", "two positional parameters",
        "ldav\tmacro\n lda #\\1\n ldb #\\2\n endm\n ldav 5,7\n"),
    // lwasm marks a local with a `?` or `@` **suffix** — a third spelling of
    // locals across four dialects, and one its own parser strips.
    ok ("lwasm", "a ? suffix scopes a label to its expansion",
        "delay\tmacro\nspin? deca\n bne spin?\n endm\n delay\n delay\n"),
    ok ("lwasm", "an @ suffix does the same",
        "delay\tmacro\nspin@ deca\n bne spin@\n endm\n delay\n delay\n"),
    // Arity is unchecked: the extra argument is dropped rather than rejected.
    ok ("lwasm", "extra arguments are dropped",
        "ldav\tmacro\n lda #\\1\n endm\n ldav 5,9\n"),
    ok ("vasm", "macro definition and invocation",
        "nop2\tmacro\n nop\n nop\n endm\n nop2\n"),
    ok ("vasm", "macro with a positional parameter",
        "ldav\tmacro\n move.l #\\1,d0\n endm\n ldav 5\n"),
    ok ("vasm", "two positional parameters",
        "ldav\tmacro\n move.l #\\1,d0\n move.l #\\2,d1\n endm\n ldav 5,7\n"),
    // vasm also takes the definition keyword-first, which lwasm rejects with
    // `Missing macro name`.
    ok ("vasm", "macro defined keyword-first",
        " macro nop2\n nop\n endm\n nop2\n"),
    // `\@` numbers the expansion, so `spin\@` is a fresh label each time. The
    // bodies below take the label's *address* rather than branching to it:
    // vasm's branch sizing under `-no-opt` is #110's business, not macros'.
    ok ("vasm", "the \\@ counter makes a label unique per expansion",
        "mk\tmacro\nspin\\@ nop\n move.l #spin\\@,d0\n endm\n mk\n mk\n"),
    ok ("vasm", "several \\@ names in one body stay distinct",
        "mk\tmacro\nspin\\@ nop\nother\\@ nop\n move.l #spin\\@,d0\n move.l #other\\@,d1\n endm\n mk\n mk\n"),
    ok ("vasm", "extra arguments are dropped",
        "ldav\tmacro\n move.l #\\1,d0\n endm\n ldav 5,9\n"),
    ok ("ca65-816", "macro definition and invocation",
        ".macro nop2\n nop\n nop\n.endmacro\n nop2\n"),
    ok ("ca65-816", "macro with a parameter",
        ".macro ldav v\n lda #v\n.endmacro\n ldav 5\n"),
    // ca65 takes a *space* after the name, like sjasmplus — `.macro m1, v` is
    // `Unexpected trailing garbage characters` — and the short spelling too.
    ok ("ca65-816", "macro parameters are comma-separated after a space",
        ".macro m1 v, w\n lda #v\n ldx #w\n.endmacro\n m1 9, 7\n"),
    ok ("ca65-816", "the .mac short spelling",
        ".mac nop2\n nop\n nop\n.endmac\n nop2\n"),
    ok ("ca65-816", "the keyword is case-insensitive",
        ".MACRO nop2\n nop\n.ENDMACRO\n nop2\n"),
    // Substitution is textual, word-bounded, string-safe, and runs before the
    // expression is evaluated: `val` survives, the quoted `v` is a letter.
    ok ("ca65-816", "substitution respects words and strings",
        "val = 7\n.macro m1 v\n lda #v\n lda val\n.byte \"v\"\n.endmacro\n m1 9\n"),
    ok ("ca65-816", "substitution precedes evaluation",
        ".macro m1 v\n lda #v*2\n.endmacro\n m1 5\n"),
    // `.local` is the only thing that scopes a label to an expansion; a plain
    // one is global and the second expansion gets `already defined`.
    ok ("ca65-816", ".local label, invoked twice",
        ".macro delay\n.local spin\nspin: dex\n bne spin\n.endmacro\n delay\n delay\n"),
    ok ("ca65-816", ".local declares several names",
        ".macro delay\n.local spin, done\nspin: dex\ndone: bne spin\n.endmacro\n delay\n delay\n"),
    ok ("ca65-816", "nested macro passes a parameter through",
        ".macro inner v\n lda #v\n.endmacro\n.macro outer v\n inner v\n inner v+1\n.endmacro\n outer 3\n"),
    ok ("ca65-816", "macro invokes one defined later",
        ".macro outer\n inner\n.endmacro\n.macro inner\n nop\n.endmacro\n outer\n"),
    ok ("ca65-816", "label in front of an invocation",
        ".macro m1 v\n lda #v\n.endmacro\nlbl: m1 9\n lda lbl\n"),
    // A third arity posture again: too many is an error, too few is not — the
    // missing parameter substitutes empty and only complains if reached.
    ok ("ca65-816", "a parameter that is never reached may be omitted",
        ".macro m1 v, w\n lda #v\n.endmacro\n m1 9\n"),
    ok ("ca65-816", "a multi-token argument substitutes whole",
        ".macro m1 v\n lda #v\n.endmacro\n m1 5+3\n"),
    ok ("ca65-816", "a string argument survives substitution",
        ".macro m1 v\n .byte v\n.endmacro\n m1 \"hi\"\n"),

    // ---- z80n (Spectrum Next extension ISA), sjasmplus reference -------------
    ok ("z80n", "swapnib / mirror",      " swapnib\n mirror a\n"),
    ok ("z80n", "barrel shifts",         " bsla de,b\n bsrl de,b\n brlc de,b\n"),
    ok ("z80n", "add rr,a / add rr,nn",  " add hl,a\n add de,a\n add hl,$1234\n"),
    ok ("z80n", "nextreg n,n / n,a",     " nextreg $12,$34\n nextreg $07,a\n"),
    ok ("z80n", "test n / outinb",       " test 5\n outinb\n"),
    ok ("z80n", "block loads",           " ldix\n ldirx\n lddx\n lddrx\n ldpirx\n ldws\n"),
    ok ("z80n", "pixel ops",             " pixeldn\n pixelad\n setae\n"),
    ok ("z80n", "push nn (big-endian)",  " push $1234\n"),
    ok ("z80n", "mul d,e mnemonic",      " mul d,e\n"),

    // ---- lwasm / 6809 -------------------------------------------------------
    ok ("lwasm", "indexed modes",        " lda ,x\n lda 5,y\n lda ,-u\n lda [,s++]\n"),
    ok ("lwasm", "tfr / exg / pshs",     " tfr a,b\n exg x,y\n pshs a,b,x\n"),
    ok ("lwasm", "fcb / fdb / fcc / rmb"," fcb 1,2\n fdb $1234\n fcc \"hi\"\n rmb 4\n"),
    ok ("lwasm", "abx / mul / sex",      " abx\n mul\n sex\n"),
    ok ("lwasm", "instruction andcc",    " andcc #$fe\n"),
    ok ("lwasm", "instruction orcc",     " orcc #1\n"),
    ok ("lwasm", "instruction cmpu",     " cmpu #$1234\n"),
    ok ("lwasm", "instruction cmps",     " cmps ,y\n"),
    ok ("lwasm", "instruction swi2",     " swi2\n"),
    ok ("lwasm", "instruction swi3",     " swi3\n"),
    ok ("lwasm", "directive fill",       " fill 0,4\n"),
    ok ("lwasm", "directive zmb",        " zmb 4\n"),
    ok ("lwasm", "directive fqb",        " fqb $12345678\n"),

    // Conditionals. lwasm compares against **zero** rather than taking a
    // boolean, so each spelling is its own comparison — and both `endc` and
    // `endif` close, which is one word more than any other dialect measured.
    ok ("lwasm", "ifne taken",           " ifne 1\n nop\n endc\n rts\n"),
    ok ("lwasm", "ifne not taken",       " ifne 0\n nop\n endc\n rts\n"),
    ok ("lwasm", "ifeq",                 " ifeq 0\n nop\n endc\n rts\n"),
    ok ("lwasm", "ifgt",                 " ifgt 1\n nop\n endc\n rts\n"),
    ok ("lwasm", "ifge",                 " ifge 0\n nop\n endc\n rts\n"),
    ok ("lwasm", "iflt",                 " iflt 1\n nop\n endc\n rts\n"),
    ok ("lwasm", "ifle",                 " ifle 0\n nop\n endc\n rts\n"),
    ok ("lwasm", "else",                 " ifne 0\n nop\n else\n clra\n endc\n rts\n"),
    ok ("lwasm", "endif closes too",     " ifne 1\n nop\n endif\n rts\n"),
    ok ("lwasm", "ifdef",                "sym equ 1\n ifdef sym\n nop\n endc\n rts\n"),
    ok ("lwasm", "ifndef",               " ifndef nosuch\n nop\n endc\n rts\n"),
    ok ("lwasm", "uppercase keywords",   " IFNE 1\n NOP\n ENDC\n RTS\n"),
    ok ("lwasm", "nested conditionals",  " ifne 1\n ifne 1\n nop\n endc\n rts\n endc\n"),
    ok ("lwasm", "condition folds a constant",
        "n equ 3\n ifne n-3\n nop\n endc\n rts\n"),
    // An untaken branch defines nothing, and the definition it holds decides an
    // instruction's *size* — `equ $10` is direct and two bytes, `equ $1234` is
    // extended and three. A walk-time binding would silently pick direct.
    ok ("lwasm", "an untaken branch's equ is invisible",
        " ifne 0\nsym equ $10\n endc\nsym equ $1234\n lda sym\n"),
    ok ("lwasm", "a taken branch's equ decides the mode",
        " ifne 1\nsym equ $10\n endc\n lda sym\n"),

    // ---- rgbasm / SM83 ------------------------------------------------------
    // Conditionals: `ELIF` rather than `ELSEIF`, and `ENDC` is the **only**
    // closer — rgbds answers `ENDIF` with `Undefined macro`.
    ok ("rgbasm", "if taken", "SECTION \"s\",ROM0[0]\nIF 1\n nop\nENDC\n ret\n"),
    ok ("rgbasm", "if not taken", "SECTION \"s\",ROM0[0]\nIF 0\n nop\nENDC\n ret\n"),
    ok ("rgbasm", "if/else", "SECTION \"s\",ROM0[0]\nIF 0\n nop\nELSE\n ret\nENDC\n"),
    ok ("rgbasm", "elif", "SECTION \"s\",ROM0[0]\nIF 0\n nop\nELIF 1\n ret\nENDC\n"),
    ok ("rgbasm", "condition folds a constant", "SECTION \"s\",ROM0[0]\nDEF N EQU 1\nIF N\n nop\nENDC\n ret\n"),
    ok ("rgbasm", "lowercase conditional", "SECTION \"s\",ROM0[0]\nif 1\n nop\nendc\n ret\n"),
    ok ("rgbasm", "nested conditionals", "SECTION \"s\",ROM0[0]\nIF 1\nIF 1\n nop\nENDC\n ret\nENDC\n"),
    ok ("rgbasm", "an untaken branch defines nothing", "SECTION \"s\",ROM0[0]\nIF 0\nDEF N EQU 1\nENDC\n ret\n"),
    // Repetition.
    ok ("rgbasm", "rept", "SECTION \"s\",ROM0[0]\nREPT 3\n nop\nENDR\n"),
    ok ("rgbasm", "rept 0", "SECTION \"s\",ROM0[0]\nREPT 0\n nop\nENDR\n ret\n"),
    ok ("rgbasm", "lowercase rept", "SECTION \"s\",ROM0[0]\nrept 3\n nop\nendr\n"),
    ok ("rgbasm", "rept count from a constant", "SECTION \"s\",ROM0[0]\nDEF N EQU 3\nREPT N\n nop\nENDR\n"),
    ok ("rgbasm", "nested rept", "SECTION \"s\",ROM0[0]\nREPT 2\nREPT 2\n nop\nENDR\n inc a\nENDR\n"),

    // rgbasm macros: `MACRO name` … `ENDM` with positional `\\1` parameters.
    // The old `name: MACRO` header is gone in rgbds 1.0 — `syntax error,
    // unexpected MACRO` — so only the keyword-first form is ours to take.
    ok ("rgbasm", "macro definition and invocation",
        "SECTION \"s\",ROM0[0]\nMACRO m\n nop\nENDM\n m\n ret\n"),
    ok ("rgbasm", "macro with a positional parameter",
        "SECTION \"s\",ROM0[0]\nMACRO ldav\n ld a,\\1\nENDM\n ldav 5\n ret\n"),
    ok ("rgbasm", "two positional parameters",
        "SECTION \"s\",ROM0[0]\nMACRO two\n ld a,\\1\n ld b,\\2\nENDM\n two 1,2\n"),
    ok ("rgbasm", "a macro invokes one defined earlier",
        "SECTION \"s\",ROM0[0]\nMACRO inner\n nop\nENDM\nMACRO outer\n inner\nENDM\n outer\n"),
    ok ("rgbasm", "a macro inside a conditional",
        "SECTION \"s\",ROM0[0]\nMACRO m\n nop\nENDM\nIF 1\n m\nENDC\n"),
    ok ("rgbasm", "a macro inside a repetition",
        "SECTION \"s\",ROM0[0]\nMACRO m\n nop\nENDM\nREPT 3\n m\nENDR\n"),

    // ---- vasm / 68000 -------------------------------------------------------
    // Conditionals: numeric forms compare against zero, `ifd`/`ifnd` test a
    // symbol (`ifdef` is *not* vasm's — `unknown mnemonic`), and `endif` and
    // `endc` both close.
    ok ("vasm", "ifne taken",      "\tifne 1\n\tnop\n\tendif\n\trts\n"),
    ok ("vasm", "ifne not taken",  "\tifne 0\n\tnop\n\tendif\n\trts\n"),
    ok ("vasm", "ifeq",            "\tifeq 0\n\tnop\n\tendif\n\trts\n"),
    ok ("vasm", "ifgt",            "\tifgt 1\n\tnop\n\tendif\n\trts\n"),
    ok ("vasm", "ifge",            "\tifge 0\n\tnop\n\tendif\n\trts\n"),
    ok ("vasm", "iflt",            "\tiflt 1\n\tnop\n\tendif\n\trts\n"),
    ok ("vasm", "ifle",            "\tifle 0\n\tnop\n\tendif\n\trts\n"),
    ok ("vasm", "plain if",        "\tif 1\n\tnop\n\tendif\n\trts\n"),
    ok ("vasm", "ifd",             "sym\tequ 1\n\tifd sym\n\tnop\n\tendif\n\trts\n"),
    ok ("vasm", "ifnd",            "\tifnd nosuch\n\tnop\n\tendif\n\trts\n"),
    ok ("vasm", "else",            "\tifne 0\n\tnop\n\telse\n\trts\n\tendif\n"),
    ok ("vasm", "endc closes too", "\tifne 1\n\tnop\n\tendc\n\trts\n"),
    ok ("vasm", "uppercase",       "\tIFNE 1\n\tNOP\n\tENDIF\n"),
    ok ("vasm", "nested",          "\tifne 1\n\tifne 1\n\tnop\n\tendif\n\trts\n\tendif\n"),
    ok ("vasm", "condition folds a constant",
        "n\tequ 3\n\tifne n-3\n\tnop\n\tendif\n\trts\n"),
    ok ("vasm", "an untaken branch defines nothing",
        "\tifne 0\nsym\tequ 1\n\tendif\n\tdc.b 2\n"),

    // Repetition. `REPTN` is an **implicit** 0-based counter — no named
    // parameter — and it reads -1 outside any `rept`.
    ok ("vasm", "rept",            "\trept 3\n\tnop\n\tendr\n"),
    ok ("vasm", "rept 0",          "\trept 0\n\tnop\n\tendr\n\trts\n"),
    ok ("vasm", "rept negative is empty", "\trept -1\n\tdc.b 1\n\tendr\n\tdc.b 2\n"),
    ok ("vasm", "rept count from a constant",
        "n\tequ 3\n\trept n\n\tdc.b 1\n\tendr\n"),
    ok ("vasm", "REPTN counts from zero", "\trept 3\n\tdc.b REPTN\n\tendr\n"),
    ok ("vasm", "REPTN is the innermost loop's",
        "\trept 2\n\trept 2\n\tdc.b REPTN\n\tendr\n\tendr\n"),
    ok ("vasm", "REPTN outside a rept is -1", "\tdc.b REPTN\n"),
    ok ("vasm", "a condition inside a rept reads REPTN",
        "\trept 3\n\tifne REPTN\n\tdc.b REPTN\n\tendif\n\tendr\n"),

    ok ("vasm", "moveq / move.l imm",    " moveq #1,d0\n move.l #$12345678,d0\n"),
    ok ("vasm", "old-style d(An)",       " move.w 4(a0),d0\n move.w 4(a0,d0.w),d1\n"),
    ok ("vasm", "predec / postinc",      " move.l -(a7),d0\n move.l (a0)+,d1\n"),
    ok ("vasm", "movem / dbra / trap",   " movem.l d0-d7/a0-a6,-(a7)\n dbra d0,*\n trap #0\n"),
    ok ("vasm", "sub/cmp An (base)",     " sub.l a0,a1\n cmp.l a0,a1\n"),
    ok ("vasm", "new-style (d,An)",      " lea (4,a0),a1\n"),
    ok ("vasm", "new-style (d,An,Xn)",   " lea (4,a0,d0.w),a1\n"),
    ok ("vasm", "move (d,An)",           " move.w (4,a0),d0\n"),
    ok ("vasm", "new-style (An,Xn)",     " lea (a0,d0.w),a1\n"),
    ok ("vasm", "new-style (d,PC)",      " move.w (6,pc),d0\n"),
    ok ("vasm", "mnemonic suba",         " suba.l a0,a1\n"),
    ok ("vasm", "mnemonic cmpa",         " cmpa.l a0,a1\n"),
    ok ("vasm", "mnemonic adda",         " adda.l a0,a1\n"),
    ok ("vasm", "eori form eor #imm",    " eor.w #5,d0\n"),
    ok ("vasm", "andi #imm,(mem)",       " and.w #$ff,(a0)\n"),
    ok ("vasm", "ori #imm,(mem)",        " or.w #1,(a0)\n"),
    ok ("vasm", "abs size suffix .w",    " move.w $1234.w,d0\n"),
    ok ("vasm", "abs size suffix .l",    " move.l $12345678.l,d0\n"),
    ok ("vasm", "abs .l forces long",    " move.w $1234.l,d0\n"),

    // ---- ca65-816 / 65816 ---------------------------------------------------
    ok ("ca65-816", "imm / dp / long",   " lda #$12\n lda $12\n lda $123456\n"),
    ok ("ca65-816", "[dp] / [dp],y / ,s"," lda [$12]\n lda [$12],y\n lda $12,s\n"),
    ok ("ca65-816", "jml / jsl / rep",   " jml $123456\n jsl $123456\n rep #$30\n"),
    ok ("ca65-816", "mvn / pei / bank ^", " mvn $01,$02\n pei ($12)\n lda #^$123456\n"),
    ok ("ca65-816", "operator &",        " lda #7&3\n"),
    ok ("ca65-816", "operator |",        " lda #1|2\n"),
    ok ("ca65-816", "operator <<",       " lda #1<<3\n"),
    ok ("ca65-816", "operator ^ (xor)",  " lda #5^1\n"),
    ok ("ca65-816", "instruction rtl",   " rtl\n"),
    ok ("ca65-816", "directive .dword",  " .dword $12345678\n"),
    ok ("ca65-816", "directive .dbyt",   " .dbyt $1234\n"),
    ok ("ca65-816", "directive .asciiz", " .asciiz \"hi\"\n"),
];

#[test]
#[ignore = "needs the reference assemblers; run with --ignored"]
fn source_matches_reference() {
    let tmp = std::env::temp_dir().join("asm198x-differential");
    fs::create_dir_all(&tmp).expect("temp dir");

    let mut regressions: Vec<String> = Vec::new();
    let mut fixed: Vec<String> = Vec::new();
    let mut checked = 0usize;
    let mut skipped_tools: Vec<&str> = Vec::new();
    let mut recorder = support::verdicts::Recorder::new();

    for p in PROBES {
        let bin = tool(p.dialect);
        if !have(bin) {
            if !skipped_tools.contains(&bin) {
                skipped_tools.push(bin);
            }
            continue;
        }
        // Reference is the arbiter: if it won't accept the snippet, it's out of
        // scope for a "we must accept it too" check.
        let Some(reference) = reference(&tmp, p.dialect, p.body) else {
            continue;
        };
        checked += 1;
        recorder.record(
            support::verdicts::CaseRef {
                suite: Suite::Probe,
                cpu: probe_cpu(p.dialect),
                tool: bin,
                dialect: p.dialect,
                case: p.note.to_string(),
                source: p.body,
            },
            match p.gap {
                // A tracked gap is a divergence, not a plain fact: the
                // reference accepts and we knowingly do not match. The issue
                // number is the join id, so the recorded half and the `gap(..)`
                // marker in this file stay tied to each other.
                Some(issue) => verdict_corpus::Outcome::Divergence {
                    divergence: format!("issue-{issue}"),
                    hex: verdict_corpus::encode_hex(&reference),
                },
                None => verdict_corpus::Outcome::Bytes {
                    hex: verdict_corpus::encode_hex(&reference),
                },
            },
        );
        let mine = ours(p.dialect, p.body);
        let matches = mine.as_deref() == Some(reference.as_slice());
        match p.gap {
            None if !matches => regressions.push(format!(
                "[{}] {}: reference accepts, we {}",
                p.dialect,
                p.note,
                match &mine {
                    Some(b) => format!("emit {b:02X?} vs ref {reference:02X?}"),
                    None => "reject it".into(),
                }
            )),
            Some(issue) if matches => fixed.push(format!("[{}] {} (#{issue})", p.dialect, p.note)),
            _ => {}
        }
    }

    let recorded = recorder.flush().expect("write the verdict corpus");
    for bin in &skipped_tools {
        eprintln!("SKIP: `{bin}` not on PATH");
    }
    eprintln!("recorded {recorded} new verdict(s)");
    eprintln!(
        "differential: {checked} reference-accepted snippets checked, \
         {} known gaps still open",
        PROBES.iter().filter(|p| p.gap.is_some()).count()
    );

    assert!(
        regressions.is_empty(),
        "{} regression(s) — source the reference accepts that we no longer do:\n  {}",
        regressions.len(),
        regressions.join("\n  ")
    );
    assert!(
        fixed.is_empty(),
        "{} known gap(s) now pass — delete their `gap(...)` marker so the ledger stays honest:\n  {}",
        fixed.len(),
        fixed.join("\n  ")
    );
    assert!(
        checked > 0,
        "no snippets checked — no reference tools present?"
    );
}

// ===========================================================================
// U2/U3 — multi-file z80-family probes: the include and incbin mechanisms
// against the real tools. Each probe gets its own SUBDIRECTORY so stale files
// from other probes (or earlier runs) can never leak into resolution.
// ===========================================================================

/// One multi-file fixture: a root file plus its includes and binary assets,
/// assembled by both sides from a per-probe directory.
struct MultiProbe {
    /// `sjasmplus` | `pasmo` | `acme` | `ca65-816` | `ca65-huc6280` |
    /// `rgbasm` | `lwasm` — selects the reference tool and our entry point.
    dialect: &'static str,
    note: &'static str,
    /// `(file name, contents)`; the first entry is the root.
    files: &'static [(&'static str, &'static str)],
    /// `(file name, bytes)` — binary assets for the incbin probes (U3).
    binaries: &'static [(&'static str, &'static [u8])],
}

/// The 8-byte incbin probe asset (`10..17`), matching the U3 probe runs.
const ASSET: &[u8] = &[0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17];

const MULTI_PROBES: &[MultiProbe] = &[
    MultiProbe {
        dialect: "sjasmplus",
        binaries: &[],
        note: "two-file include + equ feeding bit/rst/ds (KTD1)",
        files: &[
            (
                "main.asm",
                "        org $8000\n        include \"defs.inc\"\n        bit BITNUM,a\n        rst RSTVEC\n        ds PAD\n        ld a,1\n",
            ),
            ("defs.inc", "BITNUM equ 5\nRSTVEC equ $18\nPAD equ 3\n"),
        ],
    },
    MultiProbe {
        dialect: "sjasmplus",
        binaries: &[],
        note: "three-deep nested include, code at every level",
        files: &[
            (
                "main.asm",
                "        org $8000\n        ld a,1\n        include \"a.inc\"\n        ld e,5\n",
            ),
            (
                "a.inc",
                "        ld b,2\n        include \"b.inc\"\n        ld d,4\n",
            ),
            ("b.inc", "        ld c,3\n"),
        ],
    },
    MultiProbe {
        dialect: "sjasmplus",
        binaries: &[],
        note: "locals scope across the include boundary, both directions",
        files: &[
            (
                "main.asm",
                "        org $8000\nstart:\n.here:  nop\n        include \"loc.inc\"\n        jr .after\n.after: nop\n",
            ),
            (
                "loc.inc",
                ".inloc: nop\n        jr .inloc\n        jr .here\n",
            ),
        ],
    },
    MultiProbe {
        dialect: "sjasmplus",
        binaries: &[],
        note: "same file included twice is processed twice",
        files: &[
            (
                "main.asm",
                "        org $8000\n        include \"body.inc\"\n        include \"body.inc\"\n",
            ),
            ("body.inc", "        nop\n"),
        ],
    },
    MultiProbe {
        dialect: "sjasmplus",
        binaries: &[],
        note: "global defined inside the include rescopes the includer's locals",
        files: &[
            (
                "main.asm",
                "        org $8000\nstart:  nop\n        include \"glob.inc\"\n.tail:  nop\n        jr .tail\n",
            ),
            ("glob.inc", "mid:    nop\n"),
        ],
    },
    // --- U3: incbin ---
    MultiProbe {
        dialect: "sjasmplus",
        binaries: &[("data.bin", ASSET)],
        note: "plain incbin inserts the whole asset between code (U3)",
        files: &[(
            "main.asm",
            "        org $8000\n        db $aa\n        incbin \"data.bin\"\n        db $bb\n",
        )],
    },
    MultiProbe {
        dialect: "sjasmplus",
        binaries: &[("data.bin", ASSET)],
        note: "incbin offset form skips into the asset (U3)",
        files: &[(
            "main.asm",
            "        org $8000\n        incbin \"data.bin\",2\n",
        )],
    },
    MultiProbe {
        dialect: "sjasmplus",
        binaries: &[("data.bin", ASSET)],
        note: "incbin offset+length form, args as equ-constant expressions (U3)",
        files: &[(
            "main.asm",
            "OFF equ 2\n        org $8000\n        incbin \"data.bin\",OFF,3\n",
        )],
    },
    MultiProbe {
        dialect: "sjasmplus",
        binaries: &[("data.bin", ASSET)],
        note: "incbin negative offset/length count from the end (U3)",
        files: &[(
            "main.asm",
            "        org $8000\n        incbin \"data.bin\",-4,2\n        incbin \"data.bin\",2,-3\n",
        )],
    },
    MultiProbe {
        dialect: "sjasmplus",
        binaries: &[("sprite.bin", ASSET)],
        note: "incbin inside an include resolves via the include machinery (U3)",
        files: &[
            (
                "main.asm",
                "        org $8000\n        include \"art.inc\"\n        db $bb\n",
            ),
            ("art.inc", "        incbin \"sprite.bin\",0,4\n"),
        ],
    },
    MultiProbe {
        dialect: "pasmo",
        binaries: &[("data.bin", ASSET)],
        note: "pasmo's plain incbin inserts the whole asset (U3)",
        files: &[(
            "main.asm",
            "        org $8000\n        db $aa\n        incbin \"data.bin\"\n        db $bb\n",
        )],
    },
    // --- U8: conditional-guarded includes (KTD1) ---
    MultiProbe {
        dialect: "sjasmplus",
        binaries: &[],
        note: "untaken guarded include never loads — the target does not exist (U8, KTD1)",
        files: &[(
            "main.asm",
            "        org $8000\n        IF 0\n        include \"missing.inc\"\n        ENDIF\n        ld a,1\n",
        )],
    },
    MultiProbe {
        dialect: "sjasmplus",
        binaries: &[],
        note: "taken guarded include loads; its equ feeds a later form; DEFINEs thread both ways (U8)",
        files: &[
            (
                "main.asm",
                "        org $8000\n        DEFINE WANT\n        IF 1\n        include \"guard.inc\"\n        ENDIF\n        bit BITN,a\n        IFDEF FROMINC\n        ld b,1\n        ENDIF\n",
            ),
            (
                "guard.inc",
                "        IFDEF WANT\n        ld a,9\n        ENDIF\nBITN equ 5\n        DEFINE FROMINC\n",
            ),
        ],
    },
    MultiProbe {
        dialect: "sjasmplus",
        binaries: &[],
        note: "conditional wholly inside an include, condition from the includer's equ (U8)",
        files: &[
            (
                "main.asm",
                "        org $8000\nMODE equ 2\nstart:\n        include \"body.inc\"\n",
            ),
            (
                "body.inc",
                "        IF MODE = 2\n.here:  nop\n        jr .here\n        ELSE\n        ld a,0\n        ENDIF\n",
            ),
        ],
    },
    // --- U4: acme (`!src`/`!bin`) ---
    MultiProbe {
        dialect: "acme",
        binaries: &[],
        note: "nested !src, code at every level, include-defined symbol feeds zp/abs (U4)",
        files: &[
            (
                "main.a",
                "* = $1000\n        lda #1\n        !src \"a.a\"\n        lda ptr\n        sta addr\n",
            ),
            (
                "a.a",
                "        lda #2\n        !src \"b.a\"\n        lda #4\n",
            ),
            ("b.a", "        lda #3\nptr = $10\naddr = $0400\n"),
        ],
    },
    MultiProbe {
        dialect: "acme",
        binaries: &[("data.bin", ASSET)],
        note: "!bin size+skip window, empty size slot, and the zero-pad posture (U4)",
        files: &[(
            "main.a",
            "* = $1000\n!byte $aa\n!bin \"data.bin\", 3, 2\n!bin \"data.bin\", , 6\n!bin \"data.bin\", 12\n!byte $bb\n",
        )],
    },
    MultiProbe {
        dialect: "acme",
        binaries: &[],
        note: "anonymous labels resolve across the !src boundary, both directions (U4)",
        files: &[
            (
                "main.a",
                "* = $1000\n-       lda #1\n        jmp +\n        !src \"part.a\"\n        bne -\n",
            ),
            ("part.a", "+       lda #2\n        beq -\n"),
        ],
    },
    MultiProbe {
        dialect: "acme",
        binaries: &[],
        note: "conditional-guarded !src: untaken never loads (target absent), taken splices (U4)",
        files: &[
            (
                "main.a",
                "* = $1000\nDEMO = 1\n!ifdef NOPE {\n        !src \"missing.a\"\n}\n!ifdef DEMO {\n        !src \"demo.a\"\n}\n        lda #3\n",
            ),
            ("demo.a", "        lda #2\n"),
        ],
    },
    MultiProbe {
        dialect: "acme",
        binaries: &[("data.bin", ASSET)],
        note: "labels on the !src and !bin lines bind at the include point / payload (U4)",
        files: &[
            (
                "main.a",
                "* = $1000\nhere    !src \"body.a\"\nart     !bin \"data.bin\", 2\n        !word here\n        !word art\n",
            ),
            ("body.a", "        lda #7\n"),
        ],
    },
    // --- U7: acme `!zone` × `!src` (zone state threads through includes) ---
    MultiProbe {
        dialect: "acme",
        binaries: &[],
        note: "zone state threads through !src: the include inherits the \
               includer's zone, and a !zone inside it persists after return (U7)",
        files: &[
            (
                "main.a",
                "* = $1000\n!zone one\n.x      lda #1\n        !src \"part.a\"\n        bne .y\n",
            ),
            ("part.a", "        beq .x\n!zone inc\n.y      lda #2\n"),
        ],
    },
    MultiProbe {
        dialect: "acme",
        binaries: &[],
        note: "a .local defined inside an include (no zone switch) is visible \
               to the includer after return — same zone (U7)",
        files: &[
            (
                "main.a",
                "* = $1000\n!zone one\n        lda #1\n        !src \"mid.a\"\n        bne .mid\n",
            ),
            ("mid.a", ".mid    lda #2\n"),
        ],
    },
    // --- U4: the ca65-flat family (`.include`/`.incbin`, 65816 + HuC6280) ---
    MultiProbe {
        dialect: "ca65-816",
        binaries: &[],
        note: "nested .include via a subdirectory; the ancestor-chain resolution; \
               .a16 + a symbol defined inside flow out to the includer (U4)",
        files: &[
            (
                "main.s",
                " lda #$11\n .include \"sub/mid.s\"\n lda #$34\n lda ptr\n",
            ),
            // From sub/mid.s, `shared.s` lives in the ROOT's directory — ca65
            // resolves it by walking the include chain's directories
            // (probe-pinned); so must we.
            ("sub/mid.s", " lda #$22\n .include \"shared.s\"\n"),
            ("shared.s", ".a16\nptr = $10\n lda #$12\n"),
        ],
    },
    MultiProbe {
        dialect: "ca65-816",
        binaries: &[("data.bin", ASSET)],
        note: ".incbin windows: plain, offset, offset+size, offset at EOF, \
               and ca65's negative-size-reads-to-EOF sentinel (U4)",
        files: &[(
            "main.s",
            " .byte $aa\n .incbin \"data.bin\"\n .incbin \"data.bin\", 2\n \
             .incbin \"data.bin\", 2, 3\n .incbin \"data.bin\", 8\n \
             .incbin \"data.bin\", 2, -2\n .byte $bb\n",
        )],
    },
    MultiProbe {
        dialect: "ca65-816",
        binaries: &[("data.bin", ASSET)],
        note: "labels on the .include/.incbin lines bind at the include point / payload (U4)",
        files: &[
            (
                "main.s",
                "here: .include \"body.s\"\nart: .incbin \"data.bin\", 2, 2\n \
                 .word here\n .word art\n",
            ),
            ("body.s", " lda #$07\n"),
        ],
    },
    MultiProbe {
        dialect: "ca65-huc6280",
        binaries: &[],
        note: "nested .include with HuC6280 extension ops; an include-defined \
               symbol feeds later zp selection (U4)",
        files: &[
            (
                "main.s",
                " lda #$11\n .include \"a.s\"\n lda ptr\n rmb0 $10\n",
            ),
            ("a.s", " sax\n .include \"b.s\"\n"),
            ("b.s", "ptr = $10\n tii $1000, $2000, $0010\n"),
        ],
    },
    MultiProbe {
        dialect: "ca65-huc6280",
        binaries: &[("data.bin", ASSET)],
        note: ".incbin offset/size and the negative-size sentinel on the HuC6280 leg (U4)",
        files: &[(
            "main.s",
            " .byte $aa\n .incbin \"data.bin\", 2, 3\n .incbin \"data.bin\", 6, -9\n .byte $bb\n",
        )],
    },
    // --- U4: rgbasm (`INCLUDE`/`INCBIN`, SM83) — assembled + linked, the
    // reference bytes compared as a prefix (rgblink zero-pads the ROM bank).
    MultiProbe {
        dialect: "rgbasm",
        binaries: &[],
        note: "nested INCLUDE; DEF constants defined inside feed the includer's \
               later bit/rst/ds (U4)",
        files: &[
            (
                "main.asm",
                "SECTION \"c\", ROM0[$0]\n ld a, 1\n INCLUDE \"a.inc\"\n bit BITNUM, a\n \
                 rst RSTVEC\n ds PAD\n ld b, 2\n",
            ),
            ("a.inc", "DEF BITNUM EQU 5\n INCLUDE \"b.inc\"\n ld d, 4\n"),
            ("b.inc", "DEF RSTVEC EQU $18\nDEF PAD EQU 3\n ld c, 3\n"),
        ],
    },
    MultiProbe {
        dialect: "rgbasm",
        binaries: &[("data.bin", ASSET)],
        note: "INCBIN windows: plain, offset, offset+length, offset at EOF, \
               length 0, and DEF-expression arguments (U4)",
        files: &[(
            "main.asm",
            "SECTION \"c\", ROM0[$0]\nDEF OFF EQU 2\n db $aa\n INCBIN \"data.bin\"\n \
             INCBIN \"data.bin\", 2\n INCBIN \"data.bin\", OFF, OFF+1\n \
             INCBIN \"data.bin\", 8\n INCBIN \"data.bin\", 0, 0\n db $bb\n",
        )],
    },
    MultiProbe {
        dialect: "rgbasm",
        binaries: &[],
        note: "locals scope across the INCLUDE boundary; a global inside \
               rescopes the includer's later locals (U4)",
        files: &[
            (
                "main.asm",
                "SECTION \"c\", ROM0[$0]\nstart:\n.here:\n nop\n INCLUDE \"loc.inc\"\n \
                 jr .here\n INCLUDE \"glob.inc\"\n.tail:\n nop\n jr .tail\n",
            ),
            ("loc.inc", ".inloc:\n nop\n jr .inloc\n"),
            ("glob.inc", "mid:\n nop\n"),
        ],
    },
    MultiProbe {
        dialect: "rgbasm",
        binaries: &[("data.bin", ASSET)],
        note: "labels on the INCLUDE and INCBIN lines bind at the include \
               point / payload start (U4)",
        files: &[
            (
                "main.asm",
                "SECTION \"c\", ROM0[$0]\nhere: INCLUDE \"body.inc\"\n\
                 art: INCBIN \"data.bin\", 2, 2\n dw here\n dw art\n",
            ),
            ("body.inc", " ld a, 7\n"),
        ],
    },
    // --- U4: lwasm (`include`/`use`/`includebin`, 6809) ---
    MultiProbe {
        dialect: "lwasm",
        binaries: &[],
        note: "nested include in both spellings (quoted include, bare use); an \
               equ defined inside feeds the includer's direct/extended choice (U4)",
        files: &[
            (
                "main.asm",
                "        lda #1\n        include \"a.inc\"\n        lda ptr\n        lda #5\n",
            ),
            (
                "a.inc",
                "        lda #2\n        use b.inc\n        lda #4\n",
            ),
            ("b.inc", "ptr     equ $20\n        lda #3\n"),
        ],
    },
    MultiProbe {
        dialect: "lwasm",
        binaries: &[("data.bin", ASSET)],
        note: "includebin windows: plain (quoted + bare), offset, offset+length, \
               offset at EOF, length 0, and the negative-offset-from-EOF forms (U4)",
        files: &[(
            "main.asm",
            "        fcb $aa\n        includebin \"data.bin\"\n        includebin data.bin,2\n\
             \x20       includebin \"data.bin\",2,3\n        includebin \"data.bin\",8\n\
             \x20       includebin \"data.bin\",2,0\n        includebin \"data.bin\",-4,2\n\
             \x20       includebin \"data.bin\",-2\n        fcb $bb\n",
        )],
    },
    MultiProbe {
        dialect: "lwasm",
        binaries: &[("data.bin", ASSET)],
        note: "labels on the include and includebin lines bind at the include \
               point / payload start (U4)",
        files: &[
            (
                "main.asm",
                "        org $1000\nhere    include \"body.inc\"\nart     includebin \"data.bin\",2,2\n\
                 \x20       fdb here\n        fdb art\n",
            ),
            ("body.inc", "        lda #7\n"),
        ],
    },
    // --- U5: the ca65-NES assemble+link path (`.include`/`.incbin` through
    // the Item::Native pipeline; ca65 + ld65 with the curriculum's fixed
    // nes.cfg, byte-comparing the whole .nes ROM). ---
    MultiProbe {
        dialect: "ca65-nes",
        binaries: &[],
        note: "NES program split across includes: PRG code + CHARS data in \
               separate files; zp symbol + include-defined constant thread \
               both directions; a .segment switch inside the include \
               persists (U5)",
        files: &[
            (
                "main.s",
                ".segment \"HEADER\"\n .byte \"NES\", $1A, 2, 1\n\
                 .segment \"ZEROPAGE\"\npos: .res 1\n\
                 .segment \"CODE\"\nreset: lda #SPEED\n .include \"prg.s\"\n .byte $77\n\
                 .segment \"VECTORS\"\n .word 0, reset, 0\n",
            ),
            (
                "prg.s",
                "SPEED = 3\n sta pos\nloop: jmp loop\n .include \"chars.s\"\n",
            ),
            ("chars.s", ".segment \"CHARS\"\n .byte $AA, $BB\n"),
        ],
    },
    MultiProbe {
        dialect: "ca65-nes",
        binaries: &[("tiles.chr", ASSET)],
        note: ".incbin of CHR data inside a CHARS-segment include: plain, \
               offset+size, and the negative-size sentinel, under the NES \
               link (U5)",
        files: &[
            (
                "main.s",
                ".segment \"CODE\"\nreset: lda #$01\n .include \"art.s\"\n\
                 .segment \"VECTORS\"\n .word 0, reset, 0\n",
            ),
            (
                "art.s",
                ".segment \"CHARS\"\n .incbin \"tiles.chr\"\n \
                 .incbin \"tiles.chr\", 2, 3\n .incbin \"tiles.chr\", 6, -9\n",
            ),
        ],
    },
    MultiProbe {
        dialect: "ca65-nes",
        binaries: &[],
        note: "anonymous and cheap labels resolve across the .include \
               boundary in evaluation order on the NES path (U5)",
        files: &[
            (
                "main.s",
                ".segment \"CODE\"\nreset: ldx #0\n: inx\n jmp :+\n\
                 .include \"part.s\"\n bne :-\n@tail: jmp @tail\n\
                 .segment \"VECTORS\"\n .word 0, reset, 0\n",
            ),
            ("part.s", ": nop\n@in: jmp @in\nmid: nop\n"),
        ],
    },
    // --- U4: the asl chips (`include`/`binclude`, asl + p2bin) — probed on
    // the 8080 (the family's debut chip), spot-checked on the TMS9900, and
    // the CP1610 for the decle-accounting case.
    MultiProbe {
        dialect: "8080",
        binaries: &[],
        note: "asl nested include, both spellings incl. the .inc extension \
               default; equ defined inside feeds rst selection + a later \
               immediate (U4; `ds` itself is out of probe scope — asl leaves \
               a gap p2bin fills with $FF, ours emits zeros, pre-existing)",
        files: &[
            (
                "main.asm",
                "\tcpu 8080\n\torg 0\n\tmvi a,1\n\tinclude \"a.inc\"\n\trst RSTVEC\n\tmvi c,PAD\n\tmvi e,5\n",
            ),
            (
                "a.inc",
                "RSTVEC equ 3\n\tmvi b,2\n\tinclude sub\n\tmvi d,4\n",
            ),
            ("sub.inc", "PAD equ 3\n\tmvi c,3\n"),
        ],
    },
    MultiProbe {
        dialect: "8080",
        binaries: &[("data.bin", ASSET)],
        note: "asl binclude windows: plain, offset, equ-fed offset+length, \
               offset at EOF, length 0, and the bare-name spelling (U4)",
        files: &[(
            "main.asm",
            "OFF equ 2\n\tcpu 8080\n\torg 0\n\tdb 0aah\n\tbinclude \"data.bin\"\n\tbinclude \"data.bin\",2\n\tbinclude data.bin,OFF,3\n\tbinclude \"data.bin\",8\n\tbinclude \"data.bin\",0,0\n\tdb 0bbh\n",
        )],
    },
    MultiProbe {
        dialect: "8080",
        binaries: &[("data.bin", ASSET)],
        note: "asl labels on the include and binclude lines bind at the \
               include point / payload start (U4)",
        files: &[
            (
                "main.asm",
                "\tcpu 8080\n\torg 0\nhere:\tinclude \"body.inc\"\nart:\tbinclude \"data.bin\",2,2\n\tlxi h,here\n\tlxi h,art\n",
            ),
            ("body.inc", "\tmvi a,7\n"),
        ],
    },
    MultiProbe {
        dialect: "tms9900",
        binaries: &[("data.bin", ASSET)],
        note: "asl family uniformity spot-check: nested include via a \
               subdirectory beats a root decoy (requester-dir resolution), \
               equ feeds the includer, binclude window (U4)",
        files: &[
            (
                "main.asm",
                "\tcpu TMS9900\n\torg 0\n\tinclude \"sub/mid.inc\"\n\tli r1,K\n\tbinclude \"data.bin\",2,3\n\tbyte 0bbh\n",
            ),
            ("sub/mid.inc", "\tinclude \"shared.inc\"\n"),
            ("sub/shared.inc", "K equ 42h\n\tbyte 11h\n"),
            // The decoy: if either side anchored at the root/cwd instead of
            // the requesting file's directory, the bytes would diverge.
            ("shared.inc", "K equ 99h\n\tbyte 99h\n"),
        ],
    },
    MultiProbe {
        dialect: "cp1610",
        binaries: &[("odd3.bin", &[0x10, 0x11, 0x12]), ("data.bin", ASSET)],
        note: "cp1610 include + equ across the boundary, and binclude decle \
               accounting: an odd byte count and a byte-window, each one \
               decle per byte with the zero tail (U4)",
        files: &[
            (
                "main.asm",
                "\tcpu CP-1600\n\trelaxed on\n\torg 00000H\n\tinclude \"defs.inc\"\n\tmvii K,r0\n\tbinclude \"odd3.bin\"\nafter:\tword after\n\tbinclude \"data.bin\",2,3\n",
            ),
            ("defs.inc", "K equ 5\n\tword 0AAAAH\n"),
        ],
    },
    // --- U6: the vasm (68000) multipass path ---
    MultiProbe {
        dialect: "vasm",
        binaries: &[],
        note: "vasm two-file include + equ feeding the optimizer's \
               addq/lea/moveq selections (U6, KTD1)",
        files: &[
            (
                "main.s",
                "\tinclude \"defs.inc\"\n\tadd.l #N,d0\n\tadd.l #BIG,a0\n\tmoveq #N,d1\n\trts\n",
            ),
            ("defs.inc", "N equ 5\nBIG equ $1234\n"),
        ],
    },
    MultiProbe {
        dialect: "vasm",
        binaries: &[],
        note: "vasm three-deep nested include, code at every level; nested \
               resolution anchors at the root's directory — the sub/ decoy \
               would diverge if either side searched the requester's dir (U6)",
        files: &[
            (
                "main.s",
                "\tmoveq #1,d0\n\tinclude \"sub/mid.inc\"\n\tmoveq #5,d4\n",
            ),
            (
                "sub/mid.inc",
                "\tmoveq #2,d1\n\tinclude \"leaf.inc\"\n\tmoveq #4,d3\n",
            ),
            // Root-anchored resolution (probe-pinned): this copy wins…
            ("leaf.inc", "\tmoveq #3,d2\n"),
            // …over the requester-directory decoy.
            ("sub/leaf.inc", "\tmoveq #9,d2\n"),
        ],
    },
    MultiProbe {
        dialect: "vasm",
        binaries: &[],
        note: "vasm locals scope across the include boundary, both directions \
               (U6)",
        files: &[
            (
                "main.s",
                "start:\tnop\n\tinclude \"loc.inc\"\n.tail:\tnop\n\tbra.s .tail\n\tbra.s .here\n",
            ),
            ("loc.inc", ".here:\tnop\n\tbra.s .here\n"),
        ],
    },
    MultiProbe {
        dialect: "vasm",
        binaries: &[("data.bin", ASSET)],
        note: "vasm incbin windows: plain between data, offset, equ-fed \
               offset+length, offset at EOF, length 0 = rest of file, and \
               silent over-length truncation (U6)",
        files: &[(
            "main.s",
            "OFF equ 2\n\tdc.b $aa\n\tincbin \"data.bin\"\n\tincbin \"data.bin\",2\n\tincbin \"data.bin\",OFF,3\n\tincbin \"data.bin\",8\n\tincbin \"data.bin\",0,0\n\tincbin \"data.bin\",6,4\n\tdc.b $bb\n",
        )],
    },
    MultiProbe {
        dialect: "vasm",
        binaries: &[("sprite.bin", ASSET)],
        note: "vasm incbin inside an include resolves via the include \
               machinery, with labels on both directive lines (U6)",
        files: &[
            (
                "main.s",
                "here:\tinclude \"art.inc\"\n\tdc.w here\n\tdc.w art\n",
            ),
            ("art.inc", "art:\tincbin \"sprite.bin\",0,4\n"),
        ],
    },
    MultiProbe {
        dialect: "vasm-exe",
        binaries: &[("data.bin", ASSET)],
        note: "vasm hunk executable: a section switch inside an include \
               persists into the includer, and an incbin lands in that \
               section (U6; label-less so vasm emits no HUNK_SYMBOL)",
        files: &[
            (
                "main.s",
                "\tsection one,code\n\tmoveq #1,d0\n\tinclude \"sw.inc\"\n\tdc.b $02\n\tincbin \"data.bin\",2,3\n",
            ),
            ("sw.inc", "\tsection two,data\n\tdc.b $01\n"),
        ],
    },
];

#[test]
#[ignore = "needs sjasmplus + pasmo + acme + ca65/ld65 + rgbasm/rgblink + lwasm; run with --ignored"]
fn multi_file_source_matches_reference() {
    let base = std::env::temp_dir().join("asm198x-differential-multi");
    let mut failures: Vec<String> = Vec::new();
    let mut checked = 0usize;
    for (i, p) in MULTI_PROBES.iter().enumerate() {
        if !have(tool(p.dialect)) {
            eprintln!("SKIP: `{}` not on PATH", tool(p.dialect));
            continue;
        }
        // The rgbasm arm links with rgblink (RGBDS ships them together).
        if p.dialect == "rgbasm" && !have("rgblink") {
            eprintln!("SKIP: `rgblink` not on PATH");
            continue;
        }
        // The asl arms convert the `.p` object with p2bin (shipped together).
        if tool(p.dialect) == "asl" && !have("p2bin") {
            eprintln!("SKIP: `p2bin` not on PATH");
            continue;
        }
        // A per-probe subdirectory, wiped before use, so resolution can never
        // pick up a stale file from another probe.
        let dir = base.join(format!("probe-{i}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("probe dir");
        for (name, contents) in p.files {
            let path = dir.join(name);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).expect("fixture parent dir");
            }
            fs::write(path, contents).expect("write fixture");
        }
        for (name, bytes) in p.binaries {
            fs::write(dir.join(name), bytes).expect("write binary fixture");
        }
        let (root, _) = p.files[0];
        let out = dir.join("ref.bin");
        // A dialect's reference run is one or more commands (the ca65-flat
        // family assembles then links), all from the probe dir.
        let commands: Vec<Command> = match p.dialect {
            "sjasmplus" => {
                let mut c = Command::new("sjasmplus");
                c.arg("--nologo")
                    .arg(format!("--raw={}", out.display()))
                    .arg(root);
                vec![c]
            }
            "pasmo" => {
                let mut c = Command::new("pasmo");
                c.arg(root).arg(&out);
                vec![c]
            }
            // rgbasm assembles to an object; rgblink emits the ROM (the same
            // recipe as the SM83 conformance sweep).
            "rgbasm" => {
                let obj = dir.join("ref.o");
                let mut a = Command::new("rgbasm");
                a.arg("-o").arg(&obj).arg(root);
                let mut l = Command::new("rgblink");
                l.arg("-o").arg(&out).arg(&obj);
                vec![a, l]
            }
            "lwasm" => {
                let mut c = Command::new("lwasm");
                c.args(["--6809", "--raw", "-o"]).arg(&out).arg(root);
                vec![c]
            }
            // vasm runs from the probe dir, so its cwd-anchored resolution and
            // our root-input anchor agree (the cwd *is* the root's directory;
            // the nested-decoy probe proves neither side searches the
            // requester's). Flat = `-Fbin` (optimizer on, our entry's match);
            // exe = `-Fhunkexe -kick1hunks`, the curriculum recipe.
            "vasm" => {
                let mut c = Command::new("vasmm68k_mot");
                c.args(["-Fbin", "-quiet", "-o"]).arg(&out).arg(root);
                vec![c]
            }
            "vasm-exe" => {
                let mut c = Command::new("vasmm68k_mot");
                c.args(["-Fhunkexe", "-kick1hunks", "-quiet", "-o"])
                    .arg(&out)
                    .arg(root);
                vec![c]
            }
            // acme runs from the probe dir, so its cwd-anchored `!src`/`!bin`
            // resolution and our requesting-file-first order agree (the probe
            // fixtures are flat by design; the order divergence is documented
            // in the acme skin).
            "acme" => {
                let mut c = Command::new("acme");
                c.args(["-f", "plain", "-o"]).arg(&out).arg(root);
                vec![c]
            }
            // The NES path: ca65 + ld65 with the curriculum's fixed nes.cfg
            // (the recipe the ca65 curriculum leg uses), emitting a .nes ROM
            // that is byte-compared whole.
            "ca65-nes" => {
                let cfg = dir.join("nes.cfg");
                fs::write(
                    &cfg,
                    "MEMORY {\n\
                     \x20   ZP:     start = $00,    size = $100,   type = rw, file = \"\";\n\
                     \x20   RAM:    start = $0200,  size = $600,   type = rw, file = \"\";\n\
                     \x20   HEADER: start = $0,     size = $10,    type = ro, file = %O, fill = yes;\n\
                     \x20   PRG:    start = $8000,  size = $8000,  type = ro, file = %O, fill = yes;\n\
                     \x20   CHR:    start = $0,     size = $2000,  type = ro, file = %O, fill = yes;\n\
                     }\n\
                     SEGMENTS {\n\
                     \x20   ZEROPAGE: load = ZP,     type = zp;\n\
                     \x20   BSS:      load = RAM,    type = bss;\n\
                     \x20   HEADER:   load = HEADER, type = ro;\n\
                     \x20   CODE:     load = PRG,    type = ro,  start = $8000;\n\
                     \x20   VECTORS:  load = PRG,    type = ro,  start = $FFFA;\n\
                     \x20   CHARS:    load = CHR,    type = ro;\n\
                     }\n",
                )
                .expect("write nes.cfg");
                let obj = dir.join("ref.o");
                let mut a = Command::new("ca65");
                a.arg(root).arg("-o").arg(&obj);
                let mut l = Command::new("ld65");
                l.arg("-C").arg(&cfg).arg(&obj).arg("-o").arg(&out);
                vec![a, l]
            }
            // The ca65-flat family: assemble with the target CPU, link flat
            // at $0000 (the same recipe as the single-file ca65-816 arm).
            "ca65-816" | "ca65-huc6280" => {
                let cfg = dir.join("flat.cfg");
                fs::write(
                    &cfg,
                    "MEMORY { MAIN: start=$0000, size=$10000, fill=no, file=%O; }\n\
                     SEGMENTS { CODE: load=MAIN, type=ro; }\n",
                )
                .expect("write linker cfg");
                let cpu = if p.dialect == "ca65-816" {
                    "65816"
                } else {
                    "huc6280"
                };
                let obj = dir.join("ref.o");
                let mut a = Command::new("ca65");
                a.args(["--cpu", cpu]).arg(root).arg("-o").arg(&obj);
                let mut l = Command::new("ld65");
                l.arg("-C").arg(&cfg).arg(&obj).arg("-o").arg(&out);
                vec![a, l]
            }
            // The asl chips: assemble to a `.p` object, convert with p2bin
            // (the same recipe as the conformance sweeps). The root carries
            // its own `cpu`/`org` header, which our dialects ignore/share.
            "8080" | "tms9900" | "cp1610" => {
                let obj = dir.join("ref.p");
                let mut a = Command::new("asl");
                a.arg("-q").arg(root).arg("-o").arg(&obj);
                let mut b = Command::new("p2bin");
                b.arg(&obj).arg(&out);
                vec![a, b]
            }
            other => panic!("no multi-file runner for dialect `{other}`"),
        };
        let mut reference_failed = None;
        for mut c in commands {
            let run = c
                .current_dir(&dir)
                .output()
                .expect("run the reference tool");
            if !run.status.success() {
                reference_failed = Some(String::from_utf8_lossy(&run.stderr).into_owned());
                break;
            }
        }
        if let Some(stderr) = reference_failed {
            failures.push(format!(
                "{}: {} rejected the fixture: {stderr}",
                p.note, p.dialect
            ));
            continue;
        }
        let reference = fs::read(&out).expect("reference bytes");

        let root_path = dir.join(root);
        let source = fs::read_to_string(&root_path).expect("read root");
        let loader = asm198x::source::FsLoader::new(&dir, Vec::new());
        let entry = match p.dialect {
            "sjasmplus" => asm198x::assemble_sjasmplus_files,
            "pasmo" => asm198x::assemble_pasmo_files,
            "acme" => asm198x::assemble_acme_files,
            "ca65-816" => asm198x::assemble_ca65_816_files,
            "ca65-huc6280" => asm198x::assemble_ca65_huc6280_files,
            "ca65-nes" => asm198x::assemble_ca65_files,
            "rgbasm" => asm198x::assemble_rgbasm_files,
            "lwasm" => asm198x::assemble_lwasm_files,
            "vasm" => asm198x::assemble_vasm_warned_files,
            "vasm-exe" => asm198x::assemble_vasm_exe_files,
            "8080" => asm198x::assemble_i8080_files,
            "tms9900" => asm198x::assemble_tms9900_files,
            "cp1610" => asm198x::assemble_cp1610_files,
            other => panic!("no multi-file entry for dialect `{other}`"),
        };
        // rgblink zero-pads the ROM to the bank size (probe-pinned), so the
        // rgbasm arm compares our bytes as the reference's prefix and requires
        // the remainder to be all padding; every other arm is exact.
        let matches = |ours: &[u8]| {
            if p.dialect == "rgbasm" {
                reference.len() >= ours.len()
                    && reference[..ours.len()] == *ours
                    && reference[ours.len()..].iter().all(|b| *b == 0)
            } else {
                reference == ours
            }
        };
        checked += 1;
        match entry(&source, &root_path.to_string_lossy(), &loader) {
            Ok(r) if matches(&r.bytes) => {}
            Ok(r) => failures.push(format!(
                "{}: bytes diverge — ours {:02X?} vs ref {:02X?}",
                p.note,
                r.bytes,
                &reference[..reference.len().min(64)]
            )),
            Err(e) => failures.push(format!("{}: we reject it: {}", p.note, e.error)),
        }
    }
    assert!(
        failures.is_empty(),
        "{} multi-file probe failure(s):\n  {}",
        failures.len(),
        failures.join("\n  ")
    );
    assert!(
        checked > 0,
        "no probes checked — no reference tools present?"
    );
}

/// `fmt` must read what `asm` reads.
///
/// # The underlying problem, not the symptom
///
/// #130 records that five of six dialects refuse to format a file they will
/// assemble. That is worth fixing on its own — someone who can assemble a macro
/// will expect to format one, and "the formatter supports a subset of the
/// assembler" is a hard thing to explain. But the reason it went unnoticed
/// matters more: **nothing asserted the two paths agree**, and each refusal was
/// pinned by a test asserting the refusal, which locks the divergence in rather
/// than flagging it.
///
/// This is the missing assertion. Every probe body already carries the property
/// that makes it a fair test — it was verified accepted by its own reference
/// before being added — so a body this crate assembles is one it has no excuse
/// for refusing to lay out.
///
/// # Accepting is not enough
///
/// The first version of this test asked only whether the formatter *refused*,
/// and that let a worse defect through: `fmt` rendered a repetition block's
/// head and silently dropped its body and closer, so the source came back
/// shorter and still formatted cleanly. `fmt` is documented as safe to run
/// over source you have not read, so "did not refuse" is the weaker half of
/// the property. The formatted text must also **assemble to the same bytes**,
/// which is what catches a formatter that loses a line rather than one that
/// balks at it.
///
/// # The ledger
///
/// Known divergences are listed with the dialect and the reason. A listed one
/// that starts formatting fails the test, exactly as the `gap(...)` markers
/// above do: a ledger nobody has to update is a ledger that stops being true.
#[test]
fn the_formatter_reads_what_the_assembler_reads() {
    let mut refused: Vec<String> = Vec::new();
    let mut lost: Vec<String> = Vec::new();
    let mut checked = 0;

    for probe in PROBES {
        // Only bodies we actually assemble. A probe we reject is #93's problem
        // or a recorded gap, and not evidence about the formatter.
        let Some(Ok(before)) = support::verdicts::assemble_probe(probe.dialect, probe.body) else {
            continue;
        };
        let Some(outcome) = support::verdicts::format_probe(probe.dialect, probe.body) else {
            continue;
        };
        checked += 1;
        let formatted = match outcome {
            Ok(text) => text,
            Err(why) => {
                refused.push(format!("{} — {}: {why}", probe.dialect, probe.note));
                continue;
            }
        };
        // The layout changed; the program must not have.
        match support::verdicts::assemble_probe(probe.dialect, &formatted) {
            Some(Ok(after)) if after == before => {}
            Some(Ok(_)) => lost.push(format!(
                "{} — {}: formatted source assembles to different bytes",
                probe.dialect, probe.note
            )),
            Some(Err(why)) => lost.push(format!(
                "{} — {}: formatted source will not assemble: {why}",
                probe.dialect, probe.note
            )),
            None => {}
        }
    }

    let unexpected_loss: Vec<&String> = lost
        .iter()
        .filter(|l| {
            !FORMATTER_ROUND_TRIP_GAPS
                .iter()
                .any(|(d, note)| l.starts_with(&format!("{d} — {note}:")))
        })
        .collect();
    assert!(
        unexpected_loss.is_empty(),
        "{} source(s) survive formatting with a different program:\n  {}",
        unexpected_loss.len(),
        unexpected_loss
            .iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join("\n  ")
    );
    let healed: Vec<&(&str, &str)> = FORMATTER_ROUND_TRIP_GAPS
        .iter()
        .filter(|(d, note)| {
            !lost
                .iter()
                .any(|l| l.starts_with(&format!("{d} — {note}:")))
        })
        .collect();
    assert!(
        healed.is_empty(),
        "{} listed round-trip gap(s) now round-trip — delete their row:\n  {}",
        healed.len(),
        healed
            .iter()
            .map(|(d, note)| format!("{d}: {note}"))
            .collect::<Vec<_>>()
            .join("\n  ")
    );

    eprintln!("checked {checked} assembling probe(s) against the formatter");
    assert!(
        checked > 0,
        "no probe assembled — the framing has drifted and this checks nothing"
    );

    // A divergence is expected only if the dialect **and the reason** are on
    // the ledger. Keying on the dialect alone would swallow an unrelated
    // formatter failure in a dialect that already has a row, which is the
    // shape of blindness this test exists to end.
    let expected = |r: &str| {
        FORMATTER_GAPS
            .iter()
            .any(|(d, reason)| r.starts_with(&format!("{d} — ")) && r.contains(reason))
    };
    let unexpected: Vec<&String> = refused.iter().filter(|r| !expected(r)).collect();
    assert!(
        unexpected.is_empty(),
        "{} source(s) assemble and will not format, and are not on the ledger:\n  {}",
        unexpected.len(),
        unexpected
            .iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join("\n  ")
    );

    let fixed: Vec<&(&str, &str)> = FORMATTER_GAPS
        .iter()
        .filter(|(d, reason)| {
            !refused
                .iter()
                .any(|r| r.starts_with(&format!("{d} — ")) && r.contains(reason))
        })
        .collect();
    assert!(
        fixed.is_empty(),
        "{} listed formatter gap(s) now format — delete their row so the ledger stays honest:\n  {}",
        fixed.len(),
        fixed
            .iter()
            .map(|(d, n)| format!("{d}: {n}"))
            .collect::<Vec<_>>()
            .join("\n  ")
    );
}

/// Sources this crate assembles and will not format (#130).
///
/// **Empty**, and that is the point: every dialect now formats everything it
/// assembles. It stays because an empty ledger is the strictest form of the
/// test above — with no rows, *any* source that assembles and will not format
/// is reported.
///
/// Each row was `(dialect, the reason its formatter gives)`, both halves
/// matched so that a listed dialect failing for a *different* reason was still
/// reported. Every row was a macro: each dialect's formatter was written before
/// its macros were, and a definition reached a walk that could only read code.
/// Deleting a row is how a fix is recorded, and the test fails if a row
/// outlives the problem.
const FORMATTER_GAPS: &[(&str, &str)] = &[];

/// Sources this crate assembles, and formats, and then cannot assemble.
///
/// **Empty.** A weaker failure than a refusal and a worse one: the formatter
/// accepts the file, so nothing looks wrong until the result is built. With no
/// rows, any source whose program changes under `fmt` is reported.
///
/// Its two rows were both [#186] — ca65 makes a label with a colon and we
/// required column 0, so indenting a macro body, which is correct ca65 layout,
/// moved the label somewhere our own parser would not read it. Keyed by dialect
/// and probe note; a row that starts round-tripping fails the test, as on
/// [`FORMATTER_GAPS`].
///
/// [#186]: https://github.com/asm198x/asm198x/issues/186
const FORMATTER_ROUND_TRIP_GAPS: &[(&str, &str)] = &[];
