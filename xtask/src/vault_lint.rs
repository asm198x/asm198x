//! Advisory checks for illustrative assembly in the Code198x Vault.

use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::path::Path;
use std::process::ExitCode;

use asm198x::{AsmError, AssemblyResult};

type Assembler = fn(&str) -> Result<AssemblyResult, AsmError>;

#[derive(Clone, Copy)]
struct Dialect {
    name: &'static str,
    assemble: Assembler,
    prologue: &'static str,
}

const ACME: Dialect = Dialect {
    name: "acme",
    assemble: asm198x::assemble_acme,
    prologue: "* = $0000",
};
const CA65: Dialect = Dialect {
    name: "ca65",
    assemble: asm198x::assemble_ca65,
    prologue: ".org $0000",
};
const SJASMPLUS: Dialect = Dialect {
    name: "sjasmplus",
    assemble: asm198x::assemble_sjasmplus,
    prologue: " org $0000",
};
const PASMO: Dialect = Dialect {
    name: "pasmo",
    assemble: asm198x::assemble_pasmo,
    prologue: " org $0000",
};
const VASM: Dialect = Dialect {
    name: "vasm",
    assemble: asm198x::assemble_vasm,
    prologue: "",
};
const RGBASM: Dialect = Dialect {
    name: "rgbasm",
    assemble: asm198x::assemble_rgbasm,
    prologue: "SECTION \"Vault fragment\", ROM0[$0000]",
};
const LWASM: Dialect = Dialect {
    name: "lwasm",
    assemble: asm198x::assemble_lwasm,
    prologue: " org $0000",
};
const CA65_816: Dialect = Dialect {
    name: "65816",
    assemble: asm198x::assemble_ca65_816,
    prologue: ".org $0000",
};
const DIALECTS: &[Dialect] = &[ACME, CA65, SJASMPLUS, PASMO, VASM, RGBASM, LWASM, CA65_816];

fn platform_dialects(platform: &str) -> &'static [Dialect] {
    match platform {
        "commodore-64" | "commodore-vic-20" | "bbc-micro" | "atari-2600" | "atari-8-bit"
        | "apple-ii" => &[ACME, CA65],
        "nintendo-entertainment-system" => &[CA65],
        "super-nintendo" => &[CA65_816],
        "sinclair-zx-spectrum" | "zx-spectrum" | "amstrad-cpc" | "msx" | "sega-master-system" => {
            &[SJASMPLUS, PASMO]
        }
        "commodore-amiga" | "amiga" | "atari-st" | "sega-mega-drive" | "neo-geo" => &[VASM],
        "game-boy" => &[RGBASM],
        "dragon-64" | "tandy-coco" => &[LWASM],
        _ => &[],
    }
}

/// `None` means a non-assembly fence. An empty list means an assembly fence
/// that this sweep cannot cover.
fn candidates(language: &str, platforms: &[&str]) -> Option<Vec<Dialect>> {
    let named: &[Dialect] = match language {
        "asm" | "assembly" => {
            let hints: BTreeSet<_> = platforms
                .iter()
                .flat_map(|p| platform_dialects(p).iter().map(|d| d.name))
                .collect();
            return Some(
                DIALECTS
                    .iter()
                    .copied()
                    .filter(|d| platforms.is_empty() || hints.contains(d.name))
                    .collect(),
            );
        }
        "6502" | "mos6502" => &[ACME, CA65],
        "z80" => &[SJASMPLUS, PASMO],
        "68000" | "m68k" | "mot" => &[VASM],
        "6809" => &[LWASM],
        "sm83" | "gb" | "gameboy" => &[RGBASM],
        "nasm" | "x86" | "x86asm" | "armasm" | "mips" => &[],
        name => return DIALECTS.iter().find(|d| d.name == name).map(|d| vec![*d]),
    };
    Some(named.to_vec())
}

/// Read the Vault's inline `platforms` list from the leading frontmatter.
fn platforms(text: &str) -> Vec<&str> {
    let mut lines = text.lines();
    if lines.next().map(str::trim) != Some("---") {
        return Vec::new();
    }
    let list = lines
        .take_while(|line| line.trim() != "---")
        .find_map(|line| line.strip_prefix("platforms:"))
        .map(str::trim)
        .and_then(|line| line.strip_prefix('['))
        .and_then(|line| line.strip_suffix(']'));
    list.into_iter()
        .flat_map(|items| items.split(','))
        .map(|item| item.trim().trim_matches(['\'', '"']))
        .filter(|item| !item.is_empty())
        .collect()
}

