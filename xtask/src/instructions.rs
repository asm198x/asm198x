//! Per-CPU instruction reference pages, generated from the `isa` crate.
//!
//! Docs-site plan R1 and R5: the reference comes from the spec, so a spec
//! change regenerates the page and a new CPU documents itself the moment its
//! spec lands. Nothing here is hand-authored, and `cargo xtask docs --check`
//! fails if a committed page has fallen behind.
//!
//! # What is not here
//!
//! **Provenance links.** R1 asks each page to link into the umbrella
//! `reference/` datasheet library. Pages name their sources instead, from
//! `isa::provenance`. The library is private, and
//! `decisions/citing-restricted-provenance-sources.md` rules out linking
//! restricted material regardless of where it is held.
//!
//! **Six CPUs.** The 68000, 6809, TMS9900, PDP-11, CP1610 and Z8000 encode
//! with models a form table cannot describe, and are listed on the index page
//! rather than left quietly absent.
//!
//! Four of them export a `SET: InstructionSet` whose `instructions` is
//! **empty** — a placeholder beside the real, bespoke table. That is a trap
//! worth naming: reading the exports alone suggests they are on the standard
//! model, and generating from them produces a page confidently reporting zero
//! instructions. `every_listed_cpu_has_instructions` exists so that cannot
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
                    class: i.class.name(),
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
                    class: i.class.name(),
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
                    class: i.class.name(),
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
    // `<` is the dangerous one. Markdown passes inline HTML straight through,
    // so a placeholder like `<ea>` is not text — it is an opening tag, and the
    // renderer swallows it and everything up to a matching close. The 68000's
    // operand column rendered *empty* for all 78 effective-address forms this
    // way, and nothing looked wrong in the markdown.
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('|', "\\|")
}

/// The variant's name, via `Debug` — the specs derive it, and a hand-written
/// name table here would be a second copy of the class list to keep in step.
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
    out.push_str(&machines(cpu.module));
    out.push_str(&provenance(cpu.module));
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
    out.push_str(&machines("m68k"));
    out.push_str(&provenance("m68k"));
    out
}

