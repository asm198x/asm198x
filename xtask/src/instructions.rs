//! Per-CPU instruction reference pages, generated from the `isa` crate.
//!
//! Docs-site plan R1 and R5: the reference comes from the spec, so a spec
//! change regenerates the page and a new CPU documents itself the moment its
//! spec lands. Nothing here is hand-authored, and `cargo xtask docs --check`
//! fails if a committed page has fallen behind.
//!
//! # What is not here
//!
//! **Provenance links.** R1 also wants each page to link into the umbrella
//! `reference/` datasheet library. The spec has no field for it: ten of the
//! nineteen modules carry a `**Provenance.**` paragraph in their doc comment
//! and nine — including 6502, Z80 and 65816 — carry nothing. Generating a
//! citation would mean inventing one, which is worse than having none.
//!
//! **Six CPUs.** The 68000, 6809, TMS9900, PDP-11, CP1610 and Z8000 encode
//! with models a form table cannot describe, and are listed on the index page
//! rather than left quietly absent.
//!
//! Four of them export a `SET: InstructionSet` whose `instructions` is
//! **empty** — a placeholder beside the real, bespoke table. That is a trap
//! worth naming: reading the exports alone suggests they are on the standard
//! model, and generating from them produces a page confidently reporting zero
//! instructions. [`every_listed_cpu_has_instructions`] exists so that cannot
//! happen quietly again.

use std::fmt::Write as _;

/// One CPU whose spec can be rendered as a form table.
struct Cpu {
    /// URL and file stem.
    slug: &'static str,
    /// The spec module this comes from, named on the page so a reader who
    /// wants the encoding truth can go straight to it.
    module: &'static str,
    set: &'static isa::InstructionSet,
}

/// Every CPU the reference covers, in the order the book lists them: the
/// machines people arrive looking for, then the rest by lineage.
fn cpus() -> Vec<Cpu> {
    vec![
        Cpu {
            slug: "mos6502",
            module: "mos6502",
            set: &isa::mos6502::SET,
        },
        Cpu {
            slug: "mos65816",
            module: "mos65816",
            set: &isa::mos65816::SET,
        },
        Cpu {
            slug: "huc6280",
            module: "huc6280",
            set: &isa::huc6280::SET,
        },
        Cpu {
            slug: "z80",
            module: "z80",
            set: &isa::z80::SET,
        },
        Cpu {
            slug: "z80n",
            module: "z80",
            set: &isa::z80::NEXT,
        },
        Cpu {
            slug: "sm83",
            module: "sm83",
            set: &isa::sm83::SET,
        },
        Cpu {
            slug: "i8080",
            module: "i8080",
            set: &isa::i8080::SET,
        },
        Cpu {
            slug: "m6800",
            module: "m6800",
            set: &isa::m6800::SET,
        },
        Cpu {
            slug: "cdp1802",
            module: "cdp1802",
            set: &isa::cdp1802::SET,
        },
        Cpu {
            slug: "i8048",
            module: "i8048",
            set: &isa::i8048::SET,
        },
        Cpu {
            slug: "scmp",
            module: "scmp",
            set: &isa::scmp::SET,
        },
        Cpu {
            slug: "f8",
            module: "f8",
            set: &isa::f8::SET,
        },
        Cpu {
            slug: "s2650",
            module: "s2650",
            set: &isa::s2650::SET,
        },
        Cpu {
            slug: "tms7000",
            module: "tms7000",
            set: &isa::tms7000::SET,
        },
    ]
}

/// One CPU whose instructions are opcode *words* with operand fields inside
/// them, rather than opcode bytes followed by operand bytes.
///
/// The three share a model — `Insn { mnemonic, base, class, summary }` over a
/// per-CPU `Class` — so they share a renderer. What differs is the class set,
/// and each class states its own bit layout, so the layouts live beside the
/// spec instead of being restated here.
struct WordCpu {
    slug: &'static str,
    module: &'static str,
    name: &'static str,
    endianness: isa::Endianness,
    /// Flattened by the caller, because each CPU's `Class` is its own type.
    rows: Vec<WordRow>,
}

struct WordRow {
    mnemonic: &'static str,
    base: u16,
    class: &'static str,
    encoding: &'static str,
    describe: &'static str,
    summary: &'static str,
}

