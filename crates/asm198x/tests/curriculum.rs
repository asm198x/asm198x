//! Byte-identity validation against the real toolchains and the Code198x corpus.
//!
//! This is the guarantee the whole project rests on: our output is byte-for-byte
//! what the reference assembler produces on the actual curriculum. It is
//! `#[ignore]`d because it needs external programs (`acme`, `ca65`, `ld65`,
//! `pasmo`/PasmoNext, `sjasmplus`) and the sibling `Code198x` checkout, so it
//! can't run in the default unit-test pass or in a vanilla CI box.
//!
//! Run it with:
//!
//! ```text
//! cargo test --test curriculum -- --ignored --nocapture
//! ```
//!
//! Each section degrades gracefully: a missing tool or a missing corpus path is
//! reported and skipped rather than failed, so the test is safe to run anywhere.
//! Whatever *is* present is checked exactly.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Locate the `Code198x` checkout, a sibling of the `Asm198x` container two
/// levels above this crate's workspace.
fn code198x() -> Option<PathBuf> {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../../Code198x");
    let p = p.canonicalize().ok()?;
    p.is_dir().then_some(p)
}

/// Whether a reference tool is on `PATH` (it exists if it runs at all).
fn have(bin: &str) -> bool {
    Command::new(bin).output().is_ok()
}

/// The buildable `.asm` files: one per `unit-*` directory, directly inside it
/// (not in a `snippets/` subdirectory).
fn main_asms(project: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(units) = fs::read_dir(project) else {
        return out;
    };
    for unit in units.flatten() {
        let dir = unit.path();
        let is_unit = dir.is_dir()
            && dir
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("unit-"));
        if !is_unit {
            continue;
        }
        if let Ok(files) = fs::read_dir(&dir) {
            for f in files.flatten() {
                let fp = f.path();
                if fp.extension().and_then(|e| e.to_str()) == Some("asm") {
                    out.push(fp);
                }
            }
        }
    }
    out.sort();
    out
}

fn label(file: &Path) -> String {
    let unit = file
        .parent()
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str())
        .unwrap_or("?");
    let proj = file
        .parent()
        .and_then(|p| p.parent())
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str())
        .unwrap_or("?");
    format!("{proj}/{unit}")
}

/// Run a reference command in `tmp` and return the bytes it wrote to `out`.
fn ref_bytes(tmp: &Path, out: &Path, mut build: Command) -> Option<Vec<u8>> {
    let status = build.current_dir(tmp).output().ok()?;
    if !status.status.success() {
        return None;
    }
    fs::read(out).ok()
}

/// Strip the optional `HUNK_SYMBOL` (debug) blocks from a vasm hunk executable,
/// leaving the loadable image AmigaDOS actually consumes — the parity target,
/// since we omit the symbol table (its order is vasm's internal hash order).
/// Returns `None` on any malformed/unrecognised hunk.
fn strip_hunk_symbols(data: &[u8]) -> Option<Vec<u8>> {
    let u32at = |o: usize| -> Option<u32> {
        data.get(o..o + 4)
            .map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    };
    let mut out = Vec::new();
    let mut i = 0usize;
    while i < data.len() {
        match u32at(i)? {
            0x3f3 => {
                // HUNK_HEADER: resident-library names (until 0), then hunk sizes.
                let start = i;
                i += 4;
                while u32at(i)? != 0 {
                    i += 4 + u32at(i)? as usize * 4;
                }
                i += 4; // the terminating zero
                i += 4; // table size
                let first = u32at(i)?;
                i += 4;
                let last = u32at(i)?;
                i += 4 + (last - first + 1) as usize * 4;
                out.extend_from_slice(data.get(start..i)?);
            }
            tag @ (0x3e9 | 0x3ea) => {
                // HUNK_CODE / HUNK_DATA: a longword count, then that many words.
                let _ = tag;
                let sz = u32at(i + 4)? as usize;
                let end = i + 8 + sz * 4;
                out.extend_from_slice(data.get(i..end)?);
                i = end;
            }
            0x3eb => {
                // HUNK_BSS: a size, no data.
                out.extend_from_slice(data.get(i..i + 8)?);
                i += 8;
            }
            0x3ec => {
                // HUNK_RELOC32: [count, target hunk, offsets…] blocks until 0.
                let start = i;
                i += 4;
                while u32at(i)? != 0 {
                    i += 8 + u32at(i)? as usize * 4;
                }
                i += 4;
                out.extend_from_slice(data.get(start..i)?);
            }
            0x3f0 => {
                // HUNK_SYMBOL: [name-longs, name, value] until 0 — dropped.
                i += 4;
                while u32at(i)? != 0 {
                    i += 4 + u32at(i)? as usize * 4 + 4;
                }
                i += 4;
            }
            0x3f2 => {
                out.extend_from_slice(data.get(i..i + 4)?);
                i += 4;
            }
            _ => return None,
        }
    }
    Some(out)
}