/// The 6809, whose spec is organised by *operand shape* rather than by form.
///
/// An instruction is a mnemonic and a [`Kind`](isa::mos6809::Kind) — inherent,
/// branch, register/memory, transfer or stack — and the kind carries the
/// opcodes for whichever modes that shape allows. So the page groups by shape,
/// which is the distinction the spec actually makes, instead of imposing a
/// uniform table that would leave most cells empty.
fn render_mos6809() -> String {
    use isa::mos6809::Kind;

    let mut out = String::from("# Motorola 6809\n\n");
    out.push_str(&generated_note("mos6809"));
    let _ = write!(
        out,
        "\n{} mnemonics, operands big-endian. Generated from \
         [`crates/isa/src/mos6809.rs`](https://github.com/asm198x/asm198x/blob/main/crates/isa/src/mos6809.rs).\n\n\
         The 6809's indexed mode computes its own length: a postbyte selects the \
         indexing form, and how many bytes follow depends on which form. So the \
         tables below give the opcode for each addressing mode an instruction \
         supports, and the operand that follows is whatever the mode calls for.\n\n\
         Instructions are grouped by operand shape, which is the distinction this \
         CPU's specification draws.\n",
        isa::mos6809::SET.len(),
    );

    // Register/memory first: it is most of the set, and the four-mode row is
    // what a reader is usually looking for.
    out.push_str(
        "\n## Register and memory\n\n\
         Each supports some subset of the four standard modes; a blank cell means \
         the mode does not exist for that instruction. **Width** is the immediate's \
         size in bytes.\n\n\
         | Mnemonic | Immediate | Direct | Indexed | Extended | Width | Summary |\n\
         |---|---|---|---|---|---|---|\n",
    );
    for insn in isa::mos6809::SET {
        if let Kind::Mem {
            imm,
            direct,
            indexed,
            extended,
            width,
        } = &insn.kind
        {
            let _ = writeln!(
                out,
                "| {} | {} | {} | {} | {} | {} | {} |",
                insn.mnemonic,
                opcode_or_blank(imm),
                opcode_or_blank(direct),
                opcode_or_blank(indexed),
                opcode_or_blank(extended),
                width,
                cell(isa::mos6809::summary(insn.mnemonic)),
            );
        }
    }

    out.push_str(
        "\n## Inherent\n\n\
         No operand — the opcode is the whole instruction.\n\n\
         | Mnemonic | Opcode | Summary |\n|---|---|---|\n",
    );
    for insn in isa::mos6809::SET {
        if let Kind::Inherent(opcode) = &insn.kind {
            let _ = writeln!(
                out,
                "| {} | {} | {} |",
                insn.mnemonic,
                opcode_or_blank(opcode),
                cell(isa::mos6809::summary(insn.mnemonic)),
            );
        }
    }

    out.push_str(
        "\n## Branches\n\n\
         Every branch has a short form with an 8-bit displacement and a long form \
         with a 16-bit one. The assembler picks by range unless the source forces \
         a spelling.\n\n\
         | Mnemonic | Short | Long | Summary |\n|---|---|---|---|\n",
    );
    for insn in isa::mos6809::SET {
        if let Kind::Branch { short, long } = &insn.kind {
            let _ = writeln!(
                out,
                "| {} | {} | {} | {} |",
                insn.mnemonic,
                opcode_or_blank(short),
                opcode_or_blank(long),
                cell(isa::mos6809::summary(insn.mnemonic)),
            );
        }
    }

    out.push_str(
        "\n## Transfer and exchange\n\n\
         The opcode is followed by a postbyte packing two 4-bit register codes, \
         source in the high nibble. Both registers must be the same width.\n\n\
         | Mnemonic | Opcode | Summary |\n|---|---|---|\n",
    );
    for insn in isa::mos6809::SET {
        if let Kind::Transfer(opcode) = &insn.kind {
            let _ = writeln!(
                out,
                "| {} | `{:02X}` | {} |",
                insn.mnemonic,
                opcode,
                cell(isa::mos6809::summary(insn.mnemonic)),
            );
        }
    }

    out.push_str(
        "\n## Stack\n\n\
         The opcode is followed by a one-byte register mask — `PC U/S Y X DP B A \
         CC`, high bit first. Registers push in the order CC, A, B, DP, X, Y, U/S, \
         PC and pull in reverse.\n\n\
         | Mnemonic | Opcode | Stack | Summary |\n|---|---|---|---|\n",
    );
    for insn in isa::mos6809::SET {
        if let Kind::Stack { opcode, u_stack } = &insn.kind {
            let _ = writeln!(
                out,
                "| {} | `{:02X}` | {} | {} |",
                insn.mnemonic,
                opcode,
                if *u_stack { "U" } else { "S" },
                cell(isa::mos6809::summary(insn.mnemonic)),
            );
        }
    }
    out.push_str(&machines("mos6809"));
    out.push_str(&provenance("mos6809"));
    out
}

/// An opcode sequence as hex, or a blank cell — an empty slice marks a mode the
/// instruction does not have, which is a real fact about it and not missing
/// data.
fn opcode_or_blank(opcode: &[u8]) -> String {
    if opcode.is_empty() {
        return String::new();
    }
    format!(
        "`{}`",
        opcode
            .iter()
            .map(|b| format!("{b:02X}"))
            .collect::<Vec<_>>()
            .join(" ")
    )
}

