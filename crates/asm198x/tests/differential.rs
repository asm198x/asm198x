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
// **The ledger is empty.** As of 2026-08-23 every probe here is an `ok` — no
// form in this file is one the reference accepts and we refuse. That is the
// 1.0 bar's item 4 (`decisions/v1-scope.md`), and it is a state to be kept
// rather than a milestone to be passed: the next gap anyone measures gets a
// marker below and this comment gets shorter.
//
// Closed in order: the U4d/#26 batch (acme `!pet`/`!align`/`!zone`/`!set`,
// ca65 `.dword`/`.dbyt`/`.asciiz`, sjasmplus `byte`, lwasm
// `fill`/`zmb`/`fqb`), then #67's conditional forms, then the two that were
// never conditional syntax — `:` as a statement separator (#98) and
// forward-symbol conditions (#99) — then #205 and #128.
//
// `gap` is unused today and kept anyway: deleting the mechanism because
// nothing is currently broken is how the next gap gets recorded as a comment
// instead of as a failing marker.
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
        // The 68000 multipass path (U6): the flat (-Fbin) and hunk-exe legs.
        "vasm-exe" => "vasmm68k_mot",
        "ca65-816" => "ca65",
        "ca65-huc6280" => "ca65",
        // The NES assemble+link path (U5): ca65 + ld65 with the fixed nes.cfg.
        "ca65-nes" => "ca65",
        "rgbasm" => "rgbasm",
        // Banked ROMs cannot use `-x` (it forces one 32K bank), so they get
        // their own leg with the padded `rgblink` recipe.
        "rgbasm-banked" => "rgbasm",
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
        "rgbasm" | "rgbasm-banked" => {
            let src = tmp.join("ref.asm");
            let obj = tmp.join("ref.o");
            fs::write(&src, body).ok()?;
            let _ = fs::remove_file(&obj);
            let mut a = Command::new("rgbasm");
            a.arg("-o").arg(&obj).arg(&src);
            let mut l = Command::new("rgblink");
            // `-x` keeps the image unpadded, which is what a ROM0-only program
            // should match. It also forces a single 32K bank, so a banked
            // program is linked without it and compared against the padded ROM.
            if dialect == "rgbasm" {
                l.arg("-x");
            }
            l.arg("-o").arg(&out).arg(&obj);
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
        "rgbasm" | "rgbasm-banked" => "SM83",
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
    ok ("acme", "operator % (modulo)",   " !byte 17 % 5\n"),
    ok ("acme", "modulo precedence",     " !byte 2 + 9 % 4 * 3\n"),
    ok ("acme", "modulo in a macro",     " !macro rem .a, .b { !byte .a % .b }\n+rem 17, 5\n"),
    ok ("acme", "operator &",            " lda #7&3\n"),
    ok ("acme", "operator |",            " lda #1|2\n"),
    ok ("acme", "operator ^ (power)",    " lda #5^3\n"),
    ok ("acme", "keyword XOR",           " lda #5 XOR 1\n"),
    ok ("acme", "keyword EOR",           " lda #5 EOR 1\n"),
    ok ("acme", "operator <<",           " lda #1<<3\n"),
    ok ("acme", "operator >>",           " lda #16>>2\n"),
    ok ("acme", "directive !pet",        " !pet \"hi\"\n"),
    ok ("acme", "directive !align",      " !align 255,0\n lda #1\n"),
    ok ("acme", "comparisons answer 1",  " !byte 2=2,2=3,2<>3,2<3,2>3\n"),
    ok ("acme", "prefix and infix < together", " !byte <$1234,2<3\n"),
    ok ("acme", "directive !fill",       " !fill 4\n lda #1\n"),
    ok ("acme", "!fill with a value",    " !fill 3,$ff\n lda #1\n"),
    // `!fi` is ACME's short spelling of `!fill`, not a conditional terminator.
    ok ("acme", "!fi is short for !fill", " !fi 3,$ff\n lda #1\n"),
    // Value ranges, at the corners each reference actually draws them
    // (asm198x#290). Only accepted forms are probed — the harness treats a
    // reference rejection as out of scope, which is exactly right here: the
    // refusals are asserted in each dialect's own tests, and what needs a
    // recorded fact is the *bytes* a reference produces for a value we used
    // to refuse.
    ok ("acme", "negative byte",         " !byte -1\n"),
    ok ("acme", "negative word",         " !word -1\n"),
    ok ("acme", "most negative word",    " !word -32768\n"),
    ok ("acme", "!be16 / !le16",         " !be16 $1234\n !le16 $1234\n"),
    ok ("acme", "!be24 / !le24",         " !be24 $123456\n !le24 $123456\n"),
    ok ("acme", "!be32 / !le32",         " !be32 $12345678\n !le32 $12345678\n"),
    ok ("acme", "sized takes a list",    " !be16 $1234, $5678\n"),
    ok ("acme", "sized spans signed",    " !be16 -32768\n !be16 65535\n"),
    ok ("acme", "!while tests first",    " !set i=0\n !while i<3 {\n !byte i\n !set i=i+1\n }\n"),
    ok ("acme", "!do tests after",       " !set i=5\n !do {\n !byte i\n } while i<3\n"),
    ok ("acme", "!do until inverts",     " !set i=0\n !do {\n !byte i\n !set i=i+1\n } until i=3\n"),
    ok ("acme", "loops nest",            " !set i=0\n !while i<2 {\n !set j=0\n !while j<2 {\n !byte i*10+j\n !set j=j+1\n }\n !set i=i+1\n }\n"),
    ok ("acme", "!pseudopc moves labels", " !pseudopc $2000 {\nfoo\n !byte <foo, >foo\n }\n"),
    ok ("acme", "!pseudopc keeps bytes",  " !byte $11\n !pseudopc $2000 {\n !byte $22\n }\n !byte $33\n"),
    ok ("acme", "!pseudopc restores",     " !pseudopc $2000 {\n !byte <*, >*\n }\n !byte <*, >*\n"),
    // Measured from the claimed address, not the real one.
    ok ("acme", "branch inside !pseudopc", " !pseudopc $2000 {\nl\tnop\n bne l\n }\n"),
    ok ("acme", "!addr binds a value",    " !addr foo = $c000\n lda foo\n"),
    ok ("acme", "!addr keeps zero page",  " !addr bar = $10\n lda bar\n"),
    // Without a value it is a label — the program counter, not a declaration.
    ok ("acme", "!addr with no value",    " !byte 1,2,3\n !addr foo\n !byte <foo, >foo\n"),
    ok ("acme", "!ct pet converts !text", " !ct pet\n !text \"aA[]@\"\n"),
    ok ("acme", "!ct scr converts !text", " !ct scr\n !text \"aA[]@\"\n"),
    // The pair the earlier "!raw agrees with !text" probe anticipated: with a
    // table named they no longer agree, and both halves are recorded.
    ok ("acme", "!raw ignores the table", " !ct pet\n !text \"ab\"\n !raw \"ab\"\n"),
    ok ("acme", "!ct block restores",     " !ct pet {\n !text \"a\"\n }\n !text \"a\"\n"),
    ok ("acme", "!xor masks a block",    " !xor $ff {\n !text \"ab\"\n }\n"),
    ok ("acme", "!xor masks the opcode", " !xor $ff {\n nop\n }\n"),
    ok ("acme", "!xor block restores",   " !xor $ff {\n !byte 1\n }\n !byte 1\n"),
    ok ("acme", "!xor masks combine",    " !xor $f0\n !byte 0\n !xor $0f\n !byte 0\n"),
    // The two that say what the mask is *for*: bytes the source wrote.
    ok ("acme", "!xor masks !fill",      " !xor $ff {\n !fill 2\n }\n"),
    ok ("acme", "!xor spares !skip",     " !xor $ff {\n !skip 2\n }\n"),
    ok ("acme", "directive !as / !rs",   " !as\n !rs\n lda #1\n"),
    ok ("acme", "directive !eof",        " nop\n !eof\n !!!garbage\n"),
    ok ("acme", "directive !endoffile",  " nop\n !endoffile\n lda\n"),
    ok ("acme", "directive !scrxor",     " !scrxor $80, \"ab\"\n"),
    // The pair that proves `!scrxor` is not `!xor` around `!scr`: the number
    // is neither converted nor masked.
    ok ("acme", "!scrxor spares numbers", " !scrxor $80, \"a\", 65, \"b\"\n"),
    ok ("acme", "!scrxor mask truncates", " !scrxor 511, \"a\"\n"),
    ok ("acme", "directive !skip",       " !skip 4\n lda #1\n"),
    ok ("acme", "!initmem fills !skip",  " !initmem $ff\n !skip 3\n"),
    // The one that says `!initmem` is not positional: the reservation is
    // written first and still takes the value.
    ok ("acme", "!initmem is not positional", " !skip 3\n !initmem $ff\n"),
    ok ("acme", "!initmem fills an org gap",  " !initmem $ff\n nop\n *=$0004\n nop\n"),
    ok ("acme", "backwards origins are placed by address",
        " *=$1004\n !byte $44\n *=$1000\n !byte $11\n *=$1002\n !byte $22\n"),
    ok ("acme", "a later overlapping region overwrites earlier bytes",
        " *=$1000\n !byte $11,$22\n *=$1001\n !byte $33\n"),
    ok ("acme", "initmem fills gaps between out-of-order regions",
        " !initmem $aa\n *=$1004\n !byte $44\n *=$1000\n !byte $11\n"),
    ok ("lwasm", "byte truncates up",    " fcb 256\n"),
    ok ("lwasm", "byte truncates down",  " fcb -129\n"),
    ok ("lwasm", "negative word",        " fdb -1\n"),
    ok ("lwasm", "word truncates",       " fdb 65536\n"),
    ok ("sjasmplus", "byte truncates",   " db 256\n"),
    ok ("sjasmplus", "negative word",    " dw -1\n"),
    ok ("sjasmplus", "operand truncates", " ld a,256\n"),
    ok ("pasmo", "byte truncates",       " defb 256\n"),
    ok ("pasmo", "negative word",        " defw -1\n"),
    ok ("pasmo", "operand truncates",    " ld a,256\n"),
    // Fixed-point literals and the exactly-defined half of rgbasm's maths.
    // `1.0` is `$10000`, and the fraction truncates toward zero rather than
    // rounding, so `3.7` is `$3B333`.
    // sjasmplus's data directives beyond the shared Z80 set: widths, the two
    // terminator conventions, hex digit pairs and bit graphics.
    ok ("sjasmplus", "the wider data directives",
        "\tword $1234\n\tdword $12345678\n\tdd $12345678\n\tdefd $12345678\n\td24 $123456\n"),
    ok ("sjasmplus", "dz terminates and dc marks each string's last character",
        "\tdz \"ab\"\n\tdc \"ab\",3,\"cd\"\n\tdz 1,2\n"),
    ok ("sjasmplus", "hex digit pairs, with commas and without",
        "\tdh 11,22\n\thex 3344\n\tdefh 1122\n"),
    ok ("sjasmplus", "bit graphics, eight characters to a byte",
        "\tdg #-#-#-#-\n\tdefg ..##..##\n\tdg #-------#------#\n"),
    ok ("sjasmplus", "abyte adds an offset, and its two suffixed forms",
        "\tabyte 4 1,2\n\tabytec 0 \"ab\"\n\tabytez 4 1,2\n"),
    ok ("sjasmplus", "block fills, with a byte and without",
        "\tblock 3,$AA\n\tblock 2\n"),
    // The text layer: string symbols and string functions, resolved before the
    // parse. The index conventions are the part worth pinning — `STRSUB` is
    // 1-based with a length and `STRSLICE` 0-based with an end, and the two
    // searches differ in both base and miss value.
    ok ("rgbasm", "the string functions fold to text and numbers",
        "SECTION \"s\",ROM0[0]\ndb STRCAT(\"ab\",\"cd\")\n\
         db STRUPR(\"ab\"), STRLWR(\"CD\")\ndb STRSUB(\"abcd\", 2, 2)\n\
         db STRSLICE(\"abcd\", 1, 3)\ndb STRLEN(\"abc\")\n\
         db STRRPL(\"abab\",\"b\",\"X\")\n"),
    ok ("rgbasm", "the searches differ in base and in what a miss answers",
        "SECTION \"s\",ROM0[0]\n\
         db STRFIND(\"abc\",\"b\"), STRIN(\"abc\",\"b\"), STRRIN(\"abab\",\"b\")\n\
         db STRFIND(\"abc\",\"z\"), STRIN(\"abc\",\"z\")\n\
         db STRCMP(\"a\",\"b\"), STRCMP(\"b\",\"a\"), STRCMP(\"a\",\"a\")\n"),
    ok ("rgbasm", "string functions nest",
        "SECTION \"s\",ROM0[0]\ndb STRLEN(STRCAT(\"ab\",\"cd\"))\n\
         db STRUPR(STRSUB(\"abcd\", 2, 2))\n"),
    ok ("rgbasm", "EQUS substitutes text, and {} reaches inside a token",
        "SECTION \"s\",ROM0[0]\nDEF s EQUS \"$41\"\ndb s\n\
         DEF n EQUS \"4\"\ndb $1{n}\ndb \"s in a string\"\n"),
    ok ("rgbasm", "an EQUS may hold a quoted string, or a call",
        "SECTION \"s\",ROM0[0]\nDEF q EQUS \"\\\"ab\\\"\"\ndb STRLEN(q)\n\
         DEF j EQUS \"STRCAT(\\\"xy\\\",\\\"z\\\")\"\ndb j\n"),
    // `STRFMT` is printf's shape and not printf's rules: the flags come in a
    // fixed order, `#` marks the base rather than C's alternate form, and `%f`
    // reads a Q16.16 value — so a plain `1` is a very small fraction.
    ok ("rgbasm", "STRFMT's types, flags and widths",
        "SECTION \"s\",ROM0[0]\ndb STRFMT(\"%d|%u|%X|%x|%b|%o\", 42, -1, 255, 255, 5, 9)\n\
         db STRFMT(\"[%s]|100%%\", \"hi\")\n\
         db STRFMT(\"%5d|%05d|%-5d|%+d|%+06d|%06d\", 42, 42, 42, 42, 42, -42)\n\
         db STRFMT(\"%#x|%#b|%#o|%+#08x|% -6d|\", 255, 5, 9, 5, 42)\n"),
    ok ("rgbasm", "STRFMT reads %f as a fixed-point value",
        "SECTION \"s\",ROM0[0]\ndb STRFMT(\"%f|%f|%f\", 1.5, -1.5, 1)\n\
         db STRFMT(\"%.2f|%#f|%#014f\", 1.5, 1.5, 1.5)\n\
         db STRFMT(\"%.0f %.0f %.0f %.0f\", 0.5, 1.5, 2.5, -0.5)\n\
         db STRFMT(\"%.17f\", 0.1)\n"),
    // The pass folds a constants environment as it walks, so a number
    // argument may be a constant defined above the line, or an expression
    // over one.
    ok ("rgbasm", "a folded number may come from a constant above it",
        "SECTION \"s\",ROM0[0]\nDEF N EQU 7\ndb STRFMT(\"n=%d|%d\", N, N*2+1)\n\
         DEF M EQU N-5\ndb STRSUB(\"abcd\", M, 2)\n"),
    ok ("rgbasm", "fixed-point literals",
        "SECTION \"s\",ROM0[0]\ndl 3.7\ndl 1.0\ndl -1.5\ndl 0.5\ndl 0.1\ndl 0.3\n"),
    ok ("rgbasm", "digit separators in every radix and fixed-point component",
        "SECTION \"s\",ROM0[0]\ndb %1111_0000,%_10,$A_B,$_C,&1_7,&_7,2_5_5\n\
         dl 1_2.2_5,1.2_5q8\n"),
    ok ("rgbasm", "the RS offset counter and byte word long definitions",
        "SECTION \"s\",ROM0[0]\ndb _RS\nrsreset\nDEF Foo RB\nDEF Bar RB 2\nDEF Cat RW\n\
         DEF Dog RW 2\nDEF Elk RL\nDEF Fox RL 2\ndb Foo,Bar,Cat,Dog,Elk,Fox,_RS\n\
         rsset 7\nDEF Gap RB 0\nDEF Hat RB\ndb Gap,Hat,_RS\n"),
    ok ("rgbasm", "DS count relative to the live location counter",
        "SECTION \"s\",ROM0[$100]\ndb $11\nHere: ds $105-@,$aa\nEmpty: ds @-@,$bb\ndb $55\n"),
    ok ("rgbasm", "STARTOF memory regions and forward placed sections",
        "SECTION \"probe\",ROM0[0]\ndw STARTOF(OAM),STARTOF(\"later\"),STARTOF(\"pinned\")\n\
         SECTION \"later\",ROM0\ndb 1,2,3\nSECTION \"pinned\",ROM0[$200]\ndb 4\n"),
    ok ("rgbasm", "graphics literals pack two-bit pixels into tile bitplanes",
        "SECTION \"s\",ROM0[0]\ndw `0,`1,`2,`3,`01,`10,`0123,`3210\n\
         dw `00000000,`11111111,`22222222,`33333333,`01230123,`32103210\n\
         dw `0_1,`_01,`333333333,`0123+1,(`3210<<1)\n"),
    ok ("rgbasm", "operandless data reserves one element in ROM and RAM",
        "SECTION \"rom\",ROM0[0]\nStart: db\nAfterByte: dw\nAfterWord: dl\nAfterLong: db $aa\n\
         SECTION \"vars\",WRAM0\nRamStart: db\nRamByte: dw\nRamWord: dl\nRamLong:\n\
         SECTION \"addresses\",ROM0[$10]\ndw RamStart,RamByte,RamWord,RamLong\n\
         db RamByte-RamStart,RamWord-RamByte,RamLong-RamWord\n"),
    ok ("rgbasm", "floating WRAM sections use RGBLINK placement order",
        "SECTION \"Zed\",WRAM0\nZed: ds 2\nSECTION \"Alpha\",WRAM0\nAlpha: ds 2\n\
         SECTION \"Middle\",WRAM0\nMiddle: ds 2\nSECTION \"Pinned\",WRAM0[$c008]\nPinned: ds 2\n\
         SECTION \"Small\",WRAM0\nSmall: ds 1\nSECTION \"rom\",ROM0[0]\n\
         dw Zed,Alpha,Middle,Pinned,Small\n"),
    ok ("rgbasm", "DEF predicates and short-circuit logical operators",
        "SECTION \"s\",ROM0[0]\nDEF Foo EQU 7\nDEF foo EQU 9\n\
         db DEF(Foo),DEF(foo),DEF(FOO),!DEF(Missing)\n\
         db 0||0&&1,1||0&&0,!0,!7,!!7,DEF(Foo)&&Foo==7,DEF(Missing)||5\n\
         Before: db 0\ndb DEF(Before),DEF(After)\nAfter: db 0\n\
         db 1||Unknown,0&&Unknown\n\
         db DEF(__RGBDS_MAJOR__),__RGBDS_MAJOR__,__RGBDS_MINOR__,__RGBDS_PATCH__\n\
         IF !DEF(Guard)\nDEF Guard EQU 1\ndb $aa\nENDC\n\
         IF DEF(Guard)&&!DEF(Missing)\ndb $bb\nENDC\n"),
    ok ("rgbasm", "a q suffix names another precision",
        "SECTION \"s\",ROM0[0]\ndl 3.7q8\ndl 1.0q4\ndl 0.25q1\ndl 1.25q1\ndl 0.125q2\n"),
    ok ("rgbasm", "fixed multiply and divide",
        "SECTION \"s\",ROM0[0]\ndl DIV(1.0, 3.0)\ndl MUL(1.5, 1.5)\ndl FMOD(7.5, 2.0)\n"),
    // The three roundings, each over a negative as well: FLOOR goes toward
    // minus infinity, CEIL away from it, and ROUND sends a half away from zero.
    ok ("rgbasm", "fixed rounding, both signs",
        "SECTION \"s\",ROM0[0]\ndl FLOOR(3.7)\ndl FLOOR(-3.2)\ndl CEIL(3.2)\n\
         dl CEIL(-3.2)\ndl ROUND(3.5)\ndl ROUND(-3.5)\n"),
    ok ("rgbasm", "byte extraction and trailing zeros",
        "SECTION \"s\",ROM0[0]\ndb HIGH($1234), LOW($1234)\ndl TZCOUNT(8)\ndl TZCOUNT(1)\n"),
    ok ("rgbasm", "dl is 32-bit little-endian",
        "SECTION \"s\",ROM0[0]\ndl $12345678, -1\n"),
    ok ("rgbasm", "byte truncates up",   "SECTION \"s\",ROM0[0]\ndb 256\n"),
    ok ("rgbasm", "byte truncates down", "SECTION \"s\",ROM0[0]\ndb -129\n"),
    ok ("rgbasm", "negative word",       "SECTION \"s\",ROM0[0]\ndw -1\n"),
    ok ("rgbasm", "word truncates",      "SECTION \"s\",ROM0[0]\ndw 65536\n"),
    ok ("rgbasm", "operand truncates",   "SECTION \"s\",ROM0[0]\nld a,300\n"),
    // `!raw` bypasses the conversion table where `!text` honours it. Both are
    // probed so the pair stays tied: today they agree, and when `!ct` lands
    // only one of them may change.
    ok ("acme", "directive !raw",        " !raw \"ab\", 3\n"),
    ok ("acme", "!raw agrees with !text",  " !text \"ab\"\n !raw \"ab\"\n"),
    ok ("acme", "directive !hex",        " !hex 0f1e2d\n"),
    ok ("acme", "!hex is case-blind",    " !hex 0F1E2D\n"),
    // Pairing is per whitespace-separated token: `!hex 0f 1e` is two bytes and
    // `!hex 0 f` is an error, though both hold two digits.
    ok ("acme", "!hex pairs per token",  " !hex 0f 1e 2d\n"),
    ok ("acme", "directive !scr",        " !scr \"abc\"\n"),
    ok ("acme", "!scr differs from !pet"," !scr \"@[]\"\n !pet \"@[]\"\n"),
    ok ("acme", "!for counts a block",   " !for i, 1, 3 {\n lda #i\n }\n"),
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
    // pasmo alone bounds a constant, and only upward: `$FFFF` assembles,
    // `$10000` does not, and a negative is fine (#228).
    ok ("pasmo", "a 16-bit constant",     "V equ $FFFF\n ld hl,V\n"),
    ok ("pasmo", "a negative constant",   "V equ -5\n ld a,V\n"),
    ok ("pasmo", "defw / dw",            " defw $1234\n dw $5678\n"),
    ok ("pasmo", "defs / ds reserve",    " ld a,1\n defs 3\n ds 2\n ld b,2\n"),
    ok ("pasmo", "if taken",             " if 1\n nop\n endif\n ret\n"),
    ok ("pasmo", "if not taken",         " if 0\n nop\n endif\n ret\n"),
    ok ("pasmo", "if / else",            " if 0\n nop\n else\n ret\n endif\n"),
    ok ("pasmo", "rept repeats a body",  " rept 3\n nop\n endm\n ret\n"),
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
    ok ("pasmo", "operator ~",           " db ~0\n dw ~~1\n"),

    // ---- sjasmplus / z80 ----------------------------------------------------
    ok ("sjasmplus", "hex $ / 0x / h",   " ld a,$10\n ld b,0x10\n ld c,10h\n"),
    ok ("sjasmplus", "binary 0b / %",    " ld a,0b1010\n ld b,%1010\n"),
    ok ("sjasmplus", "db / dw / defb",   " db 1,2,3\n dw $1234\n defb 4\n"),
    ok ("sjasmplus", "hex # prefix",     " ld a,#10\n"),
    ok ("sjasmplus", "operator <<",      " ld a,1<<2\n"),
    ok ("sjasmplus", "operator &",       " ld a,5 & 3\n"),
    ok ("sjasmplus", "operator ^",       " ld a,6 ^ 3\n"),
    // Native 1.21.0 gives 7f 7f ff f8 ff fb ff ff ff 01 00 ff ff: `~` is
    // an i64 two's-complement unary, binds before shifts/addition, composes,
    // and is truncated only when the data directive writes it (#475).
    ok ("sjasmplus", "operator ~ semantics",
        " db ~(1<<7)\n dw ~(1<<7)\n dw ~1<<2\n dw ~(1<<2)\n dw ~1+1\n dw ~~1\n dw ~0\n"),
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
    // Re-filed out of #67 (2026-08-19): neither was conditional syntax. `:`
    // failed between plain instructions too, so it was a line-model change,
    // done as one (#98, `decisions/colon-separated-statements.md`). The
    // forward label was a resolution-order property needing the reference's
    // three passes, adopted as those (#99,
    // `decisions/forward-conditions-and-passes.md`).
    ok ("sjasmplus", "colon-inline conditional",
        " IF 1 : ld a,1 : ENDIF\n"),
    ok ("sjasmplus", "colon between plain instructions",
        " ld a,1 : ld b,2\n ld a,1:ld b,2\n"),
    ok ("sjasmplus", "a label keeps its colon",
        "lbl: ld a,1 : ld b,2\n djnz lbl\n"),
    ok ("sjasmplus", "a colon in a literal separates nothing",
        " db \":\" : db 1\n db ':' : db 2\n"),
    ok ("sjasmplus", "local and export label colons",
        "glob:\n.l: ld a,1 : ld b,2\ngl:: ld a,2 : ld b,3\n"),
    ok ("sjasmplus", "colon-inline untaken branch",
        " IF 0 : ld a,1 : ENDIF\n ld b,2\n"),
    ok ("sjasmplus", "IF on a forward label (multi-pass)",
        " IF later\n ld a,1\n ENDIF\nlater: nop\n"),
    ok ("sjasmplus", "a forward condition that changes the answer",
        " IF later = 0\n ld a,1\n ENDIF\nlater: nop\n"),
    ok ("sjasmplus", "a forward condition that never settles",
        " IF later < 2\n ld a,1\n ENDIF\nlater: nop\n"),
    // The optional leading dot, which sjasmplus takes on every directive it
    // has. The conditionals already had it (#67); this is the rest.
    ok ("sjasmplus", "dotted data and origin directives",
        " .org $10\n .db 1,2\n .defb 3\n .byte 4\n .dw $1234\n .ds 2\n"),
    ok ("sjasmplus", "dotted equ and define",
        "x .equ 5\n .define V 6\n .db x,V\n"),
    ok ("sjasmplus", "dotted macro and repetition",
        " .macro m\n .db 7\n .endm\n m\n .dup 2\n .db 8\n .edup\n"),
    ok ("sjasmplus", "dotted module",
        " .module foo\nbar: .db 1\n .endmodule\n .db foo.bar\n"),

    ok ("sjasmplus", "a backward condition needs no pass",
        "later: nop\n IF later\n ld a,1\n ENDIF\n"),

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

    // #126: the refusals cannot be probed here — this harness skips a body the
    // reference rejects — so what is arbitrated is the other half, that the
    // two shapes which *look* like duplicates still assemble.
    // `cnop offset,alignment` aligns up, then adds the offset; the pad is a
    // whole NOP word where one fits, with a leading $00 when it does not.
    ok ("vasm", "cnop on the boundary",  "\tcnop 0,4\n\tdc.b $99\n"),
    ok ("vasm", "cnop pads odd with 00+NOP", "\tdc.b $11\n\tcnop 0,4\n\tdc.b $99\n"),
    ok ("vasm", "cnop pads even with NOP", "\tdc.b $11,$22\n\tcnop 0,4\n\tdc.b $99\n"),
    ok ("vasm", "cnop one short takes 00", "\tdc.b $11,$22,$33\n\tcnop 0,4\n\tdc.b $99\n"),
    ok ("vasm", "cnop offset adds past the boundary", "\tdc.b $11\n\tcnop 2,4\n\tdc.b $99\n"),
    ok ("vasm", "cnop to an eight boundary", "\tdc.b $11\n\tcnop 0,8\n\tdc.b $99\n"),
    // Comparisons. vasm answers $FF for true where the 6502 family answers 1
    // (`docs/comparison-operators.md`).
    ok ("vasm", "comparisons answer $FF",
        "\tdc.b 2=2,2=3,2<>3,2<3,2>3,2<=3,2>=3\n"),
    ok ("vasm", "a comparison binds looser than arithmetic",
        "\tdc.b 1+1=2,2*2>3\n"),
    ok ("vasm", "assert with a comparison",  "\tassert 2=2\n\tdc.b 9\n"),
    // Print-style directives emit nothing; the bytes either side are the test.
    ok ("vasm", "echo emits nothing",    "\techo \"n=\",5\n\tdc.b 1,2\n"),
    // The seven visibility words emit nothing when their name is defined —
    // the only shape that assembles in binary output. `comm`'s second operand
    // is a size, not a name, and it reserves nothing here.
    ok ("vasm", "visibility emits nothing",
        "\txdef foo\n\tpublic foo\n\tglobal foo\n\texport foo\n\tentry foo\n\tweak foo\n\
         \textrn foo\n\tcomm foo,4\n\tlocal foo\n\tidnt \"mod\"\nfoo:\tdc.b 1,2\n"),
    ok ("vasm", "a true assertion is silent",  "\tassert 1\n\tdc.b 1\n"),
    ok ("vasm", "even pads to a word",   "\tdc.b 1\n\teven\n\tdc.b 2\n"),
    ok ("vasm", "even on a word does nothing", "\tdc.b 1,2\n\teven\n\tdc.b 3\n"),
    ok ("vasm", "dcb repeats a value",   "\tdcb 3,$aa\n"),
    ok ("vasm", "dcb.w and a default 0", "\tdcb.w 2,$1234\n\tdcb.b 2\n"),
    ok ("vasm", "distinct labels assemble",
        " nop\nlbl nop\n"),
    ok ("vasm", "locals repeat under different globals",
        "a nop\n.l nop\nb nop\n.l nop\n"),

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
    // `ALIGN` here is power-of-two only, defaults to 4 with no operand, and
    // takes an optional fill byte.
    // The device model. Two pages written at one address concatenate rather
    // than colliding, because they are different memory
    // (`docs/sjasmplus-device-model.md`).
    ok ("sjasmplus", "two pages at one address concatenate",
        " DEVICE ZXSPECTRUM128\n SLOT 3\n PAGE 1\n ORG $C000\n db $11\n PAGE 2\n db $22\n"),
    ok ("sjasmplus", "a device changes no bytes",
        " DEVICE ZXSPECTRUM48\n ORG $8000\n ld a,1\n ret\n"),
    ok ("sjasmplus", "DEVICE NONE is no device",
        " DEVICE NONE\n ld a,1\n"),
    ok ("sjasmplus", "the Next has eight slots",
        " DEVICE ZXSPECTRUMNEXT\n SLOT 7\n PAGE 223\n db 1\n"),
    // `ASSERT` passes silently and reaches forward to labels below it.
    ok ("sjasmplus", "comparisons answer $FF",
        " db 2=2,2==2,2!=3,2<3,2>3,2<=3,2>=3\n"),
    ok ("pasmo", "comparisons answer $FF",
        " ld a,2=2\n ld b,2!=3\n ld c,2<3\n ld d,2>3\n ld e,2<=3\n ld h,2>=3\n"),
    ok ("sjasmplus", "DISPLAY emits nothing",
        " DISPLAY \"hi \", 5\n db 1,2\n"),
    ok ("sjasmplus", "a true assertion is silent",
        " ORG 0\n ASSERT 1\n db 1\n"),
    ok ("sjasmplus", "an assertion sees a later label",
        " ORG 0\nbeg: db 1,2\nfin:\n ASSERT fin-beg\n"),
    ok ("sjasmplus", "align pads to the boundary",
        " db 1\n align 4\n db 2\n"),
    ok ("sjasmplus", "align defaults to 4",
        " db 1\n align\n db 2\n"),
    ok ("sjasmplus", "align takes a fill byte",
        " db 1\n align 4,$ff\n db 2\n"),
    ok ("sjasmplus", "align on the boundary pads nothing",
        " db 1,2,3,4\n align 4\n db 9\n"),
    ok ("sjasmplus", "align 1 pads nothing",
        " db 1\n align 1\n db 2\n"),
    ok ("sjasmplus", "DUP inside a macro",
        " MACRO m\n DUP 2\n nop\n EDUP\n ENDM\n m\n"),
    // #477: STRUCT/ENDS — sizes, member offsets, and instantiation, each
    // shape probed against 1.21.0 before landing.
    ok ("sjasmplus", "STRUCT binds size and member offsets",
        " STRUCT Pt\nx BYTE 0\ny BYTE 0\nw WORD 0\n ENDS\n db Pt, Pt.x, Pt.y, Pt.w\n"),
    ok ("sjasmplus", "STRUCT reserves via DS times count",
        " STRUCT S\na BYTE 0\nb BYTE 0\n ENDS\nbuf ds S * 3\n db 9\n"),
    ok ("sjasmplus", "STRUCT with an initial offset",
        " STRUCT P, 4\nf BYTE 0\n ENDS\n db P, P.f\n"),
    ok ("sjasmplus", "STRUCT member spellings DB DW DS and no-init",
        " STRUCT B\nactive BYTE\npAddr WORD\npSeq DW $0000\nloop DB $00\npad DS 2\n ENDS\n db B, B.active, B.pAddr, B.pSeq, B.loop, B.pad\n"),
    ok ("sjasmplus", "STRUCT embeds a structure and flattens its paths",
        " STRUCT H\nx0 BYTE 0\ny0 BYTE 0\n ENDS\n STRUCT B\nn BYTE 0\nhb H\n ENDS\n db B, B.n, B.hb, B.hb.x0, B.hb.y0\n"),
    ok ("sjasmplus", "STRUCT is referenced before its definition",
        " db Foo\n STRUCT Foo\nf BYTE 0\n ENDS\n"),
    ok ("sjasmplus", "STRUCT under a module takes its prefix",
        " MODULE m\n STRUCT S\nf BYTE 0\ng WORD 0\n ENDS\n db S, S.g\n ENDMODULE\n db m.S, m.S.g\n"),
    ok ("sjasmplus", "STRUCT re-anchors the locals that follow it",
        "glob:\n STRUCT S\nf BYTE 0\n ENDS\n.after: db 2\n dw S.after\n"),
    ok ("sjasmplus", "STRUCT instantiation emits defaults and binds addresses",
        " STRUCT Pt\nx BYTE 5\nw WORD $1234\n ENDS\n STRUCT Box\ntl Pt\nn BYTE 9\n ENDS\np1 Pt\nb1 Box\n dw p1, p1.x, p1.w, b1.tl.w, b1.n\n"),
    ok ("sjasmplus", "STRUCT sizes fold in ASSERT",
        " STRUCT V\nf BYTE 0\ng BYTE 0\n ENDS\n ASSERT V.f == 0\n ASSERT V.g == 1\n ASSERT V == 2\n db 7\n"),
    ok ("sjasmplus", "STRUCT wide member types D24 and DWORD",
        " STRUCT U\nf D24 0\ng DWORD 0\n ENDS\n db U, U.f, U.g\n"),
    ok ("sjasmplus", "STRUCT unlabeled member reserves without a name",
        " STRUCT R\n BYTE 9\nf BYTE 0\n ENDS\n db R, R.f\n"),
    // #528: a DS count resolved across the passes, each shape probed
    // against 1.21.0 before landing.
    ok ("sjasmplus", "DS count reaches a later EQU",
        "buf DS COUNT * 2\n nop\nCOUNT EQU 3\n"),
    ok ("sjasmplus", "DS count on a moving label stops at pass three",
        " DS later+1\nlater: nop\n"),
    ok ("sjasmplus", "DS count that swings between passes",
        " DS 3-later\nlater: nop\n"),
    ok ("sjasmplus", "DS DEFS and BLOCK take a forward count and fill",
        " DS COUNT, $FF\n DEFS 2, FILL\n BLOCK 1\n nop\nCOUNT EQU 2\nFILL EQU $AA\n"),
    ok ("sjasmplus", "DS count uses the location counter",
        " DS $+2\n nop\n"),
    ok ("sjasmplus", "STRUCT member DS reaches a later EQU",
        " STRUCT Pt\nx DS N\n ENDS\n DB Pt\nN EQU 4\n"),
    ok ("sjasmplus", "DS count in a taken branch reaches forward",
        " IF 1\n DS COUNT\n ENDIF\n nop\nCOUNT EQU 2\n"),
    // #533: the accumulator left implicit on ADD/ADC/SBC, probed against
    // 1.21.0 with and without --syntax=abfw.
    ok ("sjasmplus", "ADD ADC SBC take a lone operand as A,operand",
        " add (hl)\n add b\n add 5\n adc c\n sbc (ix+1)\n add a\n sbc a\n adc 200\n add (iy-3)\n add ixh\n"),
    ok ("sjasmplus", "ADD ADC SBC lone operand under abfw",
        " opt --syntax=abfw\n add (hl)\n adc c\n sbc (ix+1)\n"),
    ok ("sjasmplus", "ADD ADC SBC two-operand forms stay 16-bit",
        " add hl,de\n adc hl,de\n sbc hl,de\n add ix,bc\n add a,(hl)\n"),
    // #548: a STRUCT instance with an initialiser list, probed against
    // 1.21.0 — values fill the members in order at each member's width, an
    // empty or missing slot keeps the default, DS members take no slot.
    ok ("sjasmplus", "STRUCT instance initialiser list",
        " STRUCT Hitbox\nx0 BYTE 0\nx1 BYTE 0\ny0 BYTE 1\ny1 BYTE 0\n ENDS\n\
         a Hitbox { $02, $0e, $00, $07 }\nb Hitbox\nc Hitbox { $11, $22 }\nd Hitbox { , $33 }\n\
         e Hitbox { }\nf Hitbox { $11, }\n Hitbox { $00, $02, $00, $06 }\n ld a,(c.x1)\n"),
    ok ("sjasmplus", "STRUCT instance initialiser list without braces",
        " STRUCT Hitbox\nx0 BYTE 0\nx1 BYTE 0\ny0 BYTE 1\ny1 BYTE 0\n ENDS\n\
         a Hitbox $2, $d, $0, $7\n Hitbox $3, $d, $0, $7\nb Hitbox $11, $22\nc Hitbox , $33\n"),
    ok ("sjasmplus", "STRUCT initialiser values take the member width",
        " STRUCT P\nw WORD 0\nb BYTE 0\nt D24 0\nq DWORD 0\n ENDS\n\
         i P { $1234, $56, $123456, $12345678 }\nj P { $1234 }\n"),
    ok ("sjasmplus", "STRUCT initialiser values are expressions and DS takes no slot",
        " STRUCT Rec\na BYTE 1\npad DS 2\nb BYTE 2\n ENDS\nn equ 3\n\
         c Rec { n+1, later*2 }\nlater equ $12\n"),
    // #551: column 0 is the label column without exception in sjasmplus,
    // probed against 1.21.0 — a mnemonic, a directive or a dotted `.end`
    // there binds a label and the rest of the line is the operation; text
    // after a `:` separator is in the operation field.
    ok ("sjasmplus", "column-0 .end is a local label, not END",
        "top\tnop\n.end\tld (top),a\n\tjr .end\n\tnop\n"),
    ok ("sjasmplus", "column-0 mnemonics and directives are labels",
        "top\tnop\nnop\tnop\nend\tnop\ndb\tnop\n\tdw nop,end,db\n"),
    ok ("sjasmplus", "a statement after a colon is in the operation field",
        "top\tnop : ld a,1 : ld b,2\nnext:\tld a,(top) : ld c,3\n"),
    // #552: a STRUCT initialiser list across lines, and a nested `{ … }`
    // group for an embedded member, probed against 1.21.0 — a line end
    // ends a value, a comma is taken only on its own line, and a group
    // fills the embedded member's slots then hands over to the next member.
    ok ("sjasmplus", "STRUCT initialiser list across lines",
        " STRUCT Hitbox\nx0 BYTE 0\nx1 BYTE 0\ny0 BYTE 1\ny1 BYTE 0\n ENDS\n\
         a Hitbox { $11, $22, ; x0, x1\n $33, $44 } ; y0, y1\n Hitbox {\n $55, ; one\n\n ; comment\n $66,\n $77\n}\n\
         b Hitbox { $11\n $22 $33,\n $44 }\nc Hitbox { $11\n , $22 }\nd Hitbox { $11,\n$22, $33, $44 }\n nop\n"),
    ok ("sjasmplus", "STRUCT initialiser nested group for an embedded member",
        " STRUCT In\np BYTE 5\nq BYTE 6\n ENDS\n STRUCT Out\nh BYTE 1\ni In\nt BYTE 9\n ENDS\n\
         a Out { $11, {$22, $33}, $44 }\nb Out { $11, $22, $33, $44 }\nc Out { $11, {$22}, $44 }\n\
         d Out { $11, {}, $44 }\ne Out { $11, {,$33}, $44 }\nf Out { {$22}, $44 }\ng Out $11, {$22, $33}, $44\n\
         h Out {\n $11,\n {$22,\n $33}\n}\n ld hl,a.i\n ld hl,a.i.q\n"),
    // #225: nine core 6809 instructions the spec had no row for at all —
    // add/subtract-with-carry, bit test, two 16-bit compares and `cwai`. Every
    // mode each one takes, because a missing mnemonic is missing in all of
    // them, not only the immediate form the issue happened to tabulate.
    ok ("lwasm", "add and subtract with carry, every mode",
        "\tadca #$12\n\tadca <$34\n\tadca ,x\n\tadca >$5678\n\
         \tadcb #$12\n\tadcb <$34\n\tadcb ,x\n\tadcb >$5678\n\
         \tsbca #$12\n\tsbca <$34\n\tsbca ,x\n\tsbca >$5678\n\
         \tsbcb #$12\n\tsbcb <$34\n\tsbcb ,x\n\tsbcb >$5678\n"),
    ok ("lwasm", "bit test, every mode",
        "\tbita #$12\n\tbita <$34\n\tbita ,x\n\tbita >$5678\n\
         \tbitb #$12\n\tbitb <$34\n\tbitb ,x\n\tbitb >$5678\n"),
    // `cmpd` and `cmpy` are `$10`-prefixed, which the module doc already
    // claimed as uniform while the table carried neither.
    ok ("lwasm", "the two $10-prefixed compares, every mode",
        "\tcmpd #$1234\n\tcmpd <$34\n\tcmpd ,x\n\tcmpd >$5678\n\
         \tcmpy #$1234\n\tcmpy <$34\n\tcmpy ,x\n\tcmpy >$5678\n"),
    ok ("lwasm", "cwai is immediate only",
        "\tcwai #$af\n\tandcc #$fe\n\torcc #$01\n"),

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
    // The listing and debug knobs, and the two that say something: probed
    // inert, so the same source assembles to the same bytes either way.
    // The conditional heads beyond the comparisons: blank-argument, text
    // comparison, sign, and the two else-legs — one of which is not what its
    // name suggests.
    ok ("vasm", "ifb/ifnb, ifc/ifnc, ifmi/ifpl and the else legs",
        "\tsection code,code\n\
         \tifb\n\tdc.b $11\n\telse\n\tdc.b $22\n\tendif\n\
         \tifb x\n\tdc.b $33\n\telse\n\tdc.b $44\n\tendif\n\
         \tifc \"a\",\"a\"\n\tdc.b $55\n\tendif\n\
         \tifnc \"a\",\"b\"\n\tdc.b $66\n\tendif\n\
         \tifmi -1\n\tdc.b $77\n\tendif\n\
         \tifpl 0\n\tdc.b $88\n\tendif\n\
         \tifeq 1\n\tdc.b $CC\n\telseif 0\n\tdc.b $DD\n\tendif\n"),
    ok ("vasm", "ifb answers a macro argument that was not given",
        "m\tmacro\n\tifb \\1\n\tdc.b $11\n\telse\n\tdc.b $22\n\tendif\n\tendm\n\
         \tsection code,code\n\tm\n\tm 5\n"),
    // The offset counters. `rs` and `so` turn out to be one counter under two
    // spellings, and only `fo` is separate — which the names do not suggest.
    ok ("vasm", "rs allocates names, and the counter symbols read it",
        "\tsection code,code\na\trs.b 1\nb\trs.w 1\nc\trs.l 1\n\tdc.b a,b,c,__RS\n\
         \trsset 8\nd\trs.b 1\n\tdc.b d\n\trsreset\ne\trs.b 1\n\trseven\n\
         f\trs.b 1\n\tdc.b e,f\n\tk\trs 1\nl\trs 1\n\tdc.b k,l\n"),
    ok ("vasm", "so is the same counter as rs, under another name",
        "\tsection code,code\n\tsetso 9\n\tdc.b __RS,__SO\n\
         \trsset 6\n\tdc.b __SO\n\tclrso\na\trs.b 1\n\tdc.b a\n\
         b\tso.b 2\nc\trs.b 1\n\tdc.b b,c\n"),
    ok ("vasm", "fo is its own counter and runs the other way",
        "\tsection code,code\n\tsetfo 8\nh\tfo.b 1\ni\tfo.w 1\n\tdc.b h,i,__FO\n\
         \tdc.b __RS\n\tclrfo\nj\tfo.b 1\n\tdc.b j\n"),
    ok ("vasm", "the listing and debug words emit nothing",
        "\tsection code,code\n\tlist\n\tnolist\n\tllen 80\n\tplen 60\n\tpage\n\
         \tnopage\n\tspc 2\n\tttl \"t\"\n\tsymdebug\n\tdsource \"x.s\"\n\tmsource\n\
         \tvdebug\n\tshowoffset\n\tdc.b $11\n\tdc.w $2233\n"),
    ok ("vasm", "printt and printv say something and emit nothing",
        "\tsection code,code\n\tprintt \"hello\"\n\tprintv 42\n\tdc.b $11\n"),
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
    // Expression functions: the three byte extractions, which pick from a
    // 24-bit value the way the `<`/`>`/`^` prefixes do.
    // `<` is the low-byte prefix *and* less-than, told apart by position.
    ok ("ca65-816", "comparisons answer 1",
        " .byte 2=2,2=3,2<>3,2<3,2>3,2<=3,2>=3\n"),
    ok ("ca65-816", "a prefix < and an infix < in one list",
        " .byte <$1234,2<3,>$1234\n"),
    ok ("ca65-816", ".lobyte / .hibyte / .bankbyte",
        "V = $123456\n lda #.lobyte(V)\n lda #.hibyte(V)\n lda #.bankbyte(V)\n"),
    ok ("ca65-816", "a function takes an expression",
        " lda #.lobyte($1234+1)\n ldx #.hibyte($12ff+1)\n"),
    ok ("ca65-816", ".loword / .hiword",
        "V = $123456\n .word .loword(V)\n .word .hiword(V)\n .word .loword($1234)\n"),
    // The case `.hiword` exists for: bits 16-31 of a 32-bit constant. A tracked
    // divergence until the engine stopped capping every dialect's `equ` at a
    // 65816 long address (#228).
    ok ("ca65-816", ".hiword over a 32-bit constant",
        "V = $12345678\n .word .loword(V)\n .word .hiword(V)\n .word .loword($1234)\n"),
    // Two-argument functions: the comma survives the operand split because it
    // is paren-aware, so `.word .max($100, $200)` stays one value.
    // String arguments: consumed at parse time, yielding a number, so an
    // expression still evaluates to an integer.
    ok ("ca65-816", ".strlen",           " lda #.strlen(\"hello\")\n .byte .strlen(\"\")\n"),
    ok ("ca65-816", ".strat picks a character", " lda #.strat(\"abc\", 1)\n"),
    // The text layer: string functions folded before the parse. `.string`
    // stringifies its argument's *token* rather than its value, and `.ident`
    // builds a name that resolves like any other — a forward reference too.
    ok ("ca65-816", ".concat joins, and one argument is a whole call",
        " .byte .concat(\"ab\",\"cd\")\n .byte .concat(\"a\")\n"),
    ok ("ca65-816", ".string takes the token, not the value",
        "N = 7\n .byte .string(N)\n .byte .string(42)\nlbl: .byte .string(lbl)\n"),
    ok ("ca65-816", ".ident builds a name, forward references included",
        "foo = 5\n .byte .ident(\"foo\")\n .byte .ident(.concat(\"b\",\"ar\"))\nbar = 9\n"),
    // `.paramcount` counts the **call site**, not the declared parameters, and
    // `.definedmacro` is answered where it is written — a definition below the
    // line does not count, and the name is case-sensitive.
    ok ("ca65-816", ".paramcount counts the call site",
        ".macro m p1, p2\n .byte .paramcount\n.endmacro\n m 1, 2\n m 1\n m\n\
         .macro n p1, p2, p3\n .byte .paramcount\n.endmacro\n n 1, , 3\n\
         .macro i\n .byte .paramcount\n.endmacro\n\
         .macro o p1\n i\n.endmacro\n o 1\n"),
    ok ("ca65-816", ".definedmacro is answered where it is written",
        ".byte .definedmacro(d)\n.macro d\n.endmacro\n\
         .byte .definedmacro(d), .definedmacro(n), .definedmacro(D)\n\
         .macro n\n .byte .definedmacro(d)\n.endmacro\n n\n"),
    // `.const` asks whether an expression is constant *here*, so a constant
    // defined below the line is not one — which is what a pass walking in
    // source order sees anyway. `.ismnem` follows the CPU: `bra` and `phb` are
    // 65816 additions a plain 6502 does not know.
    ok ("ca65-816", ".const answers over what is known here",
        "N = 5\nL: .byte .const(5), .const(N), .const(N*2)\n\
         .byte .const(L), .const(*)\n .byte .const(M)\nM = 5\n"),
    ok ("ca65-816", ".ismnem follows the CPU",
        " .byte .ismnem(lda), .ismnemonic(lda), .ismnem(zzz), .ismnem(LDA)\n\
         .byte .ismnem(phb), .ismnem(bra), .ismnem(nop)\n"),
    // The token-list half. A token list is unevaluated source, so these answer
    // over what is *written*: `.match` asks what each token is and `.xmatch`
    // asks what it says. `a`, `x` and `y` are register tokens and `s` is not,
    // each dot-keyword and punctuation mark is its own kind, and a character
    // literal is not a number.
    ok ("ca65-816", ".tcount and .blank count what is written",
        " .byte .tcount({1, 2, 3}), .tcount({}), .tcount({abc}), .tcount({\"a,b\"})\n\
         .byte .tcount({#$12}), .tcount({pa::v}), .tcount({.byte 1})\n\
         .byte .tcount({++}), .tcount({<<}), .tcount({:+})\n\
         .byte .blank({}), .blank({x})\n"),
    ok ("ca65-816", ".match compares what a token is",
        " .byte .match({1},{2}), .match({\"a\"},{\"b\"}), .match({abc},{abd})\n\
         .byte .match({a},{b}), .match({x},{y}), .match({a},{A})\n\
         .byte .match({s},{q})\n\
         .byte .match({.byte},{.word}), .match({+},{-}), .match({'a'},{1})\n\
         .byte .match({a},{a b}), .match({},{})\n"),
    ok ("ca65-816", ".xmatch compares what a token says as well",
        " .byte .xmatch({1},{2}), .xmatch({1},{$1}), .xmatch({abc},{abd})\n\
         .byte .xmatch({lda},{LDA}), .xmatch({\"a\"},{\"b\"}), .xmatch({.byte},{.byte})\n"),
    ok ("ca65-816", ".left, .mid and .right splice tokens back as source",
        " .byte .left(1, {1, 2, 3})\n .byte .left(3, {1, 2, 3})\n\
         .byte .right(1, {1, 2, 3}), .mid(2, 1, {1, 2, 3})\n\
         .byte .left(9, {1}), .right(9, {1}), .mid(0, 9, {1})\n"),
    ok ("ca65-816", "a string function inside a list is one token by the time it counts",
        " .byte .tcount({.concat(\"a\",\"b\")}), .tcount({.strlen(\"ab\")})\n"),
    // `.sprintf` is C's shape and not all of C's rules. `%x` is signed and
    // `%X` is not, so the two disagree about a negative value; `%s` and `%c`
    // pad on the right by default and on the left with `-`, the reverse of
    // every other type; and `#` on `%x` shows its prefix even for zero.
    ok ("ca65-816", ".sprintf's eight conversions",
        " .byte .sprintf(\"%d|%i|%u|%c|%s\", 5, -5, -5, 65, \"ab\")\n\
         .byte .sprintf(\"%x|%X|%o|100%%\", -255, -255, -9)\n"),
    ok ("ca65-816", ".sprintf's flags and widths",
        " .byte .sprintf(\"%+d|% d|%6d|%-6d|%06d|%+06d\", 5, 5, -5, -5, -5, 0)\n\
         .byte .sprintf(\"%#x|%#X|%#o|%#x|%#o\", 255, 255, 9, 0, 0)\n\
         .byte .sprintf(\"%#08x|%+#08x|%#08X|%+#08X\", -255, 255, -255, 255)\n"),
    ok ("ca65-816", ".sprintf pads a string and a char the other way round",
        " .byte .sprintf(\"[%6s][%-6s][%06s][%.2s]\", \"ab\", \"ab\", \"ab\", \"abcd\")\n\
         .byte .sprintf(\"[%6c][%-6c]\", 65, 65)\n"),
    ok ("ca65-816", ".sprintf's precision is a minimum digit count",
        " .byte .sprintf(\"%.3d|%.4x|%.4X|%.4o|%.4u\", -5, -255, -255, -9, 5)\n\
         .byte .sprintf(\"[%.0d][%.0x][%08.3d][%#.4o]\", 0, 0, 5, 9)\n"),
    ok ("ca65-816", ".sprintf reads a constant defined above it",
        "N = 5\n .byte .sprintf(\"n=%d|%d\", N, N*2+1)\n\
         .byte .sprintf(\"%s\", .sprintf(\"%d\", 7))\n"),
    ok ("ca65-816", "a text fold feeds a numeric function",
        " .byte .strlen(.concat(\"ab\",\"cd\"))\n"),
    ok ("ca65-816", ".max / .min",
        " lda #.max(3, 7)\n lda #.min(3, 7)\n"),
    ok ("ca65-816", "two-argument functions take expressions",
        " lda #.max(1+1, 2*2)\n lda #.min(.max(1,5), 9)\n"),
    ok ("ca65-816", "a call survives a data list split",
        " .word .max($100, $200), $3\n"),
    // The plural extractors are directives, not functions: byte 0, byte 1 and
    // byte 2 of every value in the list, and `.faraddr` all three at once.
    ok ("ca65-816", ".lobytes and .hibytes over a list",
        " .lobytes $1234, $5678\n .hibytes $1234, $5678\n"),
    ok ("ca65-816", ".bankbytes takes byte 2, above a 24-bit value too",
        " .bankbytes $123456, $12345678\n"),
    ok ("ca65-816", ".faraddr is three bytes, little-endian",
        " .faraddr $112233, $445566\n"),
    ok ("ca65-816", "an extractor takes an expression",
        " .lobytes $12ff+1\n .hibytes $12ff+1\n"),
    ok ("ca65-816", "an extractor takes a label defined below it",
        " .lobytes L\n .faraddr L\nL: .byte $AA\n"),
    ok ("ca65-816", "an extractor masks a negative value",
        " .lobytes 0-1\n .hibytes 0-1\n .bankbytes 0-1\n"),
    ok ("ca65-816", "functions nest",
        " lda #.lobyte(.hibyte($123456))\n"),
    ok ("ca65-816", ".res reserves",     " lda #1\n .res 3\n lda #2\n"),
    ok ("ca65-816", ".res takes a fill", " lda #1\n .res 3,$ff\n"),
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
    // `align` states the boundary itself, not a power of two — `align 3` after
    // a byte really does put the next item at offset 3 — and takes an optional
    // fill byte. Already-aligned pads nothing.
    // lwasm has `<>` but neither `=` nor `<=`/`>=`.
    ok ("lwasm", "comparisons answer 1",  " fcb 2<>3,2<3,2>3\n"),
    ok ("lwasm", "align pads to the boundary",
        " fcb 1\n align 4\n fcb 2\n"),
    ok ("lwasm", "align to a non-power-of-two boundary",
        " fcb 1\n align 3\n fcb 2\n"),
    ok ("lwasm", "align takes a fill byte",
        " fcb 1\n align 4,$ff\n fcb 2\n"),
    ok ("lwasm", "align on the boundary pads nothing",
        " fcb 1,2,3,4\n align 4\n fcb 9\n"),
    ok ("lwasm", "setstr feeds generated source",
        " setstr BODY=\" fcb $2a\\n fcb $2b\"\n includestr \"%(BODY)\"\n"),
    ok ("lwasm", "ifstr exact prefix suffix and case forms",
        " ifstr ieq,\"AbcXYZ\",\"abcxyz\"\n fcb 1\n endc\n\
         ifstr peq,3,\"AbcXYZ\",\"Abc123\"\n fcb 2\n endc\n\
         ifstr iseq,3,\"AbcXYZ\",\"000xyz\"\n fcb 3\n endc\n"),

    // ---- rgbasm / SM83 ------------------------------------------------------
    // Conditionals: `ELIF` rather than `ELSEIF`, and `ENDC` is the **only**
    // closer — rgbds answers `ENDIF` with `Undefined macro`.
    // A banked section is addressed at $4000 whichever bank holds it and lands
    // at `bank * $4000` in the ROM, which an image position equal to an address
    // cannot express. `rgblink` pads to `(highest bank + 1) * $4000`.
    // `BANK("name")` reaches forward — the section it names may be below it.
    ok ("rgbasm-banked", "BANK reaches forward to its section",
        "SECTION \"f\",ROM0[$0]\n db BANK(\"paged\")\n db BANK(\"f\")\n\
         SECTION \"paged\",ROMX,BANK[2]\n db $22\n"),
    ok ("rgbasm-banked", "a banked section is placed by bank",
        "SECTION \"f\",ROM0[$0]\n db $00\nSECTION \"p\",ROMX,BANK[2]\nhere:\n db $22\n dw here\n"),
    ok ("rgbasm-banked", "a higher bank sizes the ROM",
        "SECTION \"f\",ROM0[$0]\n db 1\nSECTION \"z\",ROMX,BANK[5]\n db 2\n"),
    ok ("rgbasm-banked", "two banks, and labels in each",
        "SECTION \"f\",ROM0[$0]\n dw a\n dw b\n\
         SECTION \"p\",ROMX,BANK[1]\na: db $11\n\
         SECTION \"q\",ROMX,BANK[3]\nb: db $33\n"),
    // Sections are placed by address, not by the order they were written.
    // Lowering a section to an `org` could only ever move forward, so this
    // failed with `cannot move origin backwards` until the engine grew a
    // section model.
    ok ("rgbasm", "comparisons are == and !=",
        "SECTION \"s\",ROM0[0]\n db 2==2,2==3,2!=3,2<3,2>3,2<=3,2>=3\n"),
    ok ("rgbasm", "EXPORT emits nothing and asks nothing",
        "SECTION \"s\",ROM0[0]\nEXPORT foo\nfoo: db 1,2\nEXPORT nope\n"),
    ok ("rgbasm", "PRINT and PRINTLN emit nothing",
        "SECTION \"s\",ROM0[0]\n PRINT \"n=\", 5\n PRINTLN \"x\"\n db 1,2\n"),
    ok ("rgbasm", "ASSERT and STATIC_ASSERT pass silently",
        "SECTION \"s\",ROM0[0]\n ASSERT 1\n STATIC_ASSERT 1, \"fine\"\n db 1\n"),
    ok ("rgbasm", "an assertion reaches forward",
        "SECTION \"s\",ROM0[0]\n ASSERT fin-beg\nbeg: db 1,2\nfin:\n"),
    ok ("rgbasm", "sections out of address order",
        "SECTION \"c\",ROM0[$0]\n db $cc\nSECTION \"b\",ROM0[$20]\n db $bb\n\
         SECTION \"a\",ROM0[$10]\n db $aa\n"),
    ok ("rgbasm", "a section gap is filled",
        "SECTION \"a\",ROM0[$0]\n db 1\nSECTION \"b\",ROM0[$4]\n db 2\n"),
    ok ("rgbasm", "labels take their own section's base",
        "SECTION \"c\",ROM0[$0]\n dw far\nSECTION \"f\",ROM0[$30]\nfar: db $99\n"),
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
    // `elif` is the real else-if, and it tests its argument for **truth** the
    // way `if` does — not against zero the way `ifeq` does. `elseif` is the
    // false friend beside it: vasm reads whatever follows and ignores it.
    ok ("vasm", "elif takes the first true leg",
        "\tifeq 1\n\tnop\n\telif 1\n\trts\n\telse\n\tillegal\n\tendif\n"),
    ok ("vasm", "elif chains, and falls through to else",
        "\tifeq 1\n\tnop\n\telif 0\n\tillegal\n\telif 0\n\tnop\n\telse\n\trts\n\tendif\n"),
    ok ("vasm", "elif with no else",
        "\tifeq 1\n\tnop\n\telif 1\n\trts\n\tendif\n"),
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
    ok ("vasm", "unsigned condition aliases",
        " bhs.b next\n nop\nnext: blo.w done\n dbhs d0,next\n dblo.w d1,done\n shs d2\n slo.b d3\ndone: nop\n"),
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
        binaries: &[],
        note: "label on an include-defined macro call survives for forward low/high-byte references",
        files: &[
            (
                "main.a",
                "* = $1234\n        lda #<handler\n        ldx #>handler\n        !src \"defs.a\"\nhandler +body\n",
            ),
            ("defs.a", "!macro body {\n        nop\n}\n"),
        ],
    },
    MultiProbe {
        dialect: "acme",
        binaries: &[],
        note: "standalone anonymous label survives capture inside an include-defined macro",
        files: &[
            ("main.a", "* = $1234\n!src \"defs.a\"\n+spin 2\n"),
            (
                "defs.a",
                "!macro spin .count {\n!for .i, .count {\n\t-\n        nop\n        bne -\n}\n}\n",
            ),
        ],
    },
    MultiProbe {
        dialect: "acme",
        binaries: &[],
        note: "repeated nested include-defined calls freshly scope indented dotted labels",
        files: &[
            ("main.a", "* = $1234\n!src \"defs.a\"\n+outer 2\n"),
            (
                "defs.a",
                "!macro inner {\n\t.again\n        nop\n        bne .again\n}\n!macro outer .count {\n!for .i, .count {\n        +inner\n}\n}\n",
            ),
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
        dialect: "ca65-nes",
        binaries: &[],
        note: "ca65 data and structure directives with no probe until now: \
               .dbyt is big-endian where .word is little, .dword is 32-bit \
               little, .asciiz appends the terminator, and .if/.repeat/.macro \
               (with .local) fold before layout",
        files: &[(
            "main.s",
            ".segment \"HEADER\"\n .byte \"NES\", $1A, 2, 1\n\
             .segment \"CODE\"\n\
             reset: .word $1234\n .dbyt $1234\n .dword $12345678\n\
             .asciiz \"hi\"\n\
             .if 1\n lda #1\n .else\n lda #2\n .endif\n\
             .if 0\n lda #3\n .endif\n\
             .repeat 3\n nop\n .endrepeat\n\
             .macro twice arg\n .local spin\nspin: lda #arg\n bne spin\n .endmacro\n\
             twice 7\n twice 8\n\
             .segment \"VECTORS\"\n .word 0, reset, 0\n",
        )],
    },
    MultiProbe {
        dialect: "ca65-huc6280",
        binaries: &[],
        note: "HuC6280 leg: the same data and macro surface as the NES leg, \
               reached through the flat ca65 path — .org, .word, .dbyt, \
               .dword, .asciiz, .res and a .macro",
        files: &[(
            "main.s",
            " .org $2000\n lda #$11\n .word $1234\n .dbyt $1234\n\
             .dword $12345678\n .asciiz \"hi\"\n .res 3\n .res 2,$ff\n\
             .macro ld2 a1, a2\n lda #a1\n ldx #a2\n .endmacro\n ld2 1,2\n",
        )],
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
        note: "`.defined` is positional: 0 above the definition and 1 below, \
               in a condition and in an operand alike, and `.def` is the same \
               function by a shorter name",
        files: &[(
            "main.s",
            ".segment \"HEADER\"\n .byte \"NES\", $1A, 2, 1\n\
             .segment \"CODE\"\n\
             reset: lda #.defined(LATER)\n\
             .if .defined(LATER)\n lda #$11\n .else\n lda #$22\n .endif\n\
             LATER = 7\n\
             lda #.defined(LATER)\n lda #.def(LATER)\n\
             .if .defined(LATER)\n lda #$33\n .else\n lda #$44\n .endif\n\
             .if .defined(NEVER)\n lda #$55\n .endif\n\
             .segment \"VECTORS\"\n .word 0, reset, 0\n",
        )],
    },
    MultiProbe {
        dialect: "ca65-nes",
        binaries: &[],
        note: "ca65 segment shorthands and the .pushseg/.popseg stack: \
               `.code`/`.zeropage`/`.bss` place as their spelled-out \
               segments, and a push/pop pair restores the segment the \
               reservation interrupted (U5)",
        files: &[(
            "main.s",
            ".segment \"HEADER\"\n .byte \"NES\", $1A, 2, 1\n\
             .zeropage\npos: .res 1\n\
             .bss\nbuf: .res 4\n\
             .code\nreset: lda pos\n\
             .pushseg\n.zeropage\ntmp: .res 1\n.popseg\n\
             sta buf\n stx tmp\n\
             .segment \"VECTORS\"\n .word 0, reset, 0\n",
        )],
    },
    MultiProbe {
        dialect: "ca65-nes",
        binaries: &[],
        note: "the visibility words emit nothing in the shapes that assemble: \
               `.export`/`.exportzp` over a name defined below, `.import` over \
               a name nothing defines or reads, `.global` either way, and \
               `.export k := 7` defining as it exports",
        files: &[(
            "main.s",
            ".segment \"HEADER\"\n .byte \"NES\", $1A, 2, 1\n\
             .segment \"ZEROPAGE\"\npos: .res 1\n\
             .segment \"CODE\"\n\
             .export reset, later\n .exportzp pos\n\
             .import nothing_reads_this\n .global reset\n .global never_defined\n\
             .export k := 7\n .autoimport +\n\
             reset: lda #k\nlater: sta pos\n\
             .segment \"VECTORS\"\n .word 0, reset, 0\n",
        )],
    },
    MultiProbe {
        dialect: "ca65-nes",
        binaries: &[],
        note: "`.out`, `.warning` and a passing or warning `.assert` emit \
               nothing: the bytes are the same with them present as without, \
               and both still assemble (an address-dependent assertion ca65 \
               defers to ld65, which we answer at the fused link)",
        files: &[(
            "main.s",
            ".segment \"HEADER\"\n .byte \"NES\", $1A, 2, 1\n\
             .segment \"CODE\"\n\
             .out \"building\"\n\
             .warning \"soft\", 5\n\
             reset: nop\n\
             .assert 1, error, \"never fires\"\n\
             .assert later = $8001, error, \"moved\"\n\
             .assert 0, warning, \"soft, assembles anyway\"\n\
             later: lda #$07\n\
             .segment \"VECTORS\"\n .word 0, reset, 0\n",
        )],
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
        dialect: "8080",
        binaries: &[],
        note: "asl 8080 `dw` — 16-bit little-endian data beside the `db` the \
               binclude probes already cover (`ds` stays out of probe scope: \
               asl leaves a gap p2bin fills with $FF where we emit zeros)",
        files: &[(
            "main.asm",
            "\tcpu 8080\n\torg 0\n\tdb 0aah\n\tdw 1234h\n\tdw 1,2\n\tdb 0bbh\n",
        )],
    },
    // #227: asl's CP-1600 `byte` takes a 16-bit operand and emits its two
    // bytes low-first, one byte per decle — so `byte x'1234'` is `0034 0012`
    // and `byte 1` is `0001 0000`. Probed by reading the listing's location
    // counter, which advances 2 per operand whatever the value.
    //
    // `binclude` differs, and that difference is asl's rather than ours: it
    // advances one decle per byte while p2bin lays the bytes down packed. Both
    // are probed here together so neither can be "fixed" into the other.
    MultiProbe {
        dialect: "cp1610",
        binaries: &[],
        note: "cp1610 byte accounting: each operand is two decles, low byte \
               then high, a `word` beside it is one decle, and a string or \
               character operand anywhere in the list silences the whole \
               statement — the numeric operands beside it do not survive \
               (#227)",
        files: &[(
            "main.asm",
            "\tcpu CP-1600\n\torg x'0000'\n\tbyte 1,2,3\n\tword x'1234'\n\
             \tbyte 4\n\tbyte x'1234'\n\tbyte x'FF'\n\tbyte x'100'\n\tbyte 0\n\
             \tbyte \"AB\"\n\tbyte 'A'\n\tbyte \"AB\",1\n\tbyte 1,'A'\n\
             \tword x'FFFF'\n",
        )],
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
                "\tcpu CP-1600\n\torg x'0000'\n\tinclude \"defs.inc\"\n\tmvii K,r0\n\tbinclude \"odd3.bin\"\nafter:\tword after\n\tbinclude \"data.bin\",2,3\n",
            ),
            // `x'AAAA'` rather than `0AAAAH`: strict CP-1600 asl takes its own
            // hex form and nothing else, and the `relaxed on` that used to buy
            // the Intel spelling is refused now (#214).
            ("defs.inc", "K equ 5\n\tword x'AAAA'\n"),
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
    MultiProbe {
        dialect: "vasm-exe",
        binaries: &[],
        note: "vasm `align` states an exponent, not a boundary: `align 2` is \
               a four-byte boundary, zero-filled, and folds an equ constant. \
               Already-aligned pads nothing, and the even-pad an instruction \
               takes anyway is subsumed by a wider one",
        files: &[(
            "main.s",
            "N equ 2\n\tsection code,code\n\tdc.b 1\n\talign 0\n\tdc.b 2\n\
             \talign 1\n\tdc.b 3\n\talign N\n\tdc.b 4\n\talign 3\n\
             \tmoveq #1,d0\n\tdc.b 5,6,7,8\n\talign 2\n\tdc.b 9\n",
        )],
    },
    MultiProbe {
        dialect: "ca65-nes",
        binaries: &[],
        note: "the nine conditional heads beyond .if/.ifdef/.ifndef: \
               .ifblank asks whether anything follows it, .ifconst whether \
               the expression is a constant (a label is not, a same-segment \
               difference above the line is, a forward one is not), and the \
               .ifpNN family which CPU this is",
        files: &[(
            "main.s",
            ".segment \"HEADER\"\n .byte \"NES\", $1A, 2, 1\n\
             .code\nV = 5\n\
             .ifblank\n .byte $11\n .else\n .byte $22\n .endif\n\
             .ifblank x\n .byte $11\n .else\n .byte $22\n .endif\n\
             .ifnblank\n .byte $11\n .else\n .byte $22\n .endif\n\
             .ifconst 1+1\n .byte $33\n .endif\n\
             .ifconst V*2\n .byte $34\n .endif\n\
             .ifnconst 1\n .byte $ee\n .else\n .byte $35\n .endif\n\
             LA:\nLB:\n .ifconst LB-LA\n .byte $36\n .else\n .byte $ee\n .endif\n\
             .ifconst LA\n .byte $ee\n .else\n .byte $37\n .endif\n\
             .ifconst LA*2\n .byte $ee\n .else\n .byte $38\n .endif\n\
             .ifconst LC-LA\n .byte $ee\n .else\n .byte $39\n .endif\n\
             LC:\n .ifp02\n .byte $44\n .endif\n\
             .ifp816\n .byte $ee\n .else\n .byte $45\n .endif\n\
             .ifpc02\n .byte $ee\n .else\n .byte $46\n .endif\n\
             .ifpsc02\n .byte $ee\n .else\n .byte $47\n .endif\n\
             .ifp4510\n .byte $ee\n .else\n .byte $48\n .endif\n\
             .segment \"VECTORS\"\n .word 0, LA, 0\n",
        )],
    },
    // `.proc`/`.scope`: what NES source is actually written in. The rules here
    // are ca65 V2.18's own, probed: `.proc name` defines `name` and opens a
    // scope, `.scope` opens one and defines nothing, a name inside is reached
    // from outside as `scope::name`, lookup walks outward, `::name` is the top
    // level from anywhere, and cheap locals belong to the scope they are in.
    MultiProbe {
        dialect: "ca65-nes",
        binaries: &[],
        note: "scopes: .proc defines its name and opens one, .scope only opens \
               one, names inside are reached as scope::name, lookup walks \
               outward, :: is the top level, and @cheap locals do not collide",
        files: &[(
            "main.s",
            ".segment \"HEADER\"\n .byte \"NES\", $1A, 2, 1\n\
             .code\n\
             v = $11\n\
             .proc one\n\
             v = $22\n\
             inner: nop\n\
             @l: nop\n\
             bne @l\n\
             .byte v, ::v\n\
             .endproc\n\
             .proc two\n\
             inner: nop\n\
             @l: nop\n\
             bne @l\n\
             .byte one::v\n\
             .endproc\n\
             .scope outer\n\
             w = $33\n\
             .scope nested\n\
             .byte w\n\
             deep: nop\n\
             .endscope\n\
             .word nested::deep\n\
             .endscope\n\
             after: nop\n\
             .word one, two, one::inner, two::inner, outer::nested::deep, after\n\
             .byte outer::w\n\
             .segment \"VECTORS\"\n .word 0, one, 0\n",
        )],
    },
    // The words that address the listing and the object file rather than the
    // program: present or absent, the bytes are the same.
    MultiProbe {
        dialect: "ca65-nes",
        binaries: &[],
        note: "listing and object-metadata words emit nothing: .list, \
               .listbytes, .pagelen(gth), .debuginfo, .dbg, .fileopt/.fopt",
        files: &[(
            "main.s",
            ".segment \"HEADER\"\n .byte \"NES\", $1A, 2, 1\n\
             .code\n\
             .debuginfo on\n .fileopt comment, \"probe\"\n .fopt author, \"probe\"\n\
             .list on\n .byte $11\n .list off\n .byte $22\n\
             .listbytes 4\n .pagelen 60\n .pagelength 60\n .dbg line\n\
             .byte $33\n\
             .segment \"VECTORS\"\n .word 0, 0, 0\n",
        )],
    },
    // The record types: a compile-time layout, its names scoped under the
    // record, and `.tag` allocating one instance's worth of space.
    MultiProbe {
        dialect: "ca65-nes",
        binaries: &[],
        note: "records: .struct field offsets and .sizeof, .res and counted \
               fields, .union laying every member at zero, .enum counting on \
               from an explicit value, a record nested in a record allocating \
               its size, and .tag reserving an instance",
        files: &[(
            "main.s",
            ".segment \"HEADER\"\n .byte \"NES\", $1A, 2, 1\n\
             .code\n\
             .struct Point\npx .byte\npy .byte\npw .word\n.endstruct\n\
             .struct Buf\nblen .byte 3\nbdata .res 8\nbw .word 2\n.endstruct\n\
             .union U\nua .byte\nub .word\n.endunion\n\
             .enum Colours\nred\ngreen\nblue = 10\nwhite\n.endenum\n\
             .struct Outer\noa .byte\n.struct Inner\nia .word\n.endstruct\n\
             ob .byte\noi .tag Point\n.endstruct\n\
             .byte Point::px, Point::py, Point::pw, .sizeof(Point)\n\
             .byte Buf::blen, Buf::bdata, Buf::bw, .sizeof(Buf)\n\
             .byte U::ua, U::ub, .sizeof(U)\n\
             .byte Colours::red, Colours::green, Colours::blue, Colours::white\n\
             .byte Outer::oa, Outer::ob, Outer::oi, .sizeof(Outer)\n\
             .byte .sizeof(Outer::Inner), .sizeof(Outer::oi)\n\
             p: .tag Point\n\
             q: .tag Point\n\
             .word p, q, p + Point::pw\n\
             .segment \"VECTORS\"\n .word 0, 0, 0\n",
        )],
    },
    // The processor words a NES header opens with. Naming the processor this
    // leg already assembles changes nothing at all.
    MultiProbe {
        dialect: "ca65-nes",
        binaries: &[],
        note: ".setcpu \"6502\", .p02, .smart and the .pushcpu/.popcpu pair \
               emit nothing when the processor named is the one being assembled",
        files: &[(
            "main.s",
            ".segment \"HEADER\"\n .byte \"NES\", $1A, 2, 1\n\
             .setcpu \"6502\"\n .smart on\n\
             .code\n .p02\n lda #1\n .pushcpu\n .setcpu \"6502\"\n ldx #2\n\
             .popcpu\n ldy #3\n .smart off\n rts\n\
             .segment \"VECTORS\"\n .word 0, 0, 0\n",
        )],
    },
    // `.ref`/`.referenced` and the two conditional heads over them: has this
    // name been *used* above the line, which is a different question from
    // `.defined` and answered from a different record.
    MultiProbe {
        dialect: "ca65-nes",
        binaries: &[],
        note: ".ref/.referenced answer whether a name was used above the line, \
               .ifref/.ifnref branch on it, and a use inside a dead branch \
               does not count",
        files: &[(
            "main.s",
            ".segment \"HEADER\"\n .byte \"NES\", $1A, 2, 1\n\
             .code\n\
             L: nop\n .byte .ref(L)\n .word L\n .byte .ref(L), .referenced(L)\n\
             .ifref L\n .byte $11\n .else\n .byte $22\n .endif\n\
             .ifnref ZZ\n .byte $33\n .else\n .byte $44\n .endif\n\
             .if 0\n .word M\n .endif\n\
             M: nop\n .byte .ref(M)\n\
             .word M\n .byte .ref(M)\n\
             .segment \"VECTORS\"\n .word 0, 0, 0\n",
        )],
    },
    // `.org` names an address without moving the output, `.reloc` gives it
    // back, and `.end` stops the assembler reading.
    MultiProbe {
        dialect: "ca65-nes",
        binaries: &[],
        note: ".org moves the address and not the bytes, .reloc returns to the \
               segment's own addressing, and .end stops the read",
        files: &[(
            "main.s",
            ".segment \"HEADER\"\n .byte \"NES\", $1A, 2, 1\n\
             .code\n .byte $11\n\
             .org $2000\n\
             L: .byte $22\n .word L\n .word *\n\
             .org $3000\n N: .byte $44\n .word N\n\
             .reloc\n\
             M: .byte $33\n .word M\n\
             .segment \"VECTORS\"\n .word 0, 0, 0\n\
             .end\n\
             this line is never read and would not parse\n",
        )],
    },
    MultiProbe {
        dialect: "ca65-nes",
        binaries: &[],
        note: "ca65 `.align` pads within the segment, not to an absolute \
               address — `.align 3` in CODE (based at $8000, not a multiple \
               of 3) lands at segment offset 3. The boundary need not be a \
               power of two, takes an optional fill, and a label on the \
               directive line binds *before* the pad",
        files: &[(
            "main.s",
            ".segment \"HEADER\"\n .byte \"NES\", $1A, 2, 1\n\
             .code\nreset: .byte 1\n\
             here: .align 3\n .byte 2\n\
             .align 4, $ff\n .byte 3\n\
             .byte 4,5,6,7\n .align 4\n .byte 8\n\
             .segment \"VECTORS\"\n .word here, reset, 0\n",
        )],
    },
    MultiProbe {
        dialect: "vasm-exe",
        binaries: &[],
        note: "the longword a code hunk is padded out to: two bytes short \
               takes a NOP, one or three short take zeros (there is no room \
               for a whole instruction word), and the choice is made from the \
               length as it stands",
        files: &[(
            "main.s",
            "\tsection code,code\n\tdc.b $aa\n\
             \tsection two,code\n\tdc.b $aa,$bb\n\
             \tsection three,code\n\tdc.b $aa,$bb,$cc\n\
             \tsection four,code\n\tdc.b $aa,$bb,$cc,$dd\n\
             \tsection d,data\n\tdc.b $aa,$bb\n",
        )],
    },
    MultiProbe {
        dialect: "vasm-exe",
        binaries: &[],
        note: "vasm section shorthands: each opens a section of its own kind, \
               and the `_c`/`_f` suffix carries the memory flag into the \
               hunk header",
        files: &[(
            "main.s",
            "\tcode\n\tmoveq #1,d0\n\tdata\n\tdc.w $1234\n\tbss\n\tds.w 4\n\
             \tcode_c\n\tmoveq #2,d0\n\tdata_f\n\tdc.w $5678\n",
        )],
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
        if tool(p.dialect) == "rgbasm" && !have("rgblink") {
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
                     \x20   OAM:    start = $0200,  size = $100,   type = rw, file = \"\";\n\
                     \x20   RAM:    start = $0300,  size = $500,   type = rw, file = \"\";\n\
                     \x20   HEADER: start = $0,     size = $10,    type = ro, file = %O, fill = yes;\n\
                     \x20   PRG:    start = $8000,  size = $8000,  type = ro, file = %O, fill = yes;\n\
                     \x20   CHR:    start = $0,     size = $2000,  type = ro, file = %O, fill = yes;\n\
                     }\n\
                     SEGMENTS {\n\
                     \x20   ZEROPAGE: load = ZP,     type = zp;\n\
                     \x20   OAM:      load = OAM,    type = bss;\n\
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