#[test]
#[ignore = "needs the reference assemblers and the Code198x checkout; run with --ignored"]
fn curriculum_is_byte_identical() {
    let Some(root) = code198x() else {
        eprintln!("SKIP: Code198x checkout not found next to Asm198x");
        return;
    };
    let tmp = std::env::temp_dir().join("asm198x-curriculum");
    fs::create_dir_all(&tmp).expect("create temp dir");

    let mut fails: Vec<String> = Vec::new();
    let mut checked = 0usize;

    // --- 6502 / acme (C64) -------------------------------------------------
    if have("acme") {
        let c64 = root.join("code-samples/commodore-64/assembly");
        let files: Vec<_> = main_asms(&c64.join("starfield"))
            .into_iter()
            .chain(main_asms(&c64.join("sid-symphony")))
            .collect();
        for file in &files {
            let src = fs::read_to_string(file).expect("read source");
            let ours = asm198x::assemble_acme(&src).expect("acme assemble");
            let prg = tmp.join("ref.prg");
            let mut cmd = Command::new("acme");
            cmd.args(["-f", "cbm", "-o"]).arg(&prg).arg(file);
            match ref_bytes(&tmp, &prg, cmd) {
                // acme's `cbm` output is a 2-byte load address then the data.
                Some(prg_bytes) if prg_bytes.len() >= 2 => {
                    checked += 1;
                    if ours.bytes != prg_bytes[2..] {
                        fails.push(format!("acme assemble: {}", label(file)));
                    }
                }
                _ => fails.push(format!("acme reference failed: {}", label(file))),
            }
            // Disassembler round-trip: assemble -> disassemble -> reassemble.
            let listing = asm198x::listing_6502(&ours.bytes, ours.origin);
            let round = asm198x::assemble_acme(&listing).expect("reassemble").bytes;
            if round != ours.bytes {
                fails.push(format!("6502 disasm round-trip: {}", label(file)));
            }
        }
    } else {
        eprintln!("SKIP: `acme` not on PATH");
    }

    // --- 6502 / ca65 + ld65 (NES) -----------------------------------------
    if have("ca65") && have("ld65") {
        let nes = root.join("code-samples/nintendo-entertainment-system/assembly");
        let files: Vec<_> = main_asms(&nes.join("dash"))
            .into_iter()
            .chain(main_asms(&nes.join("neon-nexus")))
            .collect();
        for file in &files {
            let src = fs::read_to_string(file).expect("read source");
            let ours = asm198x::assemble_ca65(&src).expect("ca65 assemble");
            let cfg = file.parent().expect("parent").join("nes.cfg");
            let obj = tmp.join("ref.o");
            let rom = tmp.join("ref.nes");
            let mut ca = Command::new("ca65");
            ca.arg(file).arg("-o").arg(&obj);
            let assembled = ca
                .current_dir(&tmp)
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false);
            let mut ld = Command::new("ld65");
            ld.arg("-C").arg(&cfg).arg(&obj).arg("-o").arg(&rom);
            match (assembled, ref_bytes(&tmp, &rom, ld)) {
                (true, Some(reference)) => {
                    checked += 1;
                    if ours != reference {
                        fails.push(format!("ca65 link: {}", label(file)));
                    }
                }
                _ => fails.push(format!("ca65 reference failed: {}", label(file))),
            }
        }
    } else {
        eprintln!("SKIP: `ca65`/`ld65` not on PATH");
    }

    // --- Z80 / PasmoNext + sjasmplus (Spectrum, Gloaming) ------------------
    let gloaming = root.join("code-samples/sinclair-zx-spectrum/assembly/gloaming");
    let z80_files = main_asms(&gloaming);
    if have("pasmo") {
        for file in &z80_files {
            let src = fs::read_to_string(file).expect("read source");
            let ours = asm198x::assemble_pasmonext(&src).expect("pasmonext assemble");
            let bin = tmp.join("ref.bin");
            let mut cmd = Command::new("pasmo");
            cmd.arg(file).arg(&bin);
            match ref_bytes(&tmp, &bin, cmd) {
                Some(reference) => {
                    checked += 1;
                    if ours.bytes != reference {
                        fails.push(format!("pasmonext assemble: {}", label(file)));
                    }
                }
                None => fails.push(format!("pasmo reference failed: {}", label(file))),
            }
            // Z80 disassembler round-trip.
            let listing = asm198x::listing_z80(&ours.bytes, ours.origin, true);
            let round = asm198x::assemble_pasmonext(&listing)
                .expect("reassemble")
                .bytes;
            if round != ours.bytes {
                fails.push(format!("Z80 disasm round-trip: {}", label(file)));
            }
        }
    } else {
        eprintln!("SKIP: `pasmo` (PasmoNext) not on PATH");
    }
    if have("sjasmplus") {
        for file in &z80_files {
            let src = fs::read_to_string(file).expect("read source");
            let ours = asm198x::assemble_sjasmplus(&src).expect("sjasmplus assemble");
            let bin = tmp.join("ref-sj.bin");
            let mut cmd = Command::new("sjasmplus");
            cmd.arg(format!("--raw={}", bin.display())).arg(file);
            match ref_bytes(&tmp, &bin, cmd) {
                Some(reference) => {
                    checked += 1;
                    if ours.bytes != reference {
                        fails.push(format!("sjasmplus assemble: {}", label(file)));
                    }
                }
                None => fails.push(format!("sjasmplus reference failed: {}", label(file))),
            }
        }
    } else {
        eprintln!("SKIP: `sjasmplus` not on PATH");
    }

    // --- 68000 / vasm (Amiga, signal + exodus) ----------------------------
    if have("vasmm68k_mot") {
        let amiga = root.join("code-samples/commodore-amiga/assembly");
        let files: Vec<_> = main_asms(&amiga.join("signal"))
            .into_iter()
            .chain(main_asms(&amiga.join("exodus")))
            .collect();
        for file in &files {
            let src = fs::read_to_string(file).expect("read source");
            // Hunk executable (every unit): loadable-image parity — compare with
            // vasm's debug symbol table stripped, which we deliberately omit.
            let ours_exe = asm198x::assemble_vasm_exe(&src).expect("vasm exe assemble");
            let exe = tmp.join("ref.exe");
            let mut cmd = Command::new("vasmm68k_mot");
            cmd.args(["-Fhunkexe", "-kick1hunks", "-quiet", "-o"])
                .arg(&exe)
                .arg(file);
            match ref_bytes(&tmp, &exe, cmd).and_then(|b| strip_hunk_symbols(&b)) {
                Some(reference) => {
                    checked += 1;
                    if ours_exe != reference {
                        fails.push(format!("vasm hunkexe: {}", label(file)));
                    }
                }
                None => fails.push(format!("vasm hunkexe reference failed: {}", label(file))),
            }
            // Flat -Fbin: only the single-section units build as a flat binary
            // (our `assemble_vasm` errors on multi-section, as `-Fbin` does).
            if let Ok(ours_bin) = asm198x::assemble_vasm(&src) {
                let bin = tmp.join("ref.bin");
                let mut cmd = Command::new("vasmm68k_mot");
                cmd.args(["-Fbin", "-quiet", "-o"]).arg(&bin).arg(file);
                if let Some(reference) = ref_bytes(&tmp, &bin, cmd) {
                    checked += 1;
                    if ours_bin != reference {
                        fails.push(format!("vasm -Fbin: {}", label(file)));
                    }
                }
            }
        }
    } else {
        eprintln!("SKIP: `vasmm68k_mot` not on PATH");
    }

    // --- 6809 / lwasm (no curriculum yet — representative programs) --------
    // There is no 6809 curriculum, so instead of a corpus we validate a set of
    // representative programs against `lwasm --6809 --raw` directly. They cover
    // the modes the dialect implements: inherent, immediate (8/16-bit), direct,
    // extended, the `<`/`>` forces, the ALU/RMW ops, jmp/jsr, and the short,
    // long, and conditional-long branches, plus org/fcb/fdb/rmb.
    if have("lwasm") {
        for (name, src) in LWASM_PROGRAMS {
            let ours = asm198x::assemble_lwasm(src).expect("lwasm assemble");
            let asm = tmp.join("ref6809.asm");
            let bin = tmp.join("ref6809.bin");
            fs::write(&asm, src).expect("write 6809 source");
            let mut cmd = Command::new("lwasm");
            cmd.args(["--6809", "--raw", "-o"]).arg(&bin).arg(&asm);
            match ref_bytes(&tmp, &bin, cmd) {
                Some(reference) => {
                    checked += 1;
                    if ours.bytes != reference {
                        fails.push(format!("lwasm assemble: {name}"));
                    }
                }
                None => fails.push(format!("lwasm reference failed: {name}")),
            }
            // Disassembler round-trip: assemble -> disassemble -> reassemble.
            let listing = asm198x::listing_6809(&ours.bytes, ours.origin);
            let round = asm198x::assemble_lwasm(&listing).expect("reassemble").bytes;
            if round != ours.bytes {
                fails.push(format!("6809 disasm round-trip: {name}"));
            }
        }
    } else {
        eprintln!("SKIP: `lwasm` not on PATH");
    }

    // --- 65816 / ca65 (no curriculum yet — representative programs) --------
    // The 65816 is a target extension of the 6502 (ca65 syntax). With no SNES
    // curriculum, we validate representative native-mode programs against
    // `ca65 --cpu 65816` linked flat (a minimal config placing CODE at $0000,
    // matching our default origin). Covers the m/x immediate width, all the new
    // addressing modes, long calls/jumps, and the new instructions.
    if have("ca65") && have("ld65") {
        let cfg = tmp.join("flat816.cfg");
        fs::write(
            &cfg,
            "MEMORY { MAIN: start=$0000, size=$10000, fill=no, file=%O; }\n\
             SEGMENTS { CODE: load=MAIN, type=ro; }\n",
        )
        .expect("write 65816 flat config");
        for (name, src) in CA65_816_PROGRAMS {
            let ours = asm198x::assemble_ca65_816(src).expect("ca65-816 assemble");
            let asm = tmp.join("ref816.s");
            let obj = tmp.join("ref816.o");
            let bin = tmp.join("ref816.bin");
            fs::write(&asm, src).expect("write 65816 source");
            let assembled = Command::new("ca65")
                .args(["--cpu", "65816"])
                .arg(&asm)
                .arg("-o")
                .arg(&obj)
                .current_dir(&tmp)
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false);
            let mut ld = Command::new("ld65");
            ld.arg("-C").arg(&cfg).arg(&obj).arg("-o").arg(&bin);
            match (assembled, ref_bytes(&tmp, &bin, ld)) {
                (true, Some(reference)) => {
                    checked += 1;
                    if ours.bytes != reference {
                        fails.push(format!("ca65-816 assemble: {name}"));
                    }
                }
                _ => fails.push(format!("ca65-816 reference failed: {name}")),
            }
            // Disassembler round-trip: assemble -> disassemble -> reassemble. The
            // disassembler tracks m/x width via rep/sep and emits the matching
            // .aXX/.iXX directives, so width-switching code reproduces exactly.
            let listing = asm198x::listing_65816(&ours.bytes, ours.origin);
            let round = asm198x::assemble_ca65_816(&listing).expect("reassemble").bytes;
            if round != ours.bytes {
                fails.push(format!("65816 disasm round-trip: {name}"));
            }
        }
    } else {
        eprintln!("SKIP: `ca65`/`ld65` not on PATH (65816)");
    }

    eprintln!("checked {checked} byte-identity comparisons across the curriculum");
    assert!(
        fails.is_empty(),
        "{} mismatch(es):\n  {}",
        fails.len(),
        fails.join("\n  ")
    );
    assert!(
        checked > 0,
        "no comparisons ran — no tools or corpus present?"
    );
}