/// The word-oriented CPUs, flattened into one shape.
fn word_cpus() -> Vec<WordCpu> {
    vec![
        WordCpu {
            slug: "tms9900",
            module: "tms9900",
            name: "TI TMS9900",
            endianness: isa::Endianness::Big,
            rows: isa::tms9900::INSTRUCTIONS
                .iter()
                .map(|i| WordRow {
                    mnemonic: i.mnemonic,
                    base: i.base,
                    class: class_name(&i.class),
                    encoding: i.class.encoding(),
                    describe: i.class.describe(),
                    summary: i.summary,
                })
                .collect(),
        },
        WordCpu {
            slug: "pdp11",
            module: "pdp11",
            name: "DEC PDP-11",
            endianness: isa::Endianness::Little,
            rows: isa::pdp11::INSTRUCTIONS
                .iter()
                .map(|i| WordRow {
                    mnemonic: i.mnemonic,
                    base: i.base,
                    class: class_name(&i.class),
                    encoding: i.class.encoding(),
                    describe: i.class.describe(),
                    summary: i.summary,
                })
                .collect(),
        },
        WordCpu {
            slug: "cp1610",
            module: "cp1610",
            name: "GI CP1610",
            endianness: isa::Endianness::Big,
            rows: isa::cp1610::INSTRUCTIONS
                .iter()
                .map(|i| WordRow {
                    mnemonic: i.mnemonic,
                    base: i.base,
                    class: class_name(&i.class),
                    encoding: i.class.encoding(),
                    describe: i.class.describe(),
                    summary: i.summary,
                })
                .collect(),
        },
    ]
}

/// Escape a value for a markdown table cell.
///
/// A `|` inside a cell ends it, even within a code span — and the encoding
/// formulas are made of them (`base | src << 3 | dst`). Unescaped, such a row
/// silently becomes several mangled cells rather than failing, which is why
/// there is a test on the rendered shape and not just on the build succeeding.
fn cell(text: &str) -> String {
    text.replace('|', "\\|")
}

/// The variant's name, via `Debug` — the specs derive it, and a hand-written
/// name table here would be a second copy of the class list to keep in step.
fn class_name<C: std::fmt::Debug>(class: &C) -> &'static str {
    // Leaked once per row, of which there are a few hundred for the life of a
    // generator that runs and exits. The alternative is threading a lifetime
    // through the row type for no gain.
    Box::leak(format!("{class:?}").into_boxed_str())
}

fn render_word_cpu(cpu: &WordCpu) -> String {
    let mut out = format!("# {}\n\n", cpu.name);
    out.push_str(&generated_note(cpu.module));
    let _ = write!(
        out,
        "\n{} instructions, {}. Generated from \
         [`crates/isa/src/{}.rs`](https://github.com/asm198x/asm198x/blob/main/crates/isa/src/{}.rs).\n\n\
         This CPU encodes an instruction as an opcode **word** whose operand fields \
         are bits inside it, so there is no opcode-then-operands table to give. Each \
         instruction has a *base* — the opcode word with its operand fields zeroed — \
         and a *class* saying where those fields sit.\n",
        cpu.rows.len(),
        match cpu.endianness {
            isa::Endianness::Little => "words little-endian",
            isa::Endianness::Big => "words big-endian",
        },
        cpu.module,
        cpu.module,
    );

    // The legend first: a reader meets the class names in the table below, and
    // sending them elsewhere to decode a column is the sort of small rudeness
    // that makes a reference tiring to use.
    out.push_str("\n## Classes\n\n| Class | Encoding | Meaning |\n|---|---|---|\n");
    let mut seen: Vec<&str> = Vec::new();
    for row in &cpu.rows {
        if seen.contains(&row.class) {
            continue;
        }
        seen.push(row.class);
        let _ = writeln!(
            out,
            "| `{}` | `{}` | {} |",
            cell(row.class),
            cell(row.encoding),
            cell(row.describe)
        );
    }

    out.push_str("\n## Instructions\n\n| Mnemonic | Base | Class | Summary |\n|---|---|---|---|\n");
    for row in &cpu.rows {
        let _ = writeln!(
            out,
            "| {} | `{:04X}` | `{}` | {} |",
            cell(row.mnemonic),
            row.base,
            cell(row.class),
            cell(row.summary)
        );
    }
    out
}

