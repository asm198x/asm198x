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
// Currently unused: every tracked parser gap has been closed (the last batch —
// acme `!pet`/`!align`/`!zone`/`!set`, ca65 `.dword`/`.dbyt`/`.asciiz`,
// sjasmplus `byte`, lwasm `fill`/`zmb`/`fqb` — was issue #26). Kept so the next
// discovered gap is a one-line `gap(...)` entry rather than re-deriving the
// ledger's vocabulary.
#[allow(dead_code)]
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
        // ACME requires `*=` before code/data, so give `ours` the same `$0000`
        // origin the reference gets (both assemble at our fixed origin).
        "acme" => asm198x::assemble_acme(&format!("* = $0000\n{body}"))
            .ok()
            .map(|a| a.bytes),
        "pasmo" => asm198x::assemble_pasmo(body).ok().map(|a| a.bytes),
        "sjasmplus" => asm198x::assemble_sjasmplus(body).ok().map(|a| a.bytes),
        "z80n" => asm198x::assemble_sjasmplus_next(body).ok().map(|a| a.bytes),
        "lwasm" => asm198x::assemble_lwasm(body).ok().map(|a| a.bytes),
        "vasm" => asm198x::assemble_vasm(body).ok().map(|a| a.bytes),
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
    ok ("acme", "operator ^ (power)",    " lda #5^3\n"),
    ok ("acme", "keyword XOR",           " lda #5 XOR 1\n"),
    ok ("acme", "keyword EOR",           " lda #5 EOR 1\n"),
    ok ("acme", "operator <<",           " lda #1<<3\n"),
    ok ("acme", "operator >>",           " lda #16>>2\n"),
    ok ("acme", "directive !pet",        " !pet \"hi\"\n"),
    ok ("acme", "directive !align",      " !align 255,0\n lda #1\n"),
    ok ("acme", "directive !zone",       " !zone main\n rts\n"),
    ok ("acme", "directive !set",        " !set n=5\n lda #n\n"),

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

// ===========================================================================
// U2 — multi-file sjasmplus probes: the include mechanism against the real
// tool. Each probe gets its own SUBDIRECTORY so stale files from other probes
// (or earlier runs) can never leak into resolution.
// ===========================================================================

/// One multi-file fixture: a root file plus its includes, assembled by both
/// sides from a per-probe directory.
struct MultiProbe {
    note: &'static str,
    /// `(file name, contents)`; the first entry is the root.
    files: &'static [(&'static str, &'static str)],
}

const MULTI_PROBES: &[MultiProbe] = &[
    MultiProbe {
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
        note: "global defined inside the include rescopes the includer's locals",
        files: &[
            (
                "main.asm",
                "        org $8000\nstart:  nop\n        include \"glob.inc\"\n.tail:  nop\n        jr .tail\n",
            ),
            ("glob.inc", "mid:    nop\n"),
        ],
    },
];

#[test]
#[ignore = "needs sjasmplus; run with --ignored"]
fn multi_file_source_matches_reference() {
    if !have("sjasmplus") {
        eprintln!("SKIP: `sjasmplus` not on PATH");
        return;
    }
    let base = std::env::temp_dir().join("asm198x-differential-multi");
    let mut failures: Vec<String> = Vec::new();
    for (i, p) in MULTI_PROBES.iter().enumerate() {
        // A per-probe subdirectory, wiped before use, so resolution can never
        // pick up a stale file from another probe.
        let dir = base.join(format!("probe-{i}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("probe dir");
        for (name, contents) in p.files {
            fs::write(dir.join(name), contents).expect("write fixture");
        }
        let (root, _) = p.files[0];
        let out = dir.join("ref.bin");
        let status = Command::new("sjasmplus")
            .arg("--nologo")
            .arg(format!("--raw={}", out.display()))
            .arg(root)
            .current_dir(&dir)
            .output()
            .expect("run sjasmplus");
        if !status.status.success() {
            failures.push(format!(
                "{}: sjasmplus rejected the fixture: {}",
                p.note,
                String::from_utf8_lossy(&status.stderr)
            ));
            continue;
        }
        let reference = fs::read(&out).expect("reference bytes");

        let root_path = dir.join(root);
        let source = fs::read_to_string(&root_path).expect("read root");
        let loader = asm198x::source::FsLoader::new(&dir, Vec::new());
        match asm198x::assemble_sjasmplus_files(&source, &root_path.to_string_lossy(), &loader) {
            Ok(r) if r.bytes == reference => {}
            Ok(r) => failures.push(format!(
                "{}: bytes diverge — ours {:02X?} vs ref {:02X?}",
                p.note, r.bytes, reference
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
}