/// Representative 6809 programs validated byte-for-byte against `lwasm`. Stand in
/// for a curriculum corpus the 6809 does not yet have.
const LWASM_PROGRAMS: &[(&str, &str)] = &[
    (
        "modes",
        "        org     $1000\n\
         start   lda     #$42\n\
         \x20       ldb     #$10\n\
         \x20       ldx     #$1234\n\
         \x20       ldd     #$beef\n\
         \x20       lda     $20\n\
         \x20       sta     $21\n\
         \x20       lda     $1234\n\
         \x20       sta     $4000\n\
         \x20       adda    #$01\n\
         \x20       suba    $30\n\
         \x20       addd    #$0100\n\
         \x20       anda    #$0f\n\
         \x20       ora     #$80\n\
         \x20       cmpa    #$ff\n\
         \x20       cmpx    #$2000\n\
         \x20       clr     $40\n\
         \x20       inc     $41\n\
         \x20       com     $42\n\
         \x20       lda     <$20\n\
         \x20       lda     >$20\n\
         \x20       rts\n",
    ),
    (
        "branches",
        "        org     $2000\n\
         loop    deca\n\
         \x20       bne     loop\n\
         \x20       beq     done\n\
         \x20       bra     loop\n\
         \x20       lbra    loop\n\
         \x20       lbeq    done\n\
         done    rts\n",
    ),
    (
        "data-and-ldy",
        "        org     $3000\n\
         \x20       ldy     #$1234\n\
         \x20       lds     #$8000\n\
         \x20       ldu     #table\n\
         \x20       jsr     sub\n\
         \x20       jmp     start\n\
         start   nop\n\
         sub     clra\n\
         \x20       clrb\n\
         \x20       rts\n\
         table   fcb     $01,$02,$03\n\
         \x20       fdb     $1234,$5678\n\
         \x20       rmb     4\n",
    ),
    (
        "indexed",
        "        org     $1000\n\
         \x20       lda     ,x\n\
         \x20       lda     0,x\n\
         \x20       lda     5,x\n\
         \x20       lda     -16,x\n\
         \x20       lda     16,x\n\
         \x20       lda     $1234,x\n\
         \x20       lda     ,y\n\
         \x20       ldx     2,s\n\
         \x20       lda     ,x+\n\
         \x20       lda     ,x++\n\
         \x20       lda     ,-x\n\
         \x20       lda     ,--x\n\
         \x20       lda     a,x\n\
         \x20       lda     b,x\n\
         \x20       lda     d,x\n\
         \x20       lda     [,x]\n\
         \x20       lda     [5,x]\n\
         \x20       lda     [$1234,x]\n\
         \x20       lda     [,x++]\n\
         \x20       lda     [d,y]\n\
         \x20       lda     [$2000]\n\
         \x20       leax    msg,pcr\n\
         \x20       sta     ,x++\n\
         \x20       stb     [d,y]\n\
         \x20       ldd     ,y\n\
         msg     fcb     $41\n",
    ),
    (
        "register-ops",
        "        org     $1000\n\
         \x20       tfr     a,b\n\
         \x20       tfr     x,y\n\
         \x20       tfr     d,u\n\
         \x20       tfr     pc,s\n\
         \x20       exg     a,b\n\
         \x20       exg     x,d\n\
         \x20       pshs    a\n\
         \x20       pshs    a,b,x,y\n\
         \x20       pshs    cc,a,b,dp,x,y,u,pc\n\
         \x20       puls    pc\n\
         \x20       puls    a,b,x\n\
         \x20       pshu    a,b,s\n\
         \x20       puls    x,y,d\n\
         \x20       leay    -2,y\n\
         \x20       rts\n",
    ),
    (
        "strings",
        "        org     $1000\n\
         hello   fcc     \"Hello, world\"\n\
         \x20       fcc     /slashes/\n\
         \x20       fdb     hello\n\
         \x20       rts\n",
    ),
];

