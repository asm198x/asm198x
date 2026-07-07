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