struct Fence<'a> {
    line: usize,
    language: &'a str,
    body: Vec<&'a str>,
}

fn delimiter(line: &str) -> Option<(u8, usize, &str)> {
    let line = line.trim_start();
    let mark = *line.as_bytes().first()?;
    if !matches!(mark, b'`' | b'~') {
        return None;
    }
    let width = line.bytes().take_while(|&byte| byte == mark).count();
    (width >= 3).then(|| (mark, width, line[width..].trim()))
}

fn fences(text: &str) -> Vec<Fence<'_>> {
    let mut lines = text.lines().enumerate().peekable();
    if lines.peek().is_some_and(|(_, line)| line.trim() == "---") {
        lines.next();
        for (_, line) in lines.by_ref() {
            if line.trim() == "---" {
                break;
            }
        }
    }
    let mut result = Vec::new();
    while let Some((at, opening)) = lines.next() {
        let Some((mark, width, info)) = delimiter(opening) else {
            continue;
        };
        let language = info.split_whitespace().next().unwrap_or_default();
        let mut body = Vec::new();
        for (_, line) in lines.by_ref() {
            if delimiter(line).is_some_and(|(end, length, tail)| {
                end == mark && length >= width && tail.is_empty()
            }) {
                break;
            }
            body.push(line);
        }
        result.push(Fence {
            line: at + 1,
            language,
            body,
        });
    }
    result
}

// Stub only the assembler's reported missing global names. Expressions,
// local scopes and operand spellings continue through the normal assembler.
fn missing_global(message: &str) -> Option<&str> {
    let name = message
        .strip_prefix("undefined symbol `")
        .and_then(|value| value.strip_suffix('`'))
        .or_else(|| {
            message
                .strip_prefix("Symbol `")?
                .strip_suffix("` is undefined")
        })
        .or_else(|| {
            message
                .strip_prefix("Symbol '")?
                .strip_suffix("' is undefined")
        })?;
    let (first, rest) = name.as_bytes().split_first()?;
    ((first.is_ascii_alphabetic() || *first == b'_')
        && rest.iter().all(|b| b.is_ascii_alphanumeric() || *b == b'_'))
    .then_some(name)
}

fn lint(dialect: Dialect, fence: &Fence<'_>) -> Result<usize, (usize, String)> {
    let mut stubbed = BTreeSet::new();
    loop {
        let mut source = format!("{}\n", dialect.prologue);
        for name in &stubbed {
            match dialect.name {
                "acme" | "ca65" | "65816" => writeln!(source, "{name} = 0"),
                "rgbasm" => writeln!(source, "DEF {name} EQU 0"),
                _ => writeln!(source, "{name} equ 0"),
            }
            .expect("writing to String cannot fail");
        }
        for line in &fence.body {
            // Keep placeholder lines in the line map, without asking the
            // assembler to interpret an ellipsis as source.
            if !matches!(line.trim(), "..." | "…" | "; ..." | "; …") {
                source.push_str(line);
            }
            source.push('\n');
        }
        match (dialect.assemble)(&source) {
            Ok(_) => return Ok(stubbed.len()),
            Err(error) => {
                if stubbed.len() < 32
                    && let Some(name) = missing_global(&error.message)
                    && stubbed.insert(name.to_owned())
                {
                    continue;
                }
                return Err((
                    fence.line + error.line.saturating_sub(stubbed.len() + 1),
                    error.message,
                ));
            }
        }
    }
}

pub fn run(args: &[String]) -> ExitCode {
    let [folder] = args else {
        eprintln!("usage: cargo xtask vault-lint <techniques-directory>");
        return ExitCode::FAILURE;
    };
    match sweep(Path::new(folder)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("xtask vault-lint: {error}");
            ExitCode::FAILURE
        }
    }
}