/// Representative 65816 native-mode programs validated byte-for-byte against
/// `ca65 --cpu 65816` (linked flat). Stand in for a SNES curriculum the 65816
/// does not yet have. Each is valid ca65 source (`.setcpu`/`.segment` are
/// no-ops in our flat model).
const CA65_816_PROGRAMS: &[(&str, &str)] = &[
    (
        "mx-and-modes",
        ".setcpu \"65816\"\n\
         .segment \"CODE\"\n\
         \x20       clc\n\
         \x20       xce\n\
         \x20       rep     #$30\n\
         \x20       .a16\n\
         \x20       .i16\n\
         \x20       lda     #$1234\n\
         \x20       ldx     #$5678\n\
         \x20       sep     #$20\n\
         \x20       .a8\n\
         \x20       lda     #$42\n\
         \x20       lda     $12\n\
         \x20       lda     $1234\n\
         \x20       lda     $123456\n\
         \x20       lda     $123456,x\n\
         \x20       lda     [$12]\n\
         \x20       lda     [$12],y\n\
         \x20       lda     3,s\n\
         \x20       lda     (3,s),y\n\
         \x20       lda     ($12)\n\
         \x20       sta     f:$7e0000\n\
         \x20       rts\n",
    ),
    (
        "standalone-and-stz",
        ".setcpu \"65816\"\n\
         .segment \"CODE\"\n\
         \x20       xba\n\
         \x20       tcd\n\
         \x20       tdc\n\
         \x20       tcs\n\
         \x20       txy\n\
         \x20       phb\n\
         \x20       plb\n\
         \x20       phd\n\
         \x20       phk\n\
         \x20       phx\n\
         \x20       ply\n\
         \x20       wai\n\
         \x20       inc     a\n\
         \x20       dec     a\n\
         \x20       stz     $12\n\
         \x20       stz     $12,x\n\
         \x20       stz     $1234\n\
         \x20       stz     $1234,x\n\
         \x20       trb     $12\n\
         \x20       tsb     $1234\n\
         \x20       rts\n",
    ),
    (
        "block-moves-and-cop",
        ".setcpu \"65816\"\n\
         .segment \"CODE\"\n\
         src     = $7e0000\n\
         dst     = $7f0000\n\
         ptr     = $7e1234\n\
         \x20       mvn     #$7e,#$7f\n\
         \x20       mvp     #$00,#$01\n\
         \x20       mvn     src,dst\n\
         \x20       cop     $12\n\
         \x20       wdm     $34\n\
         \x20       .a8\n\
         \x20       lda     #^ptr\n\
         \x20       lda     #>ptr\n\
         \x20       lda     #<ptr\n\
         \x20       lda     f:ptr\n\
         \x20       rts\n",
    ),
    (
        "jumps-labels-branches",
        ".setcpu \"65816\"\n\
         .segment \"CODE\"\n\
         buffer  = $2000\n\
         start:\n\
         \x20       ldx     #$00\n\
         loop:\n\
         \x20       lda     buffer,x\n\
         \x20       sta     dest,x\n\
         \x20       inx\n\
         \x20       bne     loop\n\
         \x20       per     start\n\
         \x20       brl     start\n\
         \x20       jml     $018000\n\
         \x20       jsl     sub\n\
         \x20       jmp     ($1234,x)\n\
         \x20       jmp     [$1234]\n\
         \x20       pea     $abcd\n\
         \x20       pei     ($12)\n\
         dest:   .res    16\n\
         sub:    rts\n",
    ),
];