/// The Z8000, whose spec is thirteen tables — one per instruction family, each
/// with its own element type.
///
/// So the page is thirteen tables too. That is not a workaround: the families
/// are the CPU's own distinction, and each has fields the others do not — a
/// block move has a shape and a control nibble, an I/O instruction has a
/// direction. Flattening them into one table would mean a row of mostly empty
/// columns and would lose exactly the information a reader came for.
///
/// All thirteen types share `mnemonic` and `summary`, so every row is
/// described. The columns beyond those are whatever the family actually has.
fn render_z8000() -> String {
    use isa::z8000;

    let mut out = String::from("# Zilog Z8000\n\n");
    out.push_str(&generated_note("z8000"));

    let total = z8000::CONTROL.len()
        + z8000::MONO.len()
        + z8000::STACK.len()
        + z8000::SHIFTS.len()
        + z8000::EXTENDS.len()
        + z8000::BITS.len()
        + z8000::MULDIV.len()
        + z8000::BLOCK.len()
        + z8000::SIMPLE_IO.len()
        + z8000::BLOCK_IO.len()
        + z8000::CONTROLS.len()
        + z8000::MISC.len()
        + z8000::INSTRUCTIONS.len();

    let _ = write!(
        out,
        "\n{total} instructions, words big-endian. Generated from \
         [`crates/isa/src/z8000.rs`](https://github.com/asm198x/asm198x/blob/main/crates/isa/src/z8000.rs).\n\n\
         The Z8000 is specified family by family, because the families genuinely \
         differ: a block move carries a shape and a control nibble, an I/O \
         instruction carries a direction, a shift carries whether its count is a \
         following word. The tables below keep that structure, and each shows the \
         columns its family actually has.\n\n\
         Sizes are the operand widths a mnemonic assembles at. `Rn` is a word \
         register, `@Rn` an indirect one, `addr` a direct address and `addr(Rn)` an \
         indexed one.\n"
    );

    // --- the dyadic family, the one most source spends its time in ---------
    out.push_str(
        "\n## Arithmetic, logic and load\n\n\
         The dyadic family: a destination register and a source in any of the \
         addressing modes listed.\n\n\
         | Mnemonic | Base | Size | Source modes | Summary |\n|---|---|---|---|---|\n",
    );
    for i in z8000::INSTRUCTIONS.iter().filter(|i| !i.store) {
        let _ = writeln!(
            out,
            "| {} | `{:02X}` | {} | {} | {} |",
            i.mnemonic,
            i.base6,
            i.size.suffix(),
            cell(&z8000::mode_names(i.modes)),
            cell(i.summary)
        );
    }

    out.push_str(
        "\n## Program control\n\n| Mnemonic | Base | Shape | Summary |\n|---|---|---|---|\n",
    );
    for i in z8000::CONTROL {
        let _ = writeln!(
            out,
            "| {} | `{:04X}` | {} | {} |",
            i.mnemonic,
            i.base,
            cell(i.kind.describe()),
            cell(i.summary)
        );
    }

    out.push_str(
        "\n## Single-operand\n\n| Mnemonic | Base | Sub-op | Size | Summary |\n|---|---|---|---|---|\n",
    );
    for i in z8000::MONO {
        let _ = writeln!(
            out,
            "| {} | `{:02X}` | `{:X}` | {} | {} |",
            i.mnemonic,
            i.base6,
            i.subop,
            i.size.suffix(),
            cell(i.summary)
        );
    }

    out.push_str("\n## Stack\n\n| Mnemonic | Base | Size | Summary |\n|---|---|---|---|\n");
    for i in z8000::STACK {
        let _ = writeln!(
            out,
            "| {} | `{:02X}` | {} | {} |",
            i.mnemonic,
            i.base6,
            i.size.suffix(),
            cell(i.summary)
        );
    }

    out.push_str(
        "\n## Shift and rotate\n\n| Mnemonic | Base | Size | Count | Summary |\n|---|---|---|---|---|\n",
    );
    for i in z8000::SHIFTS {
        let _ = writeln!(
            out,
            "| {} | `{:02X}` | {} | {} | {} |",
            i.mnemonic,
            i.base6,
            i.size.suffix(),
            cell(i.kind.describe()),
            cell(i.summary)
        );
    }

    out.push_str("\n## Sign extend\n\n| Mnemonic | Sub-op | Size | Summary |\n|---|---|---|---|\n");
    for i in z8000::EXTENDS {
        let _ = writeln!(
            out,
            "| {} | `{:X}` | {} | {} |",
            i.mnemonic,
            i.subop,
            i.size.suffix(),
            cell(i.summary)
        );
    }

    out.push_str("\n## Bit\n\n| Mnemonic | Base | Size | Summary |\n|---|---|---|---|\n");
    for i in z8000::BITS {
        let _ = writeln!(
            out,
            "| {} | `{:02X}` | {} | {} |",
            i.mnemonic,
            i.base6,
            i.size.suffix(),
            cell(i.summary)
        );
    }

    out.push_str(
        "\n## Multiply and divide\n\n| Mnemonic | Base | Destination | Source | Summary |\n|---|---|---|---|---|\n",
    );
    for i in z8000::MULDIV {
        let _ = writeln!(
            out,
            "| {} | `{:02X}` | {} | {} | {} |",
            i.mnemonic,
            i.base6,
            i.dest.suffix(),
            i.src.suffix(),
            cell(i.summary)
        );
    }

    out.push_str(
        "\n## Block move and compare\n\n| Mnemonic | Base | Size | Operands | Summary |\n|---|---|---|---|---|\n",
    );
    for i in z8000::BLOCK {
        let _ = writeln!(
            out,
            "| {} | `{:02X}` | {} | {} | {} |",
            i.mnemonic,
            i.base6,
            i.size.suffix(),
            cell(i.shape.describe()),
            cell(i.summary)
        );
    }

    out.push_str(
        "\n## Input and output\n\n| Mnemonic | Direction | Size | Summary |\n|---|---|---|---|\n",
    );
    for i in z8000::SIMPLE_IO {
        let _ = writeln!(
            out,
            "| {} | {} | {} | {} |",
            i.mnemonic,
            if i.input { "in" } else { "out" },
            i.size.suffix(),
            cell(i.summary)
        );
    }

    out.push_str("\n## Block input and output\n\n| Mnemonic | Size | Summary |\n|---|---|---|\n");
    for i in z8000::BLOCK_IO {
        let _ = writeln!(
            out,
            "| {} | {} | {} |",
            i.mnemonic,
            i.size.suffix(),
            cell(i.summary)
        );
    }

    out.push_str("\n## CPU control\n\n| Mnemonic | Operands | Summary |\n|---|---|---|\n");
    for i in z8000::CONTROLS {
        let _ = writeln!(
            out,
            "| {} | {} | {} |",
            i.mnemonic,
            cell(i.kind.describe()),
            cell(i.summary)
        );
    }

    out.push_str(
        "\n## Other\n\n| Mnemonic | Top | Size | Operands | Summary |\n|---|---|---|---|---|\n",
    );
    for i in z8000::MISC {
        let _ = writeln!(
            out,
            "| {} | `{:02X}` | {} | {} | {} |",
            i.mnemonic,
            i.top,
            i.size.suffix(),
            cell(i.kind.describe()),
            cell(i.summary)
        );
    }
    out.push_str(&machines("z8000"));
    out.push_str(&provenance("z8000"));
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
            path: format!("reference/instructions/{}.md", cpu.slug),
            body: render_cpu(cpu),
        })
        .collect();
    out.extend(word_cpus().iter().map(|cpu| Page {
        path: format!("reference/instructions/{}.md", cpu.slug),
        body: render_word_cpu(cpu),
    }));
    out.push(Page {
        path: "reference/instructions/m68k.md".to_string(),
        body: render_m68k(),
    });
    out.push(Page {
        path: "reference/instructions/mos6809.md".to_string(),
        body: render_mos6809(),
    });
    out.push(Page {
        path: "reference/instructions/z8000.md".to_string(),
        body: render_z8000(),
    });
    out.push(Page {
        path: "reference/instructions.md".to_string(),
        body: render_index(&cpus, &word_cpus()),
    });
    out
}