/// The 68000, which has its own model again: a mnemonic and a summary, then
/// forms carrying a base opcode *word*, a size encoding, and operand slots.
///
/// Close to the byte-oriented model in shape, and nothing like it underneath —
/// an operand's bits live inside the opcode word, and how many extension words
/// follow depends on the effective address those bits select. So the table
/// gives the base word and what the operands are, not a byte count.
fn render_m68k() -> String {
    let mut out = String::from("# Motorola 68000\n\n");
    out.push_str(&generated_note("m68k"));

    let forms: usize = isa::m68k::SET
        .instructions
        .iter()
        .map(|i| i.forms.len())
        .sum();
    let _ = write!(
        out,
        "\n{} mnemonics, {} forms, words big-endian. Generated from \
         [`crates/isa/src/m68k.rs`](https://github.com/asm198x/asm198x/blob/main/crates/isa/src/m68k.rs).\n\n\
         The 68000 packs operand fields into the opcode word itself, and how many \
         extension words follow depends on the effective address those fields \
         select — so a form gives its **base** opcode word, the sizes it assembles \
         at, and its operands, rather than a fixed byte count.\n\n\
         This is the curriculum subset, and it grows: a mnemonic absent here is \
         absent from the assembler too.\n",
        isa::m68k::SET.instructions.len(),
        forms,
    );

    out.push_str("\n## Operands\n\n| Written | Meaning |\n|---|---|\n");
    let mut seen: Vec<&str> = Vec::new();
    for slot in isa::m68k::SET
        .instructions
        .iter()
        .flat_map(|i| i.forms)
        .flat_map(|f| f.operands)
    {
        if seen.contains(&slot.symbol()) {
            continue;
        }
        seen.push(slot.symbol());
        let _ = writeln!(
            out,
            "| `{}` | {} |",
            cell(slot.symbol()),
            cell(slot.describe())
        );
    }

    for instruction in isa::m68k::SET.instructions {
        let _ = write!(
            out,
            "\n## {}\n\n{}\n\n",
            instruction.mnemonic, instruction.summary
        );
        out.push_str("| Sizes | Base word | Operands |\n|---|---|---|\n");
        for form in instruction.forms {
            let operands = if form.operands.is_empty() {
                "—".to_string()
            } else {
                form.operands
                    .iter()
                    .map(|o| o.symbol())
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            let _ = writeln!(
                out,
                "| {} | `{:04X}` | {} |",
                cell(form.size.sizes()),
                form.base,
                cell(&operands)
            );
        }
    }
    out
}

/// A page the generator owns entirely: path under the book's `src`, and its
/// content.
pub struct Page {
    pub path: String,
    pub body: String,
}

/// Render every page the instruction reference owns — one per CPU, plus the
/// index that lists them.
pub fn pages() -> Vec<Page> {
    let cpus = cpus();
    let mut out: Vec<Page> = cpus
        .iter()
        .map(|cpu| Page {
            path: format!("instructions/{}.md", cpu.slug),
            body: render_cpu(cpu),
        })
        .collect();
    out.extend(word_cpus().iter().map(|cpu| Page {
        path: format!("instructions/{}.md", cpu.slug),
        body: render_word_cpu(cpu),
    }));
    out.push(Page {
        path: "instructions/m68k.md".to_string(),
        body: render_m68k(),
    });
    out.push(Page {
        path: "instructions.md".to_string(),
        body: render_index(&cpus, &word_cpus()),
    });
    out
}

/// The `SUMMARY.md` lines listing the instruction reference, so a new CPU
/// appears in the sidebar without anyone remembering to add it.
pub fn summary_lines() -> String {
    let mut out = String::from("- [Instruction reference](instructions.md)\n");
    for cpu in cpus() {
        let _ = writeln!(out, "  - [{}](instructions/{}.md)", cpu.set.cpu, cpu.slug);
    }
    for cpu in word_cpus() {
        let _ = writeln!(out, "  - [{}](instructions/{}.md)", cpu.name, cpu.slug);
    }
    out.push_str("  - [Motorola 68000](instructions/m68k.md)\n");
    out
}

/// The note every generated page opens with, so nobody edits one by hand and
/// loses the edit on the next run.
fn generated_note(module: &str) -> String {
    format!(
        "<!-- Generated by `cargo xtask docs` from `crates/isa/src/{module}.rs`. \
         Edits here are overwritten; change the spec instead. -->\n"
    )
}

fn render_index(cpus: &[Cpu], word: &[WordCpu]) -> String {
    let mut out = String::from("# Instruction reference\n\n");
    out.push_str(&generated_note("*"));
    out.push_str(
        "\nOne page per CPU, generated from the instruction-set specification the\n\
         assembler encodes with. A spec change regenerates these pages, and CI fails\n\
         if a committed page has fallen behind — so what you read here is what the\n\
         assembler does, not what someone remembered it did.\n\n",
    );

    let _ = writeln!(out, "| CPU | Mnemonics | Forms | Byte order |");
    out.push_str("|---|---|---|---|\n");
    for cpu in cpus {
        let forms: usize = cpu.set.instructions.iter().map(|i| i.forms.len()).sum();
        let _ = writeln!(
            out,
            "| [{}](instructions/{}.md) | {} | {} | {} |",
            cpu.set.cpu,
            cpu.slug,
            cpu.set.instructions.len(),
            forms,
            match cpu.set.endianness {
                isa::Endianness::Little => "little-endian",
                isa::Endianness::Big => "big-endian",
            }
        );
    }

    out.push_str(
        "\nThese CPUs encode an instruction as an opcode **word** with its operand\n\
         fields inside it, so they are listed by base opcode and encoding class\n\
         instead of by form:\n\n\
         | CPU | Instructions | Word order |\n|---|---|---|\n",
    );
    for cpu in word {
        let _ = writeln!(
            out,
            "| [{}](instructions/{}.md) | {} | {} |",
            cpu.name,
            cpu.slug,
            cpu.rows.len(),
            match cpu.endianness {
                isa::Endianness::Little => "little-endian",
                isa::Endianness::Big => "big-endian",
            }
        );
    }

    out.push_str(
        "\n[Motorola 68000](instructions/m68k.md) has a page of its own shape again:\n\
         it packs operand fields into the opcode word, so its forms give a base word\n\
         and their operands rather than a byte count.\n\n\
         ## Not generated\n\n\
         Two CPUs have no page here rather than a misleading one. Both assemble and\n\
         disassemble normally; only the *reference table* is missing.\n\n\
         **Zilog Z8000** — its spec is thirteen separate tables with thirteen element\n\
         types, one per instruction family, rather than one list. Rendering it means\n\
         thirteen renderers or a reshaped spec, and the second is the better question\n\
         to answer first.\n\n\
         **Motorola 6809** — computed operands: its postbyte selects an indexing mode\n\
         whose length depends on the mode chosen. Its spec also carries no\n\
         per-instruction summaries, so a table would be opcodes without prose.\n\n\
         ## Provenance\n\n\
         These specs are authored from datasheets in the family's primary reference\n\
         library, not extracted from an emulator's decode loop. That provenance is\n\
         recorded per module in prose today, and unevenly: ten of the nineteen carry\n\
         a citation and nine do not. Linking each page to its datasheet is the\n\
         other half of this reference, and it waits on the citation being data\n\
         rather than a paragraph.\n",
    );
    out
}

fn render_cpu(cpu: &Cpu) -> String {
    let mut out = format!("# {}\n\n", cpu.set.cpu);
    out.push_str(&generated_note(cpu.module));

    let forms: usize = cpu.set.instructions.iter().map(|i| i.forms.len()).sum();
    let undocumented: usize = cpu
        .set
        .instructions
        .iter()
        .flat_map(|i| i.forms)
        .filter(|f| f.undocumented)
        .count();

    let _ = write!(
        out,
        "\n{} mnemonics, {} encoded forms, {}. Generated from \
         [`crates/isa/src/{}.rs`](https://github.com/asm198x/asm198x/blob/main/crates/isa/src/{}.rs).\n",
        cpu.set.instructions.len(),
        forms,
        match cpu.set.endianness {
            isa::Endianness::Little => "operands little-endian",
            isa::Endianness::Big => "operands big-endian",
        },
        cpu.module,
        cpu.module,
    );
    if undocumented > 0 {
        let _ = write!(
            out,
            "\n{undocumented} of those forms are undocumented opcodes, marked \
             **undocumented** below. They encode, because real programs use them.\n"
        );
    }
    out.push_str("\nCycle counts show the base cost; `+p` is an extra cycle when an\nindexed access crosses a page boundary, `+t` when a branch is taken.\n");

    for instruction in cpu.set.instructions {
        let _ = write!(
            out,
            "\n## {}\n\n{}\n\n",
            instruction.mnemonic, instruction.summary
        );
        out.push_str("| Mode | Opcode | Operands | Bytes | Cycles | Flags |\n");
        out.push_str("|---|---|---|---|---|---|\n");
        for form in instruction.forms {
            let _ = writeln!(
                out,
                "| {}{} | `{}` | {} | {} | {} | {} |",
                cell(form.mode),
                if form.undocumented {
                    " **undocumented**"
                } else {
                    ""
                },
                hex(form.opcode, form.suffix),
                operands(form.operands),
                form.len(),
                cycles(form.cycles),
                cell(if form.flags.is_empty() {
                    "—"
                } else {
                    form.flags
                }),
            );
        }
    }
    out
}

/// Opcode bytes as hex. A suffix byte — the Z80 `DD CB <d> <op>` group, whose
/// final opcode byte follows the displacement — is shown in its real position
/// so the row reads as the bytes actually land.
fn hex(opcode: &[u8], suffix: &[u8]) -> String {
    let head = opcode
        .iter()
        .map(|b| format!("{b:02X}"))
        .collect::<Vec<_>>()
        .join(" ");
    if suffix.is_empty() {
        head
    } else {
        let tail = suffix
            .iter()
            .map(|b| format!("{b:02X}"))
            .collect::<Vec<_>>()
            .join(" ");
        format!("{head} .. {tail}")
    }
}

fn operands(operands: &[isa::Operand]) -> String {
    if operands.is_empty() {
        return "—".to_string();
    }
    operands
        .iter()
        .map(|o| {
            let kind = match o.kind {
                isa::OperandKind::Immediate => "imm",
                isa::OperandKind::ImmediateBe => "imm-be",
                isa::OperandKind::Address => "addr",
                isa::OperandKind::RelativePc => "rel",
                isa::OperandKind::Displacement => "disp",
            };
            format!("{kind}{}", o.bytes * 8)
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn cycles(cycles: isa::Cycles) -> String {
    let mut out = cycles.base.to_string();
    if cycles.branch_taken > 0 {
        out.push_str(" +t");
    }
    if cycles.page_cross > 0 {
        out.push_str(" +p");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every CPU listed must actually carry instructions.
    ///
    /// Four modules export a `SET: InstructionSet` whose `instructions` is
    /// empty, standing beside the real bespoke table — `tms9900`, `pdp11`,
    /// `cp1610`, `z8000`, and `mos6809` does the same. Listing one here
    /// generates a page that states, in a confident table, that the CPU has
    /// zero instructions. It is wrong in the most credible possible way, and
    /// only a count catches it.
    #[test]
    fn every_listed_cpu_has_instructions() {
        let empty: Vec<&str> = cpus()
            .iter()
            .filter(|cpu| cpu.set.instructions.is_empty())
            .map(|cpu| cpu.slug)
            .collect();
        assert!(
            empty.is_empty(),
            "these CPUs would generate an empty reference page: {}\n\
             Their `SET` is a placeholder; the real spec is a bespoke table that \
             needs its own renderer.",
            empty.join(", ")
        );
    }

    /// Slugs become file names and URLs, so a duplicate would silently have one
    /// page overwrite another.
    /// Every generated table row has the column count its header promises.
    ///
    /// A stray `|` does not fail a build. Markdown splits the cell and carries
    /// on, so the page renders a mangled row that nobody notices — and the
    /// encoding formulas are made of pipes, so this is the mistake this
    /// generator is most likely to make.
    #[test]
    fn no_generated_table_row_is_split_by_a_stray_pipe() {
        for page in pages() {
            let mut expected: Option<usize> = None;
            for (n, line) in page.body.lines().enumerate() {
                if !line.starts_with('|') {
                    expected = None;
                    continue;
                }
                // An escaped `\|` does not divide a cell.
                let dividers = line.replace("\\|", "").matches('|').count();
                match expected {
                    None => expected = Some(dividers),
                    Some(want) => assert_eq!(
                        dividers,
                        want,
                        "{}:{}: row has {dividers} dividers where the table's \
                         header has {want}:\n  {line}",
                        page.path,
                        n + 1
                    ),
                }
            }
        }
    }

    #[test]
    fn slugs_are_unique() {
        let mut seen = std::collections::BTreeSet::new();
        for cpu in cpus() {
            assert!(seen.insert(cpu.slug), "duplicate slug `{}`", cpu.slug);
        }
    }
}