fn sweep(folder: &Path) -> Result<(), String> {
    let mut paths: Vec<_> = std::fs::read_dir(folder)
        .and_then(|files| files.map(|entry| entry.map(|item| item.path())).collect())
        .map_err(|error| format!("{}: {error}", folder.display()))?;
    paths.retain(|p| p.extension().is_some_and(|extension| extension == "mdx"));
    paths.sort();
    if paths.is_empty() {
        return Err(format!("{} has no MDX entries", folder.display()));
    }
    let (mut assembled, mut with_stubs, mut warned, mut skipped) = (0, 0, 0, 0);
    for path in &paths {
        let text = std::fs::read_to_string(path)
            .map_err(|error| format!("{}: {error}", path.display()))?;
        let hints = platforms(&text);
        for fence in fences(&text) {
            let Some(dialects) = candidates(fence.language, &hints) else {
                continue;
            };
            if dialects.is_empty() {
                println!(
                    "{}:{}: skipped: no supported dialect for {}",
                    path.display(),
                    fence.line,
                    fence.language
                );
                skipped += 1;
                continue;
            }
            let mut errors = Vec::new();
            for dialect in dialects {
                match lint(dialect, &fence) {
                    Ok(stubs) => {
                        if stubs == 0 {
                            assembled += 1;
                        } else {
                            with_stubs += 1;
                        }
                        errors.clear();
                        break;
                    }
                    Err((line, message)) => errors.push((dialect.name, line, message)),
                }
            }
            if !errors.is_empty() {
                warned += 1;
                for (dialect, line, message) in errors {
                    println!("{}:{line}: warning: {dialect}: {message}", path.display());
                }
            }
        }
    }
    println!(
        "{} entries: {assembled} assembled, {with_stubs} assembled with label stubs, {warned} with warnings, {skipped} skipped",
        paths.len()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fenced_examples_keep_source_lines_and_respect_longer_delimiters() {
        let text = "---\nplatforms: ['commodore-64', \"sinclair-zx-spectrum\"]\n---\n\
                    \n\x60\x60\x60\x60text\n\x60\x60\x60asm\nnop\n\x60\x60\x60\n\x60\x60\x60\x60\n\
                    \n~~~z80 title=demo\n ld a, 4\n~~~~\n";
        assert_eq!(platforms(text), ["commodore-64", "sinclair-zx-spectrum"]);
        let examples = fences(text);
        assert_eq!(examples.len(), 2);
        assert_eq!((examples[0].line, examples[0].language), (5, "text"));
        assert_eq!((examples[1].line, examples[1].language), (11, "z80"));
        assert_eq!(examples[1].body, [" ld a, 4"]);
    }

    #[test]
    fn selects_compatible_targets_and_marks_unsupported_fences() {
        let hinted = candidates("asm", &["nintendo-entertainment-system", "ibm-pc"])
            .expect("assembly fence");
        assert_eq!(hinted.iter().map(|d| d.name).collect::<Vec<_>>(), ["ca65"]);
        let explicit = candidates("z80", &["commodore-64"]).expect("explicit fence");
        assert_eq!(
            explicit.iter().map(|d| d.name).collect::<Vec<_>>(),
            ["sjasmplus", "pasmo"]
        );
        assert!(candidates("nasm", &[]).expect("assembly fence").is_empty());
        assert!(candidates("basic", &[]).is_none());
    }

    #[test]
    fn stubs_missing_globals_but_reports_invalid_instructions_at_the_authored_line() {
        let mut fence = Fence {
            line: 15,
            language: "6502",
            body: vec![" jsr external_handler", " ; ...", " ...", " rts"],
        };
        for dialect in [ACME, CA65] {
            assert_eq!(lint(dialect, &fence), Ok(1));
        }
        fence.body[3] = " jsr (target_addr)";
        for dialect in [ACME, CA65] {
            let (line, diagnostic) = lint(dialect, &fence).expect_err("no indirect JSR on 6502");
            assert_eq!(line, 19);
            assert!(
                diagnostic.to_ascii_lowercase().contains("jsr")
                    || diagnostic.contains("addressing mode"),
                "{diagnostic}"
            );
        }
    }

    #[test]
    fn stubs_z80_callees_and_keeps_local_names_unresolved() {
        let fence = Fence {
            line: 15,
            language: "z80",
            body: vec![" call external_handler", " ret"],
        };
        for dialect in [PASMO, SJASMPLUS] {
            assert_eq!(lint(dialect, &fence), Ok(1));
        }
        assert_eq!(missing_global("Symbol `@local` is undefined"), None);
        assert_eq!(missing_global("undefined symbol `scope::local`"), None);
    }

    #[test]
    fn assembler_wrappers_preserve_short_operands_and_vasm_flat_defaults() {
        let fence = Fence {
            line: 1,
            language: "6502",
            body: vec![" lda #provided_elsewhere"],
        };
        assert_eq!(lint(ACME, &fence), Ok(1));
        let fence = Fence {
            line: 1,
            language: "68000",
            body: vec![" moveq #provided_elsewhere,d0", " rts"],
        };
        assert_eq!(lint(VASM, &fence), Ok(1));
    }
}