/// The `SUMMARY.md` lines listing the instruction reference, so a new CPU
/// appears in the sidebar without anyone remembering to add it.
pub fn summary_lines() -> String {
    let mut out = String::from("- [Instruction reference](reference/instructions.md)\n");
    for cpu in cpus() {
        let _ = writeln!(
            out,
            "  - [{}](reference/instructions/{}.md)",
            cpu.set.cpu, cpu.slug
        );
    }
    for cpu in word_cpus() {
        let _ = writeln!(
            out,
            "  - [{}](reference/instructions/{}.md)",
            cpu.name, cpu.slug
        );
    }
    out.push_str("  - [Motorola 68000](reference/instructions/m68k.md)\n");
    out.push_str("  - [Motorola 6809](reference/instructions/mos6809.md)\n");
    out.push_str("  - [Zilog Z8000](reference/instructions/z8000.md)\n");
    out
}

/// The machines that used one CPU, linked into the Code198x catalogue.
///
/// An instruction set on its own is an abstraction; this is the join back to
/// the hardware people actually had. A machine the catalogue has no page for is
/// named without a link — it has not stopped existing, and a link to a 404
/// would be worse than plain text.
fn machines(module: &str) -> String {
    let machines = isa::machines::machines_for(module);
    if machines.is_empty() {
        return String::new();
    }
    let listed = machines
        .iter()
        .map(|m| {
            if m.catalogued {
                format!("[{}](https://code198x.com/{}/)", cell(m.name), m.slug)
            } else {
                cell(m.name)
            }
        })
        .collect::<Vec<_>>()
        .join(", ");
    format!("\n## Machines\n\n{listed}\n")
}

