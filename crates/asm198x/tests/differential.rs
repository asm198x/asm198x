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
use std::process::Command;

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
        "ca65-816" => "ca65",
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
    match dialect {
        "acme" => asm198x::assemble_acme(body).ok().map(|a| a.bytes),
        "pasmo" => asm198x::assemble_pasmo(body).ok().map(|a| a.bytes),
        "sjasmplus" => asm198x::assemble_sjasmplus(body).ok().map(|a| a.bytes),
        "z80n" => asm198x::assemble_sjasmplus_next(body).ok().map(|a| a.bytes),
        "lwasm" => asm198x::assemble_lwasm(body).ok().map(|a| a.bytes),
        "vasm" => asm198x::assemble_vasm(body).ok(),
        "ca65-816" => asm198x::assemble_ca65_816(body).ok().map(|a| a.bytes),
        _ => None,
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
    gap("acme", "operator ^ (power)",    " lda #5^3\n", 30),
    ok ("acme", "operator <<",           " lda #1<<3\n"),
    ok ("acme", "operator >>",           " lda #16>>2\n"),
    gap("acme", "directive !pet",        " !pet \"hi\"\n",     26),
    gap("acme", "directive !align",      " !align 255,0\n lda #1\n", 26),
    gap("acme", "directive !zone",       " !zone main\n rts\n", 26),
    gap("acme", "directive !set",        " !set n=5\n lda #n\n", 26),

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
    gap("sjasmplus", "directive byte",   " byte 1,2\n",    26),

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
    gap("lwasm", "directive fill",       " fill 0,4\n",     26),
    gap("lwasm", "directive zmb",        " zmb 4\n",        26),
    gap("lwasm", "directive fqb",        " fqb $12345678\n", 26),

    // ---- vasm / 68000 -------------------------------------------------------
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
    gap("vasm", "eori form eor #imm",    " eor.w #5,d0\n",   15),
    gap("vasm", "abs size suffix .w",    " move.w $1234.w,d0\n",     17),
    gap("vasm", "abs size suffix .l",    " move.l $12345678.l,d0\n", 17),

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
    gap("ca65-816", "directive .dword",  " .dword $12345678\n", 26),
    gap("ca65-816", "directive .dbyt",   " .dbyt $1234\n",      26),
    gap("ca65-816", "directive .asciiz", " .asciiz \"hi\"\n",   26),
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

    for bin in &skipped_tools {
        eprintln!("SKIP: `{bin}` not on PATH");
    }
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