/// The source list for one CPU's page.
///
/// Covers R1. Sources are named rather than linked: the reference library is a
/// private repository, and `decisions/citing-restricted-provenance-sources.md`
/// rules out linking restricted material in any case. `Source::library` still
/// records where each document sits, for use inside this repo.
fn provenance(module: &str) -> String {
    let sources = isa::provenance::sources_for(module);
    if sources.is_empty() {
        return String::new();
    }
    let mut out = String::from("\n## Sources\n\nEncodings on this page were taken from:\n\n");
    for source in sources {
        let year = source.year.map_or(String::new(), |y| format!(", {y}"));
        let _ = writeln!(
            out,
            "- *{}* — {}{}",
            cell(source.title),
            cell(source.attribution),
            year
        );
    }
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
         if a committed page has fallen behind. Each page lists the manuals and\n\
         datasheets its specification was written from.\n\n",
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
        "\n[Motorola 68000](instructions/m68k.md), [Motorola 6809](instructions/mos6809.md)\n\
         and [Zilog Z8000](instructions/z8000.md) each have a page of their own shape:\n\
         the 68000 packs operand fields into the opcode word, so its forms give a base\n\
         word rather than a byte count; the 6809 groups by operand shape, because its\n\
         indexed mode computes its own length from a postbyte; and the Z8000 is\n\
         specified family by family, because its families genuinely differ.\n\n\
         Every CPU the assembler has a specification for is documented here.\n",
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
    out.push_str(&machines(cpu.module));
    out.push_str(&provenance(cpu.module));
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

    /// A table cell must not carry a raw `<`.
    ///
    /// Markdown treats inline HTML as HTML. `<ea>` in the 68000's operand
    /// column was parsed as a tag and rendered as nothing at all, so the
    /// column was blank on the published page while the markdown source read
    /// perfectly. Escaping happens in `cell`; this holds the escaping.
    #[test]
    fn no_generated_table_cell_carries_a_raw_angle_bracket() {
        for page in pages() {
            for (n, line) in page.body.lines().enumerate() {
                if !line.starts_with('|') {
                    continue;
                }
                assert!(
                    !line.contains('<'),
                    "{}:{}: table row carries a raw `<`, which markdown reads \
                     as an HTML tag and drops:\n  {line}",
                    page.path,
                    n + 1
                );
            }
        }
    }

    /// No CPU is documented without saying where its specification came from.
    ///
    /// R1 rests on these pages being a reading of the datasheets, so an
    /// uncited page is not evidence of anything. Nine CPUs were uncited before
    /// the provenance table landed.
    #[test]
    fn every_generated_page_cites_its_sources() {
        let uncited: Vec<String> = pages()
            .iter()
            .filter(|p| p.path.starts_with("instructions/"))
            .filter(|p| !p.body.contains("## Sources"))
            .map(|p| p.path.clone())
            .collect();
        assert!(
            uncited.is_empty(),
            "these pages document a CPU without citing a source: {}\n\
             Add the documents it was authored from to `isa::provenance::PROVENANCE`.",
            uncited.join(", ")
        );
    }

    #[test]
    fn slugs_are_unique() {
        let mut seen = std::collections::BTreeSet::new();
        for cpu in cpus() {
            assert!(seen.insert(cpu.slug), "duplicate slug `{}`", cpu.slug);
        }
    }
}
