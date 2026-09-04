//! The sjasmplus Z80 dialect.
//!
//! A thin surface over the shared Z80 core in [`crate::dialects::z80`]. The Z80
//! instruction/operand syntax is identical to pasmo's; sjasmplus differs only
//! in its surface, which is all that lives here:
//!
//! - **Comments**: `;` *and* `//`.
//! - **Numbers**: a superset — `$hex`, `0xhex`, `NNh`; `%binary`, `0bbinary`,
//!   `NNb`; decimal; `'c'` char.
//!
//! Directives and operand resolution are shared. sjasmplus also targets the
//! Spectrum Next, so it carries the same `z80n` target flag as pasmo. Unlike
//! pasmo, a leading-`.` label is *local*, scoped under the most recent global
//! label (so `.loop` may recur) — see [`Z80Syntax::scopes_locals`].
//!
//! **Conditional assembly** (language-surface U8): sjasmplus is the first
//! keyword-style adopter of the shared `ast::CondEval`/`ast::evaluate`
//! framework — `IF`/`IFDEF`/`IFNDEF`/`ELSE`/`ENDIF` plus `DEFINE` (textual
//! substitution, probe-pinned). All three entry points route through the
//! z80 keyword pipeline (`z80::parse_program_keyword` + the `SjasmEval`
//! walk), so every line lowers with the live environment and an include in
//! an untaken branch never loads. pasmo stays on the eager walker — its
//! conditional adoption is demand-gated
//! (`decisions/conditional-assembly-framework.md`).
//!
//! **Macros** (#93): `MACRO`/`ENDM` with dot-prefixed locals scoped per
//! expansion, over the shared expander in [`crate::dialects::macros`] — this
//! module supplies only the grammar, governed by
//! `decisions/macro-expansion-framework.md`. Repetition (`DUP`/`REPT`) is a
//! conditional-framework item rather than a macro one, because its count is an
//! expression over the environment.
//!
//! **Modules** (#93): `MODULE`/`ENDMODULE` prefix the names defined inside
//! them, and a leading `@` escapes to the global scope. A reference has two
//! candidates — the fully-qualified name and the bare global one, with no
//! walk-up between — which is why the choice is repaired after the walk rather
//! than made as each line is read. See
//! `docs/plans/2026-08-23-001-feat-sjasmplus-modules-plan.md`.
//!
//! TODO: macros across include boundaries, and the name-first `name MACRO`
//! spelling the reference also accepts (#205). `ELSEIF` and the dotted
//! conditional spellings landed 2026-08-18; colon-inline blocks and conditions
//! on forward symbols remain open under #67.

use std::collections::BTreeMap;

use crate::dialect::{Dialect, Oversize};
use crate::dialects::macros::{self, Expand};
use crate::dialects::z80::{self, Z80Syntax};
use crate::directives::{Category, Directive, Pattern};
use crate::engine::{AsmError, Expr, Operation, Piece, Statement};
use crate::source::{SourceLoader, SourceMap};

/// The sjasmplus Z80 dialect. `z80n` selects the target instruction set
/// (sjasmplus emits Z80N when targeting the Next).
/// What sjasmplus accepts beyond the shared Z80 base.
///
/// `bytes` overrides the base entry rather than adding a second one:
/// sjasmplus spells `db` four ways and adds `byte`, and two entries claiming
/// one concept would show as two rows in a matrix.
///
/// `include` is declared here because the shared Z80 base describes neither
/// dialect's complete file-inclusion vocabulary.
/// The data directives sjasmplus has beyond the shared Z80 set. Named once, so
/// the declaration and the dispatch cannot drift apart.
const SJASMPLUS_DATA: &[&str] = &[
    "word", "dword", "dd", "defd", "d24", "dz", "dc", "dh", "hex", "defh", "dg", "defg", "abyte",
    "abytec", "abytez", "block",
];

pub const DIRECTIVES: &[Directive] = &[
    Directive {
        id: "bytes",
        pattern: Pattern::Exact(&["defb", "db", "defm", "dm", "byte"]),
        category: Category::Operation,
    },
    // The data directives sjasmplus has beyond the shared Z80 set. Widths,
    // terminators, hex and bit-graphics — each shape probed against v1.21.0.
    Directive {
        id: "sjasmplus-data",
        pattern: Pattern::Exact(SJASMPLUS_DATA),
        category: Category::Operation,
    },
    Directive {
        id: "include",
        pattern: Pattern::Exact(&["include"]),
        category: Category::Operation,
    },
    Directive {
        id: "incbin",
        pattern: Pattern::Exact(&["incbin"]),
        category: Category::Operation,
    },
    // Scanner- and expander-handled, like acme's conditionals: these never
    // reach `parse_op`, because the macro expander and the conditional walk
    // consume them first. Declared all the same — the surface describes the
    // dialect, and a matrix showing sjasmplus with no macros and no `IF` would
    // be describing whichever parser happened to read the line.
    //
    // **One entry per construct, named by its opener.** `ENDM`, `EDUP`, `ELSE`
    // and `ENDIF` are parts of a block rather than vocabulary of their own, the
    // same call the plan already made for acme's `}`. A matrix answering "does
    // this dialect have macros" wants one row, not two.
    Directive {
        id: "macro",
        pattern: Pattern::Exact(&["macro"]),
        category: Category::Operation,
    },
    Directive {
        id: "repeat",
        pattern: Pattern::Exact(&["dup", "rept"]),
        category: Category::Operation,
    },
    Directive {
        id: "conditional",
        pattern: Pattern::Sigilled {
            sigil: '.',
            names: &["if", "ifdef", "ifndef"],
            required: false,
        },
        category: Category::Operation,
    },
    Directive {
        id: "define",
        pattern: Pattern::Exact(&["define"]),
        category: Category::Operation,
    },
    Directive {
        id: "opt",
        pattern: Pattern::Sigilled {
            sigil: '.',
            names: &["opt"],
            required: false,
        },
        category: Category::Operation,
    },
    // Selects end-of-line comments for SjASMPlus's optional `.sld` sidecar.
    // It changes neither machine bytes nor Asm198x's Debug198x output, so the
    // validated COMMENT form is an explicitly inert source-compatibility word.
    Directive {
        id: "sldopt-comment",
        pattern: Pattern::Sigilled {
            sigil: '.',
            names: &["sldopt"],
            required: false,
        },
        category: Category::Operation,
    },
    // Named by its opener, like the blocks above: `ENDMODULE`/`ENDMOD` are
    // parts of the block rather than vocabulary of their own.
    //
    // Deliberately **not** in `is_directive`. The reference reads a column-0
    // `MODULE` as a label and the name after it as an instruction (probe m27),
    // so treating it as a directive would accept source sjasmplus rejects.
    Directive {
        id: "module",
        pattern: Pattern::Exact(&["module"]),
        category: Category::Operation,
    },
    // `STRUCT`/`ENDS` (#477): read as a block by the walk, like `MODULE`,
    // and for the same reason not in `is_directive` — the reference reads a
    // column-0 `STRUCT` as a label.
    Directive {
        id: "struct",
        pattern: Pattern::Exact(&["struct"]),
        category: Category::Operation,
    },
    Directive {
        id: "ends",
        pattern: Pattern::Exact(&["ends"]),
        category: Category::Operation,
    },
    // What sjasmplus has here and we do not.
    //
    // 82 spellings against 1.21.0 (STRUCT/ENDS left for #477). The `save*`, `device` and
    // `shellexec` families are in here rather than on a roadmap on purpose:
    // `assemble-io-model.md` scopes output to native containers, and
    // `decisions/sjasmplus-lua.md` keeps process execution out of the Lua
    // sandbox for good. For those, refusing with a diagnostic that says the
    // source is valid and the gap is ours is the honest end state, not a
    // staging post. `lua`/`endlua`/`includelua` are different: the same
    // decision accepts them behind the `lua` build feature, so their stay
    // here ends when that lands.
    // The device model (`docs/sjasmplus-device-model.md`). `DEVICE` and `SLOT`
    // emit nothing; `PAGE` opens a section, because two pages written at one
    // address concatenate in the output rather than colliding.
    // `SAVEBIN "file",start[,length]` writes a span of the address space to a
    // file of its own. It needs a `DEVICE` — without one SjASMPlus 1.21.0
    // answers "SAVEBIN only allowed in real device emulation mode" — and the
    // span is against a 64K space that starts zero-filled, so an address the
    // source never wrote saves as zero rather than as an error.
    //
    // A length of zero, or none at all, means to the end of the space. The rest
    // of the `save*` family writes a *container*, and each waits on its format
    // (`decisions/multi-artifact-output.md`); this one writes the bytes as they
    // stand, so it needs no format at all.
    Directive {
        id: "savebin",
        pattern: Pattern::Sigilled {
            sigil: '.',
            names: &["savebin"],
            required: false,
        },
        category: Category::Operation,
    },
    // `SAVETAP "file",CODE|BASIC,"name",start,length` wraps the same span in a
    // Spectrum tape: a ROM header block naming it, then the block itself. The
    // layout is `format198x-sinclair-zx-spectrum-tap`'s, which graduated to
    // Format198x when this became its second consumer.
    //
    // The forms that name no kind save the *device's* memory rather than a
    // span, so they wait on the same fact `SAVEBIN` does (asm198x#318).
    Directive {
        id: "savetap",
        pattern: Pattern::Sigilled {
            sigil: '.',
            names: &["savetap"],
            required: false,
        },
        category: Category::Operation,
    },
    Directive {
        id: "device",
        pattern: Pattern::Sigilled {
            sigil: '.',
            names: &["device", "slot"],
            required: false,
        },
        category: Category::Operation,
    },
    Directive {
        id: "page",
        pattern: Pattern::Sigilled {
            sigil: '.',
            names: &["page"],
            required: false,
        },
        category: Category::Operation,
    },
    Directive {
        id: "savecpr",
        pattern: Pattern::Sigilled {
            sigil: '.',
            names: &["savecpr"],
            required: false,
        },
        category: Category::Operation,
    },
    Directive {
        id: "display",
        pattern: Pattern::Sigilled {
            sigil: '.',
            names: &["display"],
            required: false,
        },
        category: Category::Operation,
    },
    Directive {
        id: "assert",
        pattern: Pattern::Sigilled {
            sigil: '.',
            names: &["assert"],
            required: false,
        },
        category: Category::Operation,
    },
    Directive {
        id: "align",
        pattern: Pattern::Sigilled {
            sigil: '.',
            names: &["align"],
            required: false,
        },
        category: Category::Operation,
    },
    Directive {
        id: "unsupported-sjasmplus",
        pattern: Pattern::Sigilled {
            sigil: '.',
            names: &[
                "binary",
                "bplist",
                "cspectmap",
                "defarray",
                "defdevice",
                "dephase",
                "disp",
                "emptytap",
                "emptytrd",
                "encoding",
                "endlua",
                "endm",
                "endt",
                "endw",
                "ent",
                "exa",
                "exd",
                "export",
                "fpos",
                "ifn",
                "ifnused",
                "ifused",
                "inchob",
                "includelua",
                "inctrd",
                "inf",
                "insert",
                "labelslist",
                "lua",
                "mmu",
                "outend",
                "output",
                "phase",
                "relocate_end",
                "relocate_start",
                "relocate_table",
                "save3dos",
                "saveamsdos",
                "savecdt",
                "savecpcsna",
                "savedev",
                "savehob",
                "savenex",
                "savesna",
                "savetrd",
                "setbp",
                "setbreakpoint",
                "shellexec",
                "size",
                "tapend",
                "tapout",
                "textarea",
                "undefine",
                "unphase",
                "while",
            ],
            required: false,
        },
        category: Category::KnownUnsupported,
    },
];

pub(crate) struct Sjasmplus {
    pub(crate) z80n: bool,
}

impl Dialect for Sjasmplus {
    /// Every instruction lowers by form; the only piece-encoded emissions
    /// are data directives, so an absent cycle record means data (#497).
    fn cycle_coverage(&self) -> crate::dialect::CycleCoverage {
        crate::dialect::CycleCoverage::Full
    }
    fn instruction_set(&self) -> &'static isa::InstructionSet {
        &isa::z80::SET
    }
    fn extension_set(&self) -> Option<&'static isa::InstructionSet> {
        self.z80n.then_some(&isa::z80::NEXT)
    }
    /// Assembly routes through the keyword-conditional pipeline (U8): the
    /// structure parse builds the shared conditional tree, and the
    /// `ast::evaluate` walk lowers each live line with the environment as of
    /// that point (an `equ` in a taken branch feeds later form selection).
    fn parse(&self, source: &str) -> Result<Vec<Statement>, AsmError> {
        Ok(self.parse_warned(source)?.0)
    }
    /// The advisories are sjasmplus's own: a condition that reached forward,
    /// and a label that never settled (#99).
    fn parse_warned(
        &self,
        source: &str,
    ) -> Result<(Vec<Statement>, Vec<crate::engine::Warning>), AsmError> {
        z80::assemble_keyword_warned(
            &SjasmplusSyntax,
            self.instruction_set(),
            self.extension_set(),
            source,
        )
    }
    fn parse_ast(&self, source: &str) -> Result<Option<crate::ast::Program>, AsmError> {
        Ok(Some(z80::parse_program_keyword(
            &SjasmplusSyntax,
            self.instruction_set(),
            self.extension_set(),
            crate::span::FileId(0),
            source,
            // The formatter must not expand: it lays source out, and would
            // otherwise write the expansions back in place of the macro.
            Expand::No,
        )?))
    }
    /// The include-capable parse (language-surface U2, conditional-aware
    /// since U8): includes resolve lazily *inside* the conditional walk, so
    /// a guarded include in an untaken branch never loads (KTD1) and the
    /// environment threads through the boundary in both directions.
    fn parse_multi(
        &self,
        map: &mut SourceMap,
        loader: &dyn SourceLoader,
    ) -> Result<Vec<Statement>, AsmError> {
        Ok(self.parse_multi_warned(map, loader)?.0)
    }
    fn parse_multi_warned(
        &self,
        map: &mut SourceMap,
        loader: &dyn SourceLoader,
    ) -> Result<(Vec<Statement>, Vec<crate::engine::Warning>), AsmError> {
        z80::parse_program_multi_keyword_warned(
            &SjasmplusSyntax,
            self.instruction_set(),
            self.extension_set(),
            map,
            loader,
        )
    }
    /// sjasmplus truncates an over-range byte to its low 8 bits and warns.
    fn oversized_byte_policy(&self) -> Oversize {
        Oversize::TruncateWarn
    }
}

/// sjasmplus's surface syntax.
/// The devices sjasmplus accepts, with the page and slot counts it bounds
/// `PAGE`/`SLOT` by. Probed against 1.21.0 — see
/// `docs/sjasmplus-device-model.md` for how, and for the two facts that are
/// not guessable from the names (`ZXSPECTRUMNEXT` is 224 pages across 8 slots;
/// `NOSLOT64K` is 32 pages in one).
///
/// `NONE` is absent on purpose: it behaves as no `DEVICE` line at all, with no
/// bounds and no write check, so it is handled where the device is set rather
/// than sitting here with fabricated numbers.
const DEVICES: &[(&str, usize, usize, usize)] = &[
    ("ZXSPECTRUM48", 4, 4, 0x4000),
    ("ZXSPECTRUM128", 8, 4, 0x4000),
    ("ZXSPECTRUM256", 16, 4, 0x4000),
    ("ZXSPECTRUM512", 32, 4, 0x4000),
    ("ZXSPECTRUM1024", 64, 4, 0x4000),
    ("ZXSPECTRUM2048", 128, 4, 0x4000),
    ("ZXSPECTRUM4096", 256, 4, 0x4000),
    ("ZXSPECTRUM8192", 512, 4, 0x4000),
    ("ZXSPECTRUMNEXT", 224, 8, 0x2000),
    ("AMSTRADCPC464", 4, 4, 0x4000),
    ("AMSTRADCPC6128", 8, 4, 0x4000),
    // 32 pages — 512 KiB, the largest cartridge `SAVECPR` writes (#538).
    ("AMSTRADCPCPLUS", 32, 4, 0x4000),
    ("NOSLOT64K", 32, 1, 0x10000),
];

struct SjasmplusSyntax;

impl SjasmplusSyntax {
    fn parse_savecpr(&self, args: &str, line: usize) -> Result<Operation, AsmError> {
        let parts = crate::dialects::mos6502::split_top_level(args.trim(), ',');
        let [name, pages] = parts.as_slice() else {
            return Err(AsmError::new(line, "`SAVECPR` needs `\"file\",size`"));
        };
        let name = name
            .trim()
            .strip_prefix('"')
            .and_then(|text| text.strip_suffix('"'))
            .ok_or_else(|| AsmError::new(line, "`SAVECPR` needs a quoted file name"))?;
        Ok(Operation::SaveCpr {
            name: name.to_string(),
            pages: z80::parse_value(self, pages.trim(), line)?,
        })
    }

    /// `ALIGN [boundary[,fill]]` — pad to the next multiple of `boundary`.
    ///
    /// sjasmplus takes a **power of two** and refuses anything else outright
    /// (`align 3` is `Illegal align: 3`), defaults the boundary to 4 when the
    /// operand is omitted, and fills with zero unless told otherwise. All three
    /// are probe-pinned against 1.21.0.
    /// `SAVEBIN "file",start[,length]`.
    ///
    /// The name is quoted, the start is required — SjASMPlus 1.21.0 answers
    /// "[SAVEBIN] Syntax error. No parameters" without it — and the length is
    /// optional. The span is resolved against the finished image rather than
    /// here, so both expressions travel as they were written.
    fn parse_savebin(
        &self,
        args: &str,
        line: usize,
        consts: &BTreeMap<String, i64>,
    ) -> Result<Operation, AsmError> {
        let _ = consts;
        let mut parts = crate::dialects::mos6502::split_top_level(args.trim(), ',');
        if parts.is_empty() {
            return Err(AsmError::new(line, "[SAVEBIN] Syntax error. No parameters"));
        }
        let name = parts.remove(0);
        let name = name
            .trim()
            .strip_prefix('"')
            .and_then(|t| t.strip_suffix('"'))
            .ok_or_else(|| AsmError::new(line, "`SAVEBIN` needs a quoted file name"))?
            .to_string();
        let Some(start) = parts.first() else {
            return Err(AsmError::new(line, "[SAVEBIN] Syntax error. No parameters"));
        };
        let start = z80::parse_value(self, start.trim(), line)?;
        let length = match parts.get(1) {
            Some(e) => Some(z80::parse_value(self, e.trim(), line)?),
            None => None,
        };
        Ok(Operation::SaveRaw {
            name,
            start,
            length,
        })
    }

    /// `SAVETAP "file",CODE|BASIC,"name",start,length`.
    ///
    /// The kind and the name are what a tape's header carries beyond the
    /// bytes. SjASMPlus also takes forms that name neither and save the whole
    /// of a device's memory; those are refused here for the reason `SAVEBIN`
    /// refuses a span outside the image (asm198x#318).
    fn parse_savetap(&self, args: &str, line: usize) -> Result<Operation, AsmError> {
        let parts = crate::dialects::mos6502::split_top_level(args.trim(), ',');
        let quoted = |p: &str| {
            p.trim()
                .strip_prefix('"')
                .and_then(|t| t.strip_suffix('"'))
                .map(str::to_string)
        };
        let [file, kind, name, start, length] = parts.as_slice() else {
            return Err(AsmError::new(
                line,
                "`SAVETAP` needs `\"file\",CODE|BASIC,\"name\",start,length` here — the \
                 forms that name no kind save a whole device's memory, which asm198x \
                 has no record of yet, so the source is valid and the gap is ours",
            ));
        };
        let file = quoted(file)
            .ok_or_else(|| AsmError::new(line, "`SAVETAP` needs a quoted file name"))?;
        let kind = match kind.trim().to_ascii_uppercase().as_str() {
            "CODE" => crate::engine::TapeKind::Code,
            "BASIC" => crate::engine::TapeKind::Program,
            other => {
                return Err(AsmError::new(
                    line,
                    format!("`{other}` is not a tape block kind asm198x writes (CODE, BASIC)"),
                ));
            }
        };
        let name = quoted(name)
            .ok_or_else(|| AsmError::new(line, "`SAVETAP` needs a quoted block name"))?;
        Ok(Operation::SaveTape {
            file,
            kind,
            name,
            start: z80::parse_value(self, start.trim(), line)?,
            length: z80::parse_value(self, length.trim(), line)?,
        })
    }

    fn parse_align(
        &self,
        args: &str,
        line: usize,
        consts: &BTreeMap<String, i64>,
    ) -> Result<Option<Operation>, AsmError> {
        let parts: Vec<&str> = args.trim().split(',').map(str::trim).collect();
        let modulus = match parts.first() {
            None | Some(&"") => 4,
            Some(n) => z80::literal(&z80::parse_value(self, n, line)?, consts, line)?,
        };
        if modulus < 1 || modulus & (modulus - 1) != 0 {
            return Err(AsmError::new(line, format!("Illegal align: {modulus}")));
        }
        let fill = match parts.get(1) {
            Some(f) => {
                let v = z80::literal(&z80::parse_value(self, f, line)?, consts, line)?;
                u8::try_from(v & 0xFF).expect("masked")
            }
            None => 0,
        };
        Ok(Some(Operation::AlignTo {
            modulus,
            fill: vec![fill],
        }))
    }
}

impl Z80Syntax for SjasmplusSyntax {
    fn lexical_option(
        &self,
        word: &str,
        args: &str,
        line: usize,
    ) -> Result<Option<z80::LexicalOption>, AsmError> {
        if !undot(word).eq_ignore_ascii_case("opt") {
            return Ok(None);
        }
        let option = match args.trim() {
            "--syntax=abfw" => z80::LexicalOption::SyntaxAbfw,
            "--zxnext" => z80::LexicalOption::ZxNext { cspect: false },
            "--zxnext=cspect" => z80::LexicalOption::ZxNext { cspect: true },
            other => {
                return Err(AsmError::new(
                    line,
                    format!(
                        "`OPT {other}` is valid SjASMPlus option state that asm198x has not implemented"
                    ),
                ));
            }
        };
        Ok(Some(option))
    }

    /// Comparisons in an expression, probed against the binary — see
    /// `docs/comparison-operators.md`. Both answer `$FF` for true.
    fn compare(&self) -> crate::dialects::mos6502::Compare {
        crate::dialects::mos6502::Compare {
            eq: true,
            eq_eq: true,
            ne_angle: false,
            ne_bang: true,
            relational: true,
            ordered_eq: true,
            minus_one: true,
        }
    }

    /// sjasmplus is the dialect the shared keyword vocabulary was measured
    /// against, so its adoption is the free functions unchanged.
    fn cond_keyword(&self, word: &str) -> Option<z80::CondKw> {
        z80::cond_keyword(word)
    }

    fn repeat_keyword(&self, word: &str) -> Option<z80::RepeatKw> {
        z80::repeat_keyword(undot(word))
    }

    fn module_keyword(&self, word: &str) -> Option<z80::ModuleKw> {
        z80::module_keyword(undot(word))
    }

    fn struct_keyword(&self, word: &str) -> Option<z80::StructKw> {
        z80::struct_keyword(undot(word))
    }

    /// The formatter must copy a macro definition rather than re-lay it out.
    /// It survived without this while every spelling was indented — the
    /// definition simply looked like an unrecognised line and came through
    /// unchanged. The name-first spelling (#205) puts the name in the *label*
    /// column, where the formatter peels it onto a line of its own and the
    /// definition stops being one.
    fn macro_line(&self, line: &str, known: &dyn Fn(&str) -> bool) -> macros::MacroLine {
        macros::macro_line(self, line, known)
    }

    /// sjasmplus scopes names under the open `MODULE`s (#93's third item).
    fn scopes_modules(&self) -> bool {
        true
    }

    /// `add (hl)` is `add a,(hl)` (#533; probed with and without `abfw`).
    fn implicit_accumulator(&self) -> bool {
        true
    }

    /// Column 0 is the label column, full stop (#551; probed: `db 1` there
    /// binds `db` and refuses `1`, `.end nop` binds the local `.end`).
    fn column_zero_is_a_label(&self) -> bool {
        true
    }

    fn temporary_labels(&self) -> bool {
        true
    }

    /// sjasmplus takes `:` as a statement separator as well as a label
    /// terminator (#98) — ` ld a,1 : ld b,2` is two instructions, and it is
    /// how hand-written Spectrum source is often laid out.
    fn splits_on_colon(&self) -> bool {
        true
    }

    /// sjasmplus resolves a condition against a symbol defined later in the
    /// file, across its three passes (#99).
    fn resolves_forward_conditions(&self) -> bool {
        true
    }

    fn is_define_word(&self, word: &str) -> bool {
        z80::is_define_word(undot(word))
    }

    fn is_equ_word(&self, word: &str) -> bool {
        undot(word).eq_ignore_ascii_case("equ")
    }

    fn constant_sources(&self) -> &'static str {
        "a value defined with `equ` or `DEFINE` above"
    }

    fn strip_comment<'a>(&self, line: &'a str) -> &'a str {
        // The earlier of `;` and `//` starts the comment.
        let semi = line.find(';');
        let slashes = line.find("//");
        let cut = match (semi, slashes) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        };
        cut.map_or(line, |i| &line[..i])
    }

    /// sjasmplus has the `^` bitwise-XOR operator (pasmo does not).
    fn has_xor_operator(&self) -> bool {
        true
    }

    /// sjasmplus adds `byte` as a spelling of `db` (pasmo has neither), plus
    /// `include` (U2) and `incbin` (U3) — listed here so a column-0 spelling
    /// reads as an operation, not a label; the walk intercepts both before
    /// directive parsing.
    fn is_directive(&self, word: &str) -> bool {
        let word = undot(word);
        word.eq_ignore_ascii_case("byte")
            || word.eq_ignore_ascii_case("align")
            || word.eq_ignore_ascii_case("device")
            || word.eq_ignore_ascii_case("slot")
            || word.eq_ignore_ascii_case("page")
            || word.eq_ignore_ascii_case("assert")
            || word.eq_ignore_ascii_case("display")
            || word.eq_ignore_ascii_case("opt")
            || word.eq_ignore_ascii_case("sldopt")
            || word.eq_ignore_ascii_case("savebin")
            || word.eq_ignore_ascii_case("savetap")
            || word.eq_ignore_ascii_case("savecpr")
            // The data directives beyond the shared set — see the
            // `sjasmplus-data` declaration.
            || SJASMPLUS_DATA.iter().any(|d| word.eq_ignore_ascii_case(d))
            || self.is_include(word)
            || self.is_incbin(word)
            || z80::is_common_directive(word)
    }

    fn is_end_word(&self, word: &str) -> bool {
        undot(word).eq_ignore_ascii_case("end")
    }

    /// sjasmplus's include directive (language-surface U2), walk-handled.
    fn is_include(&self, word: &str) -> bool {
        undot(word).eq_ignore_ascii_case("include")
    }

    /// sjasmplus's binary-inclusion directive (language-surface U3),
    /// walk-handled like `include`.
    fn is_incbin(&self, word: &str) -> bool {
        undot(word).eq_ignore_ascii_case("incbin")
    }

    fn own_directives(&self) -> &'static [crate::directives::Directive] {
        DIRECTIVES
    }

    /// sjasmplus's `INCBIN "file"[,offset[,length]]` takes the full tail,
    /// including the probe-pinned negative from-the-end forms.
    fn incbin_offset_length(&self) -> bool {
        true
    }

    /// sjasmplus accepts `<file>` for the incbin name (as its INCLUDE does).
    fn incbin_angle_quotes(&self) -> bool {
        true
    }

    /// `ALIGN` is sjasmplus's own; `byte` is `db`; everything else is the
    /// shared common set.
    /// `DS`, `DEFS` and `BLOCK` are one directive to sjasmplus —
    /// `count[, fill]`, the count resolved across passes (#528).
    fn is_reserve(&self, word: &str) -> bool {
        let word = undot(word);
        word.eq_ignore_ascii_case("block") || z80::is_common_reserve(word)
    }

    fn parse_directive(
        &self,
        word: &str,
        args: &str,
        line: usize,
        consts: &BTreeMap<String, i64>,
    ) -> Result<Option<Operation>, AsmError> {
        let word = undot(word);
        if word.eq_ignore_ascii_case("align") {
            return self.parse_align(args, line, consts);
        }
        if word.eq_ignore_ascii_case("sldopt") {
            let (kind, keywords) = crate::dialects::mos6502::split_first_word(args.trim());
            // The directive word is case-insensitive, but SjASMPlus 1.21.0's
            // only type is the case-sensitive spelling `COMMENT`.
            if kind != "COMMENT" {
                return Err(AsmError::new(
                    line,
                    "[SLDOPT] Syntax error in <type> (valid is only COMMENT)",
                ));
            }
            let keywords: Vec<&str> = keywords.split(',').map(str::trim).collect();
            if keywords.is_empty() || keywords.iter().any(|keyword| keyword.is_empty()) {
                return Err(AsmError::new(
                    line,
                    "`SLDOPT COMMENT` needs a comma-separated keyword list",
                ));
            }
            // Native probes show this changes only `.sld` K records and never
            // the assembled image. Asm198x writes Debug198x rather than SLD,
            // and the pinned project contains none of the selected comments.
            return Ok(None);
        }
        if word.eq_ignore_ascii_case("device") {
            let name = args.trim().to_ascii_uppercase();
            if name == "NONE" {
                return Ok(Some(Operation::Device(None)));
            }
            let &(name, pages, slots, slot_size) = DEVICES
                .iter()
                .find(|(device, _, _, _)| *device == name)
                .ok_or_else(|| {
                    AsmError::new(
                        line,
                        format!("`{}` is not a device sjasmplus has", args.trim()),
                    )
                })?;
            return Ok(Some(Operation::Device(Some(crate::engine::DeviceSpec {
                name: name.to_string(),
                pages,
                slots,
                slot_size,
            }))));
        }
        if word.eq_ignore_ascii_case("slot") {
            return Ok(Some(Operation::DeviceSlot(z80::parse_value(
                self,
                args.trim(),
                line,
            )?)));
        }
        if word.eq_ignore_ascii_case("display") {
            // sjasmplus prints values as `0x0005`, and prefixes the line with
            // `> `; the prefix is its own chrome, so only the text is kept.
            let mut text = String::new();
            for part in args.split(',') {
                let part = part.trim();
                match part.strip_prefix('"').and_then(|t| t.strip_suffix('"')) {
                    Some(lit) => text.push_str(lit),
                    None => {
                        match z80::literal(&z80::parse_value(self, part, line)?, consts, line) {
                            Ok(v) => text.push_str(&format!("0x{v:04X}")),
                            Err(_) => text.push_str(part),
                        }
                    }
                }
            }
            return Ok(Some(Operation::Diagnose {
                severity: crate::engine::DiagSeverity::Note,
                message: text,
            }));
        }
        if word.eq_ignore_ascii_case("savebin") {
            return self.parse_savebin(args, line, consts).map(Some);
        }
        if word.eq_ignore_ascii_case("savetap") {
            return self.parse_savetap(args, line).map(Some);
        }
        if word.eq_ignore_ascii_case("assert") {
            // sjasmplus takes the whole tail as the expression and echoes it
            // back in the message — `ASSERT 0, "x"` reports `0, "x"` rather
            // than treating the tail as a message operand (probe-pinned).
            return Ok(Some(Operation::Assert {
                cond: z80::parse_value(self, args, line)?,
                fatal: true,
                message: format!("[ASSERT] Assertion failed: {}", args.trim()),
            }));
        }
        if word.eq_ignore_ascii_case("page") {
            return Ok(Some(Operation::DevicePage(z80::parse_value(
                self,
                args.trim(),
                line,
            )?)));
        }
        if word.eq_ignore_ascii_case("savecpr") {
            return self.parse_savecpr(args, line).map(Some);
        }
        // The data directives sjasmplus has and the shared Z80 set does not.
        // Every shape here was read off v1.21.0 rather than assumed.
        let lower = word.to_ascii_lowercase();
        match lower.as_str() {
            "word" => return Ok(Some(Operation::Words(flat(data_items(args, line)?)))),
            "dword" | "dd" | "defd" => {
                return Ok(Some(wide(flat(data_items(args, line)?), 4)));
            }
            "d24" => return Ok(Some(wide(flat(data_items(args, line)?), 3))),
            // `dz` terminates with a zero; `dc` sets bit 7 on the last
            // character of each string and leaves numbers alone.
            "dz" => {
                let mut bytes = flat(data_items(args, line)?);
                bytes.push(Expr::Num(0));
                return Ok(Some(Operation::Bytes(bytes)));
            }
            "dc" => return Ok(Some(Operation::Bytes(marked(data_items(args, line)?)))),
            "dh" | "hex" | "defh" => {
                return Ok(Some(Operation::Bytes(hex_bytes(args, line)?)));
            }
            "dg" | "defg" => return Ok(Some(Operation::Bytes(graphic_bytes(args, line)?))),
            // `abyte n list` adds `n` to every byte; `abytec` marks the last
            // character of each string as `dc` does, and `abytez` appends a
            // zero as `dz` does.
            "abyte" | "abytec" | "abytez" => {
                let (offset, rest) = crate::dialects::mos6502::split_first_word(args.trim());
                let offset = z80::parse_value(self, offset, line)?;
                let items = data_items(rest, line)?;
                let mut bytes = if lower == "abytec" {
                    marked(items)
                } else {
                    flat(items)
                };
                bytes = bytes
                    .into_iter()
                    .map(|b| {
                        Expr::Bin(
                            crate::engine::BinOp::Add,
                            Box::new(b),
                            Box::new(offset.clone()),
                        )
                    })
                    .collect();
                if lower == "abytez" {
                    bytes.push(Expr::Num(0));
                }
                return Ok(Some(Operation::Bytes(bytes)));
            }
            _ => {}
        }
        let word = if word.eq_ignore_ascii_case("byte") {
            "db"
        } else {
            word
        };
        z80::common_directive(self, word, args, line, consts)
    }

    /// Nothing is rewritten before the parse: macros and device state are live
    /// in the shared textual walk below.
    fn expand_source(
        &self,
        source: &str,
    ) -> Result<Option<(String, Vec<macros::LineOrigin>)>, AsmError> {
        let _ = source;
        Ok(None)
    }

    /// Macros are live (#557): a definition enters the namespace when the
    /// walk reaches it and an invocation expands against what is defined by
    /// then, across every file in include order. Probed: a definition in an
    /// included file is visible afterwards in its includer and in later
    /// sibling includes, one in the includer is visible inside a later
    /// include, one in a nested include flows all the way out, and an
    /// invocation above the definition — or above the `INCLUDE` that holds
    /// it, or of a definition in an untaken `IF` — is `Unrecognized
    /// instruction`. Until this, each file expanded on its own (#93).
    fn expand_live(
        &self,
        source: &str,
        file: crate::span::FileId,
        line: usize,
        state: &mut macros::MacroState,
    ) -> Result<Option<macros::Expanded>, AsmError> {
        macros::expand_at(&SjasmplusSyntax, source, file, line, state).map(Some)
    }

    fn scopes_locals(&self) -> bool {
        true
    }

    /// sjasmplus numbers: hex (`$`/`0x`/`#` prefix, `h` suffix), binary (`%`/`0b`
    /// prefix, `b` suffix), `'c'` char, decimal.
    fn parse_number(&self, tok: &str, line: usize) -> Result<i64, AsmError> {
        let t = tok.trim();
        let bad = || AsmError::new(line, format!("invalid number `{tok}`"));

        if t.starts_with('\'') && t.ends_with('\'') && t.chars().count() == 3 {
            return t.chars().nth(1).map(|c| c as i64).ok_or_else(bad);
        }
        // Hex: $, 0x, or # prefix, or h suffix.
        if let Some(hex) = t
            .strip_prefix('$')
            .or_else(|| t.strip_prefix("0x"))
            .or_else(|| t.strip_prefix("0X"))
            .or_else(|| t.strip_prefix('#'))
        {
            return i64::from_str_radix(hex, 16).map_err(|_| bad());
        }
        if let Some(hex) = t.strip_suffix(['h', 'H'])
            && let Ok(v) = i64::from_str_radix(hex, 16)
        {
            return Ok(v);
        }
        // Binary: % or 0b prefix, or b suffix.
        if let Some(bin) = t
            .strip_prefix('%')
            .or_else(|| t.strip_prefix("0b"))
            .or_else(|| t.strip_prefix("0B"))
        {
            return i64::from_str_radix(bin, 2).map_err(|_| bad());
        }
        if let Some(bin) = t.strip_suffix(['b', 'B'])
            && let Ok(v) = i64::from_str_radix(bin, 2)
        {
            return Ok(v);
        }
        t.parse::<i64>().map_err(|_| bad())
    }
}

/// One item of a data list, and whether the source wrote it as a string.
///
/// `dc` and `abytec` set bit 7 on the **last character of each string** and
/// leave a number alone (`dc "ab",3,"cd"` is `61 E2 03 63 E4`), so the item
/// boundaries have to survive the parse rather than being flattened away.
struct DataItem {
    from_string: bool,
    bytes: Vec<Expr>,
}

fn data_items(rest: &str, line: usize) -> Result<Vec<DataItem>, AsmError> {
    let rest = rest.trim();
    if rest.is_empty() {
        return Err(AsmError::new(line, "directive needs at least one value"));
    }
    let mut out = Vec::new();
    for piece in z80::split_data_items(rest) {
        if let Some(text) = z80::string_literal(piece) {
            out.push(DataItem {
                from_string: true,
                bytes: text.chars().map(|c| Expr::Num(c as i64)).collect(),
            });
        } else {
            out.push(DataItem {
                from_string: false,
                bytes: vec![z80::parse_value(&SjasmplusSyntax, piece, line)?],
            });
        }
    }
    Ok(out)
}

/// `dh`/`hex`/`defh` — hex digit pairs, with or without commas between them:
/// `dh 11,22` and `hex 3344` are both `11 22` / `33 44`.
fn hex_bytes(rest: &str, line: usize) -> Result<Vec<Expr>, AsmError> {
    let digits: String = rest
        .chars()
        .filter(|c| !c.is_whitespace() && *c != ',')
        .collect();
    if digits.is_empty()
        || !digits.len().is_multiple_of(2)
        || !digits.chars().all(|c| c.is_ascii_hexdigit())
    {
        return Err(AsmError::new(
            line,
            "hex data takes pairs of hex digits, optionally separated by commas",
        ));
    }
    digits
        .as_bytes()
        .chunks(2)
        .map(|pair| {
            let text = std::str::from_utf8(pair).expect("ascii");
            u8::from_str_radix(text, 16)
                .map(|b| Expr::Num(i64::from(b)))
                .map_err(|_| AsmError::new(line, format!("`{text}` is not a pair of hex digits")))
        })
        .collect()
}

/// `dg`/`defg` — one bit per character, eight to a byte. `#` is a one; `-`,
/// `.` and `_` are zeros, which is how sjasmplus spells a blank pixel.
fn graphic_bytes(rest: &str, line: usize) -> Result<Vec<Expr>, AsmError> {
    let bits: String = rest.chars().filter(|c| !c.is_whitespace()).collect();
    if bits.is_empty() || !bits.len().is_multiple_of(8) {
        return Err(AsmError::new(
            line,
            "graphic data takes a multiple of eight characters, one per bit",
        ));
    }
    bits.as_bytes()
        .chunks(8)
        .map(|byte| {
            let mut value = 0i64;
            for &c in byte {
                let bit = match c {
                    b'#' | b'1' | b'x' | b'X' | b'*' => 1,
                    b'-' | b'.' | b'_' | b'0' => 0,
                    other => {
                        return Err(AsmError::new(
                            line,
                            format!("`{}` is not a graphic character", other as char),
                        ));
                    }
                };
                value = (value << 1) | bit;
            }
            Ok(Expr::Num(value))
        })
        .collect()
}

/// Every item's bytes, in order, with the item boundaries dropped.
fn flat(items: Vec<DataItem>) -> Vec<Expr> {
    items.into_iter().flat_map(|i| i.bytes).collect()
}

/// The same, with bit 7 set on the last byte of every item that came from a
/// string — `dc` and `abytec`'s terminator convention.
fn marked(items: Vec<DataItem>) -> Vec<Expr> {
    let mut out = Vec::new();
    for item in items {
        let last = item.bytes.len().saturating_sub(1);
        for (i, byte) in item.bytes.into_iter().enumerate() {
            if item.from_string && i == last {
                out.push(Expr::Bin(
                    crate::engine::BinOp::Or,
                    Box::new(byte),
                    Box::new(Expr::Num(0x80)),
                ));
            } else {
                out.push(byte);
            }
        }
    }
    out
}

/// Lay a value down over `width` bytes, little-endian, as computed pieces so a
/// symbol still resolves in pass two.
fn wide(values: Vec<Expr>, width: u8) -> Operation {
    Operation::Encoded(
        values
            .into_iter()
            .map(|expr| Piece::Val {
                expr,
                bytes: width,
                rel: false,
                signed: false,
            })
            .collect(),
    )
}

// ---------------------------------------------------------------------------
// Macros (#93)
//
// The mechanics live in [`crate::dialects::macros`]; this is sjasmplus's
// grammar. Every rule below was measured against sjasmplus 1.21.0 rather than
// read from its manual, and two of them are not what a reasonable person would
// guess:
//
//   * the `MACRO`/`ENDM` **keyword** is case-insensitive, but a macro **name**
//     is case-sensitive — defining `mac` and calling `MAC` is an error;
//   * substitution respects word boundaries (a parameter `v` leaves the symbol
//     `val` alone) and does **not** reach inside string literals, so
//     `db "v"` emits the letter, not the argument.
//
// Substitution is textual and happens before expression evaluation, which is
// why `val*2` with `val = 5` assembles to `ld a,10`.
// ---------------------------------------------------------------------------

/// Strip sjasmplus's optional leading `.` from a directive word.
///
/// Every directive it has takes one — `.db`, `.org`, `.module`, `.equ` — and
/// the conditionals already stripped it (#67). This is the same rule for the
/// rest of the surface, and it belongs to *this* dialect: pasmo shares the Z80
/// core and reads `.db` as an ordinary label.
///
/// Stripping before the case test is deliberate, and copies what the
/// conditionals do: `.Db` stays as unacceptable as `Db` where a spelling is
/// case-sensitive.
fn undot(word: &str) -> &str {
    word.strip_prefix('.').unwrap_or(word)
}

/// Split a macro header's tail into its leading word and the rest — the name
/// and its parameters in the keyword-first form, the keyword and the
/// parameters in the name-first one.
fn split_macro_name(rest: &str) -> (&str, &str) {
    let rest = rest.trim();
    match rest.split_once(char::is_whitespace) {
        Some((word, tail)) => (word.trim(), tail.trim()),
        None => (rest, ""),
    }
}

impl macros::MacroSyntax for SjasmplusSyntax {
    /// Two spellings, both the reference's (#205):
    ///
    /// ```text
    ///     MACRO name [p1[, p2]...]     indented — the keyword leads
    /// name[:] MACRO [p1[, p2]...]      column 0 — the name leads
    /// ```
    ///
    /// The keyword matches case-insensitively; the name is kept as written and
    /// stays case-sensitive at the call site.
    ///
    /// **Indentation decides which is which, and a line in the wrong column is
    /// not a definition at all.** At column 0 the reference reads `MACRO` as a
    /// label and the name after it as an instruction (probe n9); indented,
    /// `mk MACRO a` is an unrecognised instruction rather than a definition
    /// (probe n8). Both are errors there, and returning `None` makes them
    /// errors here.
    ///
    /// Parameters are comma-separated in both forms, and a comma may not stand
    /// between the keyword and the name in either (probes n10, n11) — which
    /// the name check below rejects, where the previous grammar allowed it.
    fn header(&self, line: &str) -> Option<(String, Vec<String>)> {
        let text = macros::without_comment(line);
        let indented = text.starts_with(char::is_whitespace);
        let (first, rest) = text.trim().split_once(char::is_whitespace)?;
        let (name, tail) = if undot(first).eq_ignore_ascii_case("macro") {
            if !indented {
                return None;
            }
            split_macro_name(rest)
        } else {
            if indented {
                return None;
            }
            let (kw, tail) = split_macro_name(rest);
            if !undot(kw).eq_ignore_ascii_case("macro") {
                return None;
            }
            (first.trim_end_matches(':'), tail)
        };
        if name.is_empty() || name.contains(',') {
            return None;
        }
        let params = tail
            .split(',')
            .map(|p| p.trim().to_string())
            .filter(|p| !p.is_empty())
            .collect();
        Some((name.to_string(), params))
    }

    /// `ENDM`, alone on its line.
    fn is_end(&self, line: &str) -> bool {
        undot(macros::without_comment(line).trim()).eq_ignore_ascii_case("endm")
    }

    fn end_keyword(&self) -> &'static str {
        "endm"
    }

    /// sjasmplus rejects any mismatch, and says which way round it went.
    fn fit_arguments(
        &self,
        name: &str,
        params: &[String],
        args: Vec<String>,
    ) -> Result<Vec<String>, String> {
        match args.len().cmp(&params.len()) {
            std::cmp::Ordering::Greater => Err(format!("too many arguments for macro `{name}`")),
            std::cmp::Ordering::Less => Err(format!("not enough arguments for macro `{name}`")),
            std::cmp::Ordering::Equal => Ok(args),
        }
    }

    /// The dot-prefixed labels a macro body **defines**.
    ///
    /// These scope to the expansion rather than to the file, which is what lets
    /// a macro containing a loop be invoked more than once — most of what macros
    /// are for. A *plain* label in the same position stays global and collides
    /// on the second invocation; the reference reports `Duplicate label` there
    /// and so do we, which is why only the dotted ones are renamed.
    ///
    /// Only names the body defines are renamed, so a body referring to a local
    /// defined outside it still refers to that one.
    fn locals(&self, body: &[String]) -> Vec<String> {
        let mut names = Vec::new();
        for line in body {
            let text = macros::without_comment(line);
            if text.starts_with(char::is_whitespace) {
                continue;
            }
            let token = text.split_whitespace().next().unwrap_or("");
            let name = token.trim_end_matches(':');
            if name.starts_with('.') && name.len() > 1 && !names.iter().any(|n| n == name) {
                names.push(name.to_string());
            }
        }
        names
    }
}

#[cfg(test)]
mod tests {
    use crate::{assemble_sjasmplus as asm, assemble_sjasmplus_files, source::MemoryLoader};

    /// #559: decimal labels are repeatable, and a jump target reaches the
    /// nearest definition in the requested direction.
    #[test]
    fn temporary_labels_resolve_forward_and_backward() {
        let result = asm("\tjr nc,1F\n\tnop\n1\tnop\n\tjr 1B\n1:\tnop\n\tdjnz 1b\n")
            .expect("temporary labels");
        assert_eq!(
            result.bytes,
            vec![0x30, 0x01, 0x00, 0x00, 0x18, 0xFD, 0x00, 0x10, 0xFD]
        );
        assert!(
            result
                .symbols
                .keys()
                .all(|name| !name.contains("temporary")),
            "temporary implementation names must not leak"
        );

        let absolute = asm("\torg $100\n\tjp 12F\n\tcall 12f\n12\tnop\n")
            .expect("multi-digit absolute targets");
        assert_eq!(
            absolute.bytes,
            vec![0xC3, 0x06, 0x01, 0xCD, 0x06, 0x01, 0x00]
        );
    }

    /// The `F`/`B` spelling is contextual, not a new general expression
    /// token: `1B` keeps its binary-literal meaning outside branch targets,
    /// while `1F` keeps the reference's invalid-digit refusal.
    #[test]
    fn temporary_reference_spelling_is_jump_target_only() {
        assert_eq!(
            asm("\tdw 1B\n\tld hl,1B\n\tld a,1B\n\tdw 1B+1\n")
                .expect("binary literals")
                .bytes,
            vec![0x01, 0x00, 0x21, 0x01, 0x00, 0x3E, 0x01, 0x02, 0x00]
        );
        for source in ["\tdw 1F\n", "\tld hl,1F+0\n", "\tld hl,(1F)\n"] {
            let error = asm(source).expect_err("F is not a general expression suffix");
            assert!(error.to_string().contains("invalid number `1F`"), "{error}");
        }
        let error = asm("1\tequ 5\n").expect_err("number EQU");
        assert!(error.to_string().contains("address labels only"), "{error}");
        for source in ["\tjr 1F\n", "\tjr 1B\n", "\tjp 2b\n"] {
            let error = asm(source).expect_err("missing temporary label");
            assert!(
                error.to_string().contains("Temporary label not found"),
                "{error}"
            );
        }
    }

    /// A temporary definition is not a global-label event: it neither opens a
    /// local scope nor disturbs the preceding global anchor.
    #[test]
    fn a_temporary_label_does_not_reanchor_locals() {
        let result =
            asm("glob\tnop\n1\tnop\n.loc\tnop\n\tjr glob.loc\n").expect("local stays below glob");
        assert_eq!(result.symbols.get("glob.loc"), Some(&2));
        assert_eq!(result.bytes, vec![0x00, 0x00, 0x00, 0x18, 0xFD]);
    }

    /// Includes and live macro expansions are one textual label stream. Each
    /// expansion contributes a fresh definition, just as the reference does.
    #[test]
    fn temporary_labels_span_includes_and_macro_expansions() {
        let loader = MemoryLoader::new().text("label.asm", "1\tnop\n");
        let included = assemble_sjasmplus_files(
            "\tjr 1F\n\tinclude \"label.asm\"\n\tjr 1B\n",
            "main.asm",
            &loader,
        )
        .expect("include shares temporary labels");
        assert_eq!(included.bytes, vec![0x18, 0x00, 0x00, 0x18, 0xFD]);

        let expanded = asm("\tMACRO M\n\tjr 1F\n1\tnop\n\tENDM\n\tM\n\tM\n\tjr 1B\n")
            .expect("each macro expansion has a fresh temporary label");
        assert_eq!(
            expanded.bytes,
            vec![0x18, 0x00, 0x00, 0x18, 0x00, 0x00, 0x18, 0xFD]
        );
    }

    /// #554: a live `END` stops every enclosing source construct. A skipped
    /// one does nothing, while the dotted spelling and a macro-expanded one
    /// have the same control effect as the plain directive.
    #[test]
    fn end_stops_the_whole_live_assembly() {
        assert_eq!(
            asm("\tnop\n\tend\n\tnop\n").expect("plain").bytes,
            vec![0x00]
        );
        assert_eq!(
            asm("\tnop\n\t.end\n\tnop\n").expect("dotted").bytes,
            vec![0x00]
        );
        assert_eq!(
            asm("\tIF 0\n\tEND\n\tENDIF\n\tnop\n")
                .expect("skipped END")
                .bytes,
            vec![0x00]
        );
        assert_eq!(
            asm("\tMACRO STOP\n\tEND\n\tENDM\n\tnop\n\tSTOP\n\tnop\n")
                .expect("macro END")
                .bytes,
            vec![0x00]
        );
    }

    /// #554: an `END` reached inside an include stops the includer too; it is
    /// one textual assembly, not a return from the included file.
    #[test]
    fn end_in_an_include_stops_the_includer() {
        let loader = MemoryLoader::new().text("stop.inc", "\tnop\n\tend\n\tnop\n");
        let result = assemble_sjasmplus_files(
            "\tnop\n\tinclude \"stop.inc\"\n\tnop\n",
            "main.asm",
            &loader,
        )
        .expect("assembles");
        assert_eq!(result.bytes, vec![0x00, 0x00]);
    }

    /// #477: the accepted STRUCT shapes are pinned byte-for-byte by the
    /// differential probes; these are the refusals, which a byte comparison
    /// cannot record.
    #[test]
    fn struct_refusals_match_the_reference() {
        // Probed: `[ENDS] End structure without structure`.
        let e = asm("\tENDS\n").expect_err("stray closer");
        assert!(e.to_string().contains("without"), "{e}");
        // Probed: `[STRUCT] Unexpected end of structure`.
        let e = asm("\tSTRUCT W\nf BYTE 0\n").expect_err("never closed");
        assert!(e.to_string().contains("never closed"), "{e}");
        // Probed: `[STRUCT] Unexpected: ld a,1` — a statement is not a member.
        let e = asm("\tSTRUCT Q\nf BYTE 0\n\tld a,1\n\tENDS\n").expect_err("not a member");
        assert!(e.to_string().contains("not a member"), "{e}");
        // Probed: `Duplicate label: D.f` — the exported constants collide the
        // way any duplicate label does.
        let e = asm("\tSTRUCT D\nf BYTE 0\nf BYTE 0\n\tENDS\n\tdb D\n").expect_err("duplicate");
        assert!(e.to_string().contains("duplicate"), "{e}");
    }

    /// #477: a structure's definition emits nothing — bytes begin with the
    /// first real statement after `ENDS`.
    #[test]
    fn a_struct_definition_emits_no_bytes() {
        let r = asm("\tSTRUCT S\nf BYTE 0\ng WORD 0\n\tENDS\n\tdb S\n").expect("assembles");
        assert_eq!(r.bytes, vec![3]);
    }

    // #528: a `DS` count reaches forward the way a condition does — the
    // reference resolves it across its passes, silently. Every shape below
    // was probed against 1.21.0 before landing.

    /// `DS COUNT * 2` above `COUNT EQU 3` reserves six, and says nothing:
    /// unlike a condition, the reference raises no `fwdref` advisory here.
    #[test]
    fn a_ds_count_reaches_a_later_equ() {
        let r = asm("buf\tDS COUNT * 2\n\tnop\nCOUNT\tEQU 3\n").expect("assembles");
        assert_eq!(r.bytes, vec![0, 0, 0, 0, 0, 0, 0]);
        assert!(r.warnings.is_empty(), "{:?}", r.warnings);
    }

    /// A count that moves the label it depends on never settles. Pass 1
    /// reads `later` as 0 and reserves one; each pass reserves one more; the
    /// reference stops at three and warns that the label still moved.
    #[test]
    fn a_ds_count_on_a_moving_label_stops_where_the_reference_stops() {
        let r = asm("\tDS later+1\nlater:\tnop\n").expect("assembles");
        assert_eq!(r.bytes, vec![0, 0, 0, 0]);
        assert_eq!(r.warnings.len(), 1, "{:?}", r.warnings);
        assert!(
            r.warnings[0]
                .message
                .contains("previous value 2 not equal 3")
        );
    }

    /// A count that swings negative in pass 2 (`3 - later` once `later` is
    /// 3) still ends where the reference ends: pass 3 reads the pass-2 value
    /// and reserves three again.
    #[test]
    fn a_ds_count_that_swings_between_passes_matches_pass_three() {
        let r = asm("\tDS 3-later\nlater:\tnop\n").expect("assembles");
        assert_eq!(r.bytes, vec![0, 0, 0, 0]);
        assert!(
            r.warnings[0]
                .message
                .contains("previous value 0 not equal 3")
        );
    }

    /// `DS`, `DEFS` and `BLOCK` are one directive: `count[, fill]`, and both
    /// the count and the fill may be defined later.
    #[test]
    fn ds_defs_and_block_take_a_fill_that_may_be_forward() {
        let r =
            asm("\tDS COUNT, $FF\n\tDEFS 2, FILL\n\tBLOCK 1\n\tnop\nCOUNT\tEQU 2\nFILL\tEQU $AA\n")
                .expect("assembles");
        assert_eq!(r.bytes, vec![0xFF, 0xFF, 0xAA, 0xAA, 0, 0]);
    }

    /// `$` in a count is the counter where it stands (`DS $+2` at the origin
    /// reserves two).
    #[test]
    fn a_ds_count_may_use_the_location_counter() {
        let r = asm("\tDS $+2\n\tnop\n").expect("assembles");
        assert_eq!(r.bytes, vec![0, 0, 0]);
    }

    /// A `STRUCT` member's `DS` reaches forward too: `x DS N` above
    /// `N EQU 4` sizes the structure at 4.
    #[test]
    fn a_struct_member_ds_reaches_a_later_equ() {
        let r = asm("\tSTRUCT Pt\nx\tDS N\n\tENDS\n\tDB Pt\nN\tEQU 4\n").expect("assembles");
        assert_eq!(r.bytes, vec![4]);
    }

    /// A symbol no pass defines is the reference's `Label not found` error
    /// on its last pass — for a count and for a condition alike, which
    /// until now read zero a third time and assembled.
    #[test]
    fn a_symbol_no_pass_defines_is_an_error_on_the_last_pass() {
        let e = asm("\tDS nothere\n\tnop\n").expect_err("refused");
        assert!(e.to_string().contains("label not found: `nothere`"), "{e}");
        let e = asm("\tIF nothere\n\tnop\n\tENDIF\n").expect_err("refused");
        assert!(e.to_string().contains("label not found: `nothere`"), "{e}");
    }

    /// The reference warns `Negative BLOCK?` and moves the counter backwards.
    /// This engine does not move sjasmplus's origin backwards, so the count
    /// is refused with that behaviour named rather than silently reserving
    /// nothing.
    #[test]
    fn a_negative_ds_count_is_refused_naming_the_reference() {
        let e = asm("\tDS -1\n\tnop\n").expect_err("refused");
        assert!(e.to_string().contains("Negative BLOCK?"), "{e}");
    }

    /// #533: a lone operand on `ADD`/`ADC`/`SBC` is `A,<operand>`, with or
    /// without `--syntax=abfw` (probed, SjASMPlus 1.21.0). The two-operand
    /// 16-bit forms are untouched — the rule is about one operand, not the
    /// first — and a lone 16-bit operand is still an error.
    #[test]
    fn a_lone_operand_on_add_adc_sbc_is_the_accumulator_form() {
        let src = "\tadd (hl)\n\tadd b\n\tadd 5\n\tadc c\n\tsbc (ix+1)\n\tadd a\n\tsbc a\n\tadc 200\n\tadd (iy-3)\n\tadd ixh\n";
        let want = vec![
            0x86, 0x80, 0xC6, 0x05, 0x89, 0xDD, 0x9E, 0x01, 0x87, 0x9F, 0xCE, 0xC8, 0xFD, 0x86,
            0xFD, 0xDD, 0x84,
        ];
        assert_eq!(asm(src).expect("implicit accumulator").bytes, want);
        assert_eq!(
            asm(&format!("\topt --syntax=abfw\n{src}"))
                .expect("under abfw too")
                .bytes,
            want
        );
        assert_eq!(
            asm("\tadd hl,de\n\tadc hl,de\n\tsbc hl,de\n\tadd ix,bc\n\tadd a,(hl)\n")
                .expect("two operands unchanged")
                .bytes,
            vec![0x19, 0xED, 0x5A, 0xED, 0x52, 0xDD, 0x09, 0x86]
        );
        assert!(
            asm("\tadd hl\n").is_err(),
            "`add hl` is `Comma expected` in the reference"
        );
    }

    /// #548: `label Name { v0, v1, … }` fills the members in declaration
    /// order, each at its own width; a slot left empty or off the end keeps
    /// the member's default (probed, SjASMPlus 1.21.0).
    #[test]
    fn a_struct_instance_takes_an_initialiser_list() {
        let hb = "\tSTRUCT Hitbox\nx0\tBYTE 0\nx1\tBYTE 0\ny0\tBYTE 1\ny1\tBYTE 0\n\tENDS\n";
        let r = asm(&format!(
            "{hb}a\tHitbox {{ $02, $0e, $00, $07 }}\nb\tHitbox\nc\tHitbox {{ $11, $22 }}\nd\tHitbox {{ , $33 }}\ne\tHitbox {{ }}\nf\tHitbox {{ $11, }}\n\tld a,(c.x1)\n"
        ))
        .expect("assembles");
        assert_eq!(
            r.bytes,
            vec![
                0x02, 0x0E, 0x00, 0x07, // a
                0x00, 0x00, 0x01, 0x00, // b: the defaults
                0x11, 0x22, 0x01, 0x00, // c: trailing members keep theirs
                0x00, 0x33, 0x01, 0x00, // d: an empty slot keeps its default
                0x00, 0x00, 0x01, 0x00, // e: `{ }` is the bare instance
                0x11, 0x00, 0x01, 0x00, // f: a trailing comma is an empty slot
                0x3A, 0x09, 0x00, // ld a,(c.x1): member labels bind as before
            ]
        );
        // A WORD member takes a word, little-endian; the instance name may
        // lead the line unindented as well as sit in the label column.
        let r = asm("\tSTRUCT P\nw\tWORD 0\nb\tBYTE 0\n\tENDS\ni P { $1234, $56 }\n")
            .expect("assembles");
        assert_eq!(r.bytes, vec![0x34, 0x12, 0x56]);
        // An unlabelled instance, indented, with the list.
        let r = asm(&format!("{hb}\tHitbox {{ $00, $02, $00, $06 }}\n")).expect("assembles");
        assert_eq!(r.bytes, vec![0x00, 0x02, 0x00, 0x06]);
        // The braces are optional: `Hitbox $2, $d, $0, $7` is the same list
        // (probed; invaders.asm in the SpecNext Invaders corpus spells it so).
        let r = asm(&format!(
            "{hb}g\tHitbox $2, $d, $0, $7\n\tHitbox $3, $d, $0, $7\nh\tHitbox , $33\n"
        ))
        .expect("assembles");
        assert_eq!(
            r.bytes,
            vec![
                0x02, 0x0D, 0x00, 0x07, 0x03, 0x0D, 0x00, 0x07, 0x00, 0x33, 0x01, 0x00
            ]
        );
    }

    /// #548: a value may be any expression, a forward label included; a
    /// `DS`/`BLOCK` member reserves but takes no slot (probed).
    #[test]
    fn a_struct_initialiser_value_is_an_expression_and_ds_takes_no_slot() {
        let r = asm("\tSTRUCT Hb\nx0\tBYTE 0\nw\tWORD 0\n\tENDS\nn\tequ 3\nc\tHb { n+1, later*2 }\nlater\tequ $1234\n")
            .expect("assembles");
        assert_eq!(r.bytes, vec![0x04, 0x68, 0x24]);
        let r = asm("\tSTRUCT Rec\na\tBYTE 1\npad\tDS 2\nb\tBYTE 2\n\tENDS\nc\tRec { $11, $22 }\n")
            .expect("assembles");
        assert_eq!(r.bytes, vec![0x11, 0x00, 0x00, 0x22]);
    }

    /// #548: more values than members is the reference's `closing } missing`
    /// / `too many arguments?`; a list that never closes is refused too.
    #[test]
    fn a_struct_initialiser_with_too_many_values_is_refused() {
        let hb = "\tSTRUCT Hb\nx0\tBYTE 0\ny0\tBYTE 1\n\tENDS\n";
        let e = asm(&format!("{hb}a\tHb {{ 1, 2, 3 }}\n")).expect_err("too many");
        assert!(e.to_string().contains("too many"), "{e}");
        let e = asm(&format!("{hb}a\tHb {{ 1, 2\n")).expect_err("unclosed");
        assert!(e.to_string().contains("closing }"), "{e}");
        let e = asm(&format!("{hb}a\tHb 1, 2, 3\n")).expect_err("too many, unbraced");
        assert!(e.to_string().contains("too many"), "{e}");
    }

    /// #552: a brace left open carries the list on to the lines that
    /// follow, until the one that closes it; the continuation lines are
    /// values, not statements, comments and blank lines aside. A line end
    /// ends a value where a comma would, but a comma is only taken on the
    /// line its value ends on (probed, SjASMPlus 1.21.0).
    #[test]
    fn a_struct_initialiser_list_runs_across_lines() {
        let hb = "\tSTRUCT Hitbox\nx0\tBYTE 0\nx1\tBYTE 0\ny0\tBYTE 1\ny1\tBYTE 0\n\tENDS\n";
        // The corpus shapes: a trailing comma, and one value per line with
        // the brace and its closer on lines of their own.
        let r = asm(&format!(
            "{hb}a\tHitbox {{ $11, $22, ; x0, x1\n\t$33, $44 }} ; y0, y1\n\tHitbox {{\n\t$55, ; one\n\n\t; a comment line\n\t$66,\n\t$77\n}}\n\tnop\n"
        ))
        .expect("assembles");
        assert_eq!(
            r.bytes,
            vec![0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x00, 0x00]
        );
        // A value ends at the line end whether or not a comma follows it,
        // and the reference reads an expression then an *optional* comma, so
        // two values on one line need no comma either.
        let r = asm(&format!("{hb}a\tHitbox {{ $11\n\t$22 $33,\n\t$44 }}\n")).expect("assembles");
        assert_eq!(r.bytes, vec![0x11, 0x22, 0x33, 0x44]);
        // But a comma is only taken on the line its value ends on: one that
        // opens the next line is an empty slot.
        let r = asm(&format!("{hb}a\tHitbox {{ $11\n\t, $22 }}\n")).expect("assembles");
        assert_eq!(r.bytes, vec![0x11, 0x00, 0x22, 0x00]);
        // Column 0 on a continuation line is a value, not a label.
        let r = asm(&format!("{hb}a\tHitbox {{ $11,\n$22, $33, $44 }}\n")).expect("assembles");
        assert_eq!(r.bytes, vec![0x11, 0x22, 0x33, 0x44]);
        // Without a brace the list ends with the line.
        let e = asm(&format!("{hb}a\tHitbox $11,\n\t$22, $33, $44\n")).expect_err("no brace");
        assert!(e.to_string().contains("unknown instruction"), "{e}");
        // A brace still open at the end of the source is refused.
        let e = asm(&format!("{hb}a\tHitbox {{ $11,\n\t$22,\n")).expect_err("unclosed");
        assert!(e.to_string().contains("closing }"), "{e}");
        // A DEFINE substitutes on a continuation line as it does anywhere.
        let r =
            asm(&format!("{hb}\tDEFINE V $42\na\tHitbox {{ $11,\n\tV }}\n")).expect("assembles");
        assert_eq!(r.bytes, vec![0x11, 0x42, 0x01, 0x00]);
    }

    /// #552: a nested `{ … }` at an embedded member's position fills that
    /// member's own slots — as many as it lists, the rest keeping their
    /// defaults — and the value after it lands on the member that follows
    /// the embedded one. The flat list is equivalent (probed).
    #[test]
    fn a_nested_group_fills_an_embedded_member() {
        let defs = "\tSTRUCT In\np\tBYTE 5\nq\tBYTE 6\n\tENDS\n\tSTRUCT Out\nh\tBYTE 1\ni\tIn\nt\tBYTE 9\n\tENDS\n";
        let r = asm(&format!(
            "{defs}a\tOut {{ $11, {{$22, $33}}, $44 }}\nb\tOut {{ $11, $22, $33, $44 }}\nc\tOut {{ $11, {{$22}}, $44 }}\n\
             d\tOut {{ $11, {{}}, $44 }}\ne\tOut {{ $11, {{,$33}}, $44 }}\nf\tOut {{ {{$22}}, $44 }}\ng\tOut $11, {{$22, $33}}, $44\n\
             h\tOut {{\n\t$11,\n\t{{$22,\n\t$33}}\n}}\n\tld hl,a.i\n\tld hl,a.i.q\n"
        ))
        .expect("assembles");
        assert_eq!(
            r.bytes,
            vec![
                0x11, 0x22, 0x33, 0x44, // a: the group fills the embedded member
                0x11, 0x22, 0x33, 0x44, // b: the flat list is the same
                0x11, 0x22, 0x06, 0x44, // c: a partial group keeps the rest of its defaults
                0x11, 0x05, 0x06, 0x44, // d: an empty group keeps them all
                0x11, 0x05, 0x33, 0x44, // e: an empty slot inside the group
                0x01, 0x22, 0x06,
                0x44, // f: a group where a plain member sits is left for the embedded one
                0x11, 0x22, 0x33, 0x44, // g: unbraced outer list, braced group
                0x11, 0x22, 0x33, 0x09, // h: the group may span lines too
                0x21, 0x01, 0x00, // ld hl,a.i: the embedded member's own label binds
                0x21, 0x02, 0x00, // ld hl,a.i.q
            ]
        );
        // A group the embedded member cannot take is left over, and refused.
        let e =
            asm(&format!("{defs}a\tOut {{ $11, $22, {{$33}}, $44 }}\n")).expect_err("misplaced");
        assert!(e.to_string().contains("embedded structure"), "{e}");
        let e = asm(&format!("{defs}a\tOut {{ $11, {{$22, $33, $34}}, $44 }}\n"))
            .expect_err("too many");
        assert!(e.to_string().contains("too many"), "{e}");
    }
    /// #557: a macro is visible from its definition to the end of the
    /// assembly, whichever file defines it and whichever invokes it — probed
    /// against 1.21.0 in every direction below. Each file used to expand its
    /// own macros on its own, so a definition never crossed an `INCLUDE`.
    #[test]
    fn a_macro_is_visible_across_files_from_its_definition_on() {
        let loader = MemoryLoader::new()
            .text("macros.asm", "\tMACRO SHOW\n\tnop\n\tENDM\n")
            .text("user.asm", "\tSHOW\n\tld a,1\n")
            .text("mid.asm", "\tINCLUDE \"macros.asm\"\n\tld b,2\n");
        let run = |main: &str| assemble_sjasmplus_files(main, "main.asm", &loader);
        // Defined in an include, invoked by the includer.
        assert_eq!(
            run("\tINCLUDE \"macros.asm\"\n\tSHOW\n")
                .expect("assembles")
                .bytes,
            vec![0x00]
        );
        // Defined in one include, invoked by a later sibling and the includer.
        assert_eq!(
            run("\tINCLUDE \"macros.asm\"\n\tINCLUDE \"user.asm\"\n\tSHOW\n")
                .expect("assembles")
                .bytes,
            vec![0x00, 0x3E, 0x01, 0x00]
        );
        // Defined in the includer, invoked inside a later include.
        assert_eq!(
            run("\tMACRO SHOW\n\tnop\n\tENDM\n\tINCLUDE \"user.asm\"\n")
                .expect("assembles")
                .bytes,
            vec![0x00, 0x3E, 0x01]
        );
        // Defined in a nested include, visible all the way out.
        assert_eq!(
            run("\tINCLUDE \"mid.asm\"\n\tSHOW\n")
                .expect("assembles")
                .bytes,
            vec![0x06, 0x02, 0x00]
        );
        // An invocation above the `INCLUDE` that defines it is refused
        // (probed: `Unrecognized instruction: SHOW`), and the file that made
        // the mistake is the one named.
        let e = run("\tSHOW\n\tINCLUDE \"macros.asm\"\n\tSHOW\n").expect_err("too early");
        assert_eq!(e.error.line, 1, "{e}");
        assert!(e.to_string().contains("unknown instruction `SHOW`"), "{e}");
    }

    /// #557, the same rule within one file, which the per-file expander got
    /// wrong: a definition is visible only below itself and only when the
    /// walk reaches it — one in an untaken `IF` defines nothing — and a
    /// second definition of the name is an error at its header (probed:
    /// `Unrecognized instruction` ×2, `Duplicate macroname`).
    #[test]
    fn a_macro_is_defined_where_the_walk_reaches_it() {
        let e = asm("\tSHOW\n\tMACRO SHOW\n\tnop\n\tENDM\n\tSHOW\n").expect_err("used above");
        assert_eq!(e.line, 1, "{e}");
        let e = asm("\tIF 0\n\tMACRO SHOW\n\tnop\n\tENDM\n\tENDIF\n\tSHOW\n").expect_err("untaken");
        assert_eq!(e.line, 6, "{e}");
        assert!(e.to_string().contains("unknown instruction `SHOW`"), "{e}");
        assert_eq!(
            asm("\tIF 1\n\tMACRO SHOW\n\tnop\n\tENDM\n\tENDIF\n\tSHOW\n")
                .expect("taken")
                .bytes,
            vec![0x00]
        );
        let e = asm("\tMACRO SHOW\n\tnop\n\tENDM\n\tMACRO SHOW\n\tld a,1\n\tENDM\n\tSHOW\n")
            .expect_err("duplicate");
        assert_eq!(e.line, 4, "{e}");
        assert!(e.to_string().contains("duplicate macro name `SHOW`"), "{e}");
        // A definition left open is reported against its header.
        let e = asm("\tnop\n\tMACRO SHOW\n\tnop\n").expect_err("open");
        assert_eq!(e.line, 2, "{e}");
        assert!(e.to_string().contains("no matching `endm`"), "{e}");
    }

    /// #551: column 0 is the label column, without exception — a directive
    /// or mnemonic spelled there binds a label, and a dotted name there is
    /// a local label, not the dotted directive (probed, SjASMPlus 1.21.0:
    /// `.end nop` under `top` binds `top.end` and assembles the `nop`).
    #[test]
    fn a_column_zero_word_is_always_a_label() {
        let r = asm("top\tnop\n.end\tld (top),a\n\tjr .end\n").expect("assembles");
        assert_eq!(r.bytes, vec![0x00, 0x32, 0x00, 0x00, 0x18, 0xFB]);
        // Indented, `.end` is the END directive, which binds nothing and
        // stops before the otherwise-undefined reference below it (#554).
        assert_eq!(
            asm("\tnop\n\t.end\n\tdw .end\n")
                .expect("END stops the read")
                .bytes,
            vec![0x00]
        );
        // A mnemonic or an undotted directive in column 0 is a label too;
        // what follows is the operation.
        let r = asm("top\tnop\nnop\tnop\nend\tnop\n\tdw nop,end\n").expect("assembles");
        assert_eq!(r.bytes, vec![0x00, 0x00, 0x00, 0x01, 0x00, 0x02, 0x00]);
        // `db 1` in column 0 is the label `db` and then `1`, which is not an
        // instruction (the reference: `Unrecognized instruction: 1`).
        assert!(asm("db\t1\n").is_err());
    }

    /// #477 acceptance: the SpecNext Invaders shapes, in one source — the
    /// STRUCT from sprite.asm, the `ds Name * count` reservation, and the
    /// member-offset arithmetic its code leans on.
    #[test]
    fn specnext_invaders_struct_shapes_assemble() {
        let r = asm("\torg $8000\n\tSTRUCT SpriteAttributes\nx BYTE 0\ny BYTE 0\nmrx8 BYTE 0\nvpat BYTE 0\n\tENDS\nsprites\tds SpriteAttributes * 3\n\tld hl,sprites+SpriteAttributes.vpat\n\tld (ix+SpriteAttributes.y),1\n")
            .expect("assembles");
        assert_eq!(&r.bytes[12..15], &[0x21, 0x03, 0x80], "sprites + vpat");
        assert_eq!(&r.bytes[15..19], &[0xDD, 0x36, 0x01, 0x01], "(ix + y)");
    }

    /// The three option forms used by SpecNext Invaders, pinned against
    /// SjASMPlus 1.21.0. `--zxnext` changes the live instruction set;
    /// `=cspect` additionally admits the emulator's two fake instructions.
    /// The recommended strict syntax already matches these parser behaviours.
    #[test]
    fn opt_enables_the_specnext_invaders_option_state() {
        assert!(asm("\tnextreg $07,$02\n").is_err());
        assert_eq!(
            asm("\topt --zxnext\n\tnextreg $07,$02\n")
                .expect("Next enabled")
                .bytes,
            vec![0xED, 0x91, 0x07, 0x02]
        );
        assert_eq!(
            asm("\t.opt --zxnext=cspect\n\tbreak\n\texit\n")
                .expect("CSpect fakes enabled")
                .bytes,
            vec![0xFD, 0x00, 0xDD, 0x00]
        );
        assert_eq!(
            asm("\topt --syntax=abfw\n\tld a,(hl)\n\tsub a,b\n")
                .expect("recommended strict syntax")
                .bytes,
            vec![0x7E, 0x90]
        );
        assert!(asm("\topt --syntax=abfw\n\tld b,(1234)\n").is_err());

        // Option state is lexical across textual includes, as the project's
        // root OPT lines require for the Z80N instructions in its children.
        let loader = MemoryLoader::new().text("child.asm", "\tnextreg $07,$02\n");
        assert_eq!(
            assemble_sjasmplus_files(
                "\topt --zxnext\n\tinclude \"child.asm\"\n",
                "main.asm",
                &loader,
            )
            .expect("option flows into include")
            .bytes,
            vec![0xED, 0x91, 0x07, 0x02]
        );

        let err = asm("\topt push\n").expect_err("unsupported option state");
        assert!(err.to_string().contains("has not implemented"), "{err}");
    }

    /// `SLDOPT COMMENT` changes only the optional native `.sld` sidecar: the
    /// same four data bytes assemble with and without it. Its type spelling is
    /// nevertheless case-sensitive in SjASMPlus 1.21.0, so accepting it as
    /// inert must not make the grammar more permissive.
    #[test]
    fn sldopt_comment_is_explicitly_inert_for_machine_output() {
        let source = "\tdevice zxspectrum128\n\torg $8000\n\tdb 1\n";
        let with = "\tdevice zxspectrum128\n\tSLDOPT COMMENT WPMEM, LOGPOINT, ASSERTION\n\torg $8000\n\tdb 1\n";
        assert_eq!(
            asm(with).expect("SLD keyword selection").bytes,
            asm(source).expect("same source without SLDOPT").bytes
        );

        assert!(asm("\tSLDOPT comment WPMEM\n").is_err());
        assert!(asm("\tSLDOPT COMMENT\n").is_err());
        assert!(asm("\t.SLDOPT COMMENT WPMEM\n").is_ok());
    }

    /// The data directives beyond the shared Z80 set. Every expectation was
    /// read off sjasmplus v1.21.0 before it was written here.
    #[test]
    fn the_wider_data_directives_match_sjasmplus() {
        let b = |src: &str| asm(src).unwrap_or_else(|e| panic!("{src}: {e}")).bytes;
        // Widths, all little-endian.
        assert_eq!(b("\tword $1234\n"), vec![0x34, 0x12]);
        assert_eq!(b("\tdword $12345678\n"), vec![0x78, 0x56, 0x34, 0x12]);
        assert_eq!(b("\tdd $12345678\n"), b("\tdefd $12345678\n"));
        assert_eq!(b("\td24 $123456\n"), vec![0x56, 0x34, 0x12]);

        // `dz` appends a zero. `dc` sets bit 7 on the last character of each
        // *string* and leaves a number alone — `dc "ab",3,"cd"` is the case
        // that says which of those two rules is the real one.
        assert_eq!(b("\tdz \"ab\"\n"), vec![0x61, 0x62, 0x00]);
        assert_eq!(
            b("\tdc \"ab\",3,\"cd\"\n"),
            vec![0x61, 0xE2, 0x03, 0x63, 0xE4]
        );

        // Hex pairs, comma-separated or not.
        assert_eq!(b("\tdh 11,22\n"), vec![0x11, 0x22]);
        assert_eq!(b("\thex 3344\n"), vec![0x33, 0x44]);
        assert_eq!(b("\tdefh 1122\n"), vec![0x11, 0x22]);

        // Bit graphics: eight characters to a byte, `#` a one.
        assert_eq!(b("\tdg #-#-#-#-\n"), vec![0xAA]);
        assert_eq!(b("\tdefg ..##..##\n"), vec![0x33]);
        assert_eq!(b("\tdg #-------#------#\n"), vec![0x80, 0x81]);

        // `abyte` adds its offset to every byte; the suffixed forms add the
        // `dc` and `dz` conventions on top.
        assert_eq!(b("\tabyte 4 1,2\n"), vec![0x05, 0x06]);
        assert_eq!(b("\tabytec 0 \"ab\"\n"), vec![0x61, 0xE2]);
        assert_eq!(b("\tabytez 4 1,2\n"), vec![0x05, 0x06, 0x00]);

        // `block` is `ds` with a fill.
        assert_eq!(b("\tblock 3,$AA\n"), vec![0xAA; 3]);
        assert_eq!(b("\tblock 2\n"), vec![0x00; 2]);
    }

    /// A graphic run has to be a multiple of eight, and hex data pairs of
    /// digits — sjasmplus refuses either otherwise, and so does this.
    #[test]
    fn malformed_graphic_and_hex_data_are_refused() {
        assert!(asm("\tdg #-#\n").is_err(), "three bits is not a byte");
        assert!(asm("\thex 123\n").is_err(), "an odd digit count");
        assert!(asm("\tdh zz\n").is_err(), "not hex digits");
    }

    // -----------------------------------------------------------------------
    // The optional leading dot. Probed against SjASMPlus 1.21.0, 2026-08-23:
    // every directive it has takes one, `equ` included once it has a label.
    // -----------------------------------------------------------------------

    /// The whole surface, dotted. The conditionals already took a dot (#67);
    /// this is the same rule for everything else, which is where most of
    /// sjasmplus's remaining vocabulary gap was.
    /// The device model: pages are separate memory, so two written at one
    /// address concatenate; the bounds come from the device.
    /// `SAVEBIN` describes a file besides the machine code: the library says
    /// what it holds and the caller writes it. `bytes` still holds the code.
    #[test]
    fn savebin_describes_a_file_beside_the_machine_code() {
        let out = asm(" DEVICE ZXSPECTRUM48\n ORG $8000\n DB 1,2,3,4\n \
                       SAVEBIN \"x.bin\",$8000,4\n")
        .expect("savebin");
        assert_eq!(out.bytes, vec![1, 2, 3, 4], "the code is unchanged");
        assert_eq!(out.artifacts.len(), 1);
        assert_eq!(out.artifacts[0].name, "x.bin");
        assert_eq!(out.artifacts[0].bytes, vec![1, 2, 3, 4]);
        // A span inside the image, and two of them in source order.
        let two = asm(" DEVICE ZXSPECTRUM48\n ORG $8000\n DB 1,2,3,4\n \
                       SAVEBIN \"x.bin\",$8000,4\n SAVEBIN \"y.bin\",$8002,2\n")
        .expect("two");
        assert_eq!(two.artifacts.len(), 2);
        assert_eq!(two.artifacts[1].name, "y.bin");
        assert_eq!(two.artifacts[1].bytes, vec![3, 4]);
    }

    /// `SAVETAP` wraps the same span in a tape: a ROM header block naming it,
    /// then the block. The header's two parameter words are the tape's own —
    /// for `CODE` the start address and `$8000`, for a `BASIC` program
    /// `$8000` (no auto-start line) and the length again — and both were
    /// measured rather than reasoned about.
    #[test]
    fn savetap_wraps_a_span_in_a_tape() {
        let out = asm(" DEVICE ZXSPECTRUM48\n ORG $8000\n DB 1,2,3,4\n \
                       SAVETAP \"x.tap\",CODE,\"name\",$8000,4\n")
        .expect("savetap");
        assert_eq!(out.bytes, vec![1, 2, 3, 4], "the code is unchanged");
        let tape = &out.artifacts[0];
        assert_eq!(tape.name, "x.tap");
        // 19-byte header: kind 3, `name` padded to ten, length 4, $8000, $8000.
        assert_eq!(&tape.bytes[..4], &[0x13, 0x00, 0x00, 3]);
        assert_eq!(&tape.bytes[4..14], b"name      ");
        assert_eq!(&tape.bytes[14..20], &[4, 0, 0x00, 0x80, 0x00, 0x80]);
        // then the data block: its own length, flag $FF, the span, the parity.
        assert_eq!(
            &tape.bytes[21..],
            &[6, 0, 0xFF, 1, 2, 3, 4, 0xFF ^ 1 ^ 2 ^ 3 ^ 4]
        );

        // A `BASIC` header parks $8000 in the first parameter whatever start
        // it was given, and repeats the length in the second.
        let basic = asm(" DEVICE ZXSPECTRUM48\n ORG $8000\n DB 1,2,3,4\n \
                         SAVETAP \"x.tap\",BASIC,\"n\",$8000,4\n")
        .expect("basic");
        assert_eq!(&basic.artifacts[0].bytes[3..4], &[0]);
        assert_eq!(&basic.artifacts[0].bytes[14..20], &[4, 0, 0x00, 0x80, 4, 0]);
    }

    /// The forms that name no kind save a whole device's memory rather than a
    /// span, so they wait on the same fact a wide `SAVEBIN` does.
    #[test]
    fn a_tape_of_the_whole_device_is_refused_as_our_gap() {
        let err = asm(" DEVICE ZXSPECTRUM48\n ORG $8000\n DB 1,2,3,4\n \
                       SAVETAP \"x.tap\",$8000,4\n")
        .expect_err("whole memory")
        .to_string();
        assert!(err.contains("the gap is ours"), "{err}");
        // A kind this dialect does not write says which it does.
        let err = asm(" DEVICE ZXSPECTRUM48\n ORG $8000\n DB 1,2,3,4\n \
                       SAVETAP \"x.tap\",SCREEN$,\"n\",$8000,4\n")
        .expect_err("kind")
        .to_string();
        assert!(err.contains("CODE, BASIC"), "{err}");
    }

    /// It needs a device to read memory out of — SjASMPlus answers "SAVEBIN
    /// only allowed in real device emulation mode" without one, and `DEVICE
    /// NONE` counts as none however far above it a real device was opened.
    #[test]
    fn savebin_needs_a_device() {
        for src in [
            " ORG $8000\n DB 1\n SAVEBIN \"x.bin\",$8000,1\n",
            " DEVICE NONE\n ORG $8000\n DB 1\n SAVEBIN \"x.bin\",$8000,1\n",
            " DEVICE ZXSPECTRUM48\n ORG $8000\n DB 1\n DEVICE NONE\n \
              SAVEBIN \"x.bin\",$8000,1\n",
        ] {
            let err = asm(src).expect_err(src).to_string();
            assert!(err.contains("real device emulation mode"), "{err}");
        }
        // The start is required; the name alone is a syntax error there.
        assert!(
            asm(" DEVICE ZXSPECTRUM48\n ORG $8000\n DB 1\n SAVEBIN \"x.bin\"\n").is_err(),
            "no start"
        );
    }

    /// A span reaching outside what the source assembled is refused by name.
    /// A device's memory starts as a booted machine's — the Spectrum's
    /// attribute file, system variables and UDGs are all non-zero before a
    /// line is assembled — so answering with zeros would be right across most
    /// of the address space and wrong in three parts of it.
    #[test]
    fn a_span_outside_the_image_is_refused_as_our_gap() {
        for (src, what) in [
            (" SAVEBIN \"x.bin\",$0000,4\n", "before the image"),
            (" SAVEBIN \"x.bin\",$8000,10\n", "past the end"),
            (" SAVEBIN \"x.bin\",$8000\n", "to the end of the space"),
            (
                " SAVEBIN \"x.bin\",$8000,0\n",
                "zero length reads as the same",
            ),
        ] {
            let err = asm(&format!(
                " DEVICE ZXSPECTRUM48\n ORG $8000\n DB 1,2,3,4\n{src}"
            ))
            .expect_err(what)
            .to_string();
            assert!(err.contains("the gap is ours"), "{what}: {err}");
        }
    }

    #[test]
    fn pages_are_separate_memory_bounded_by_the_device() {
        let out =
            asm(" DEVICE ZXSPECTRUM128\n SLOT 3\n PAGE 1\n ORG $C000\n db $11\n PAGE 2\n db $22\n")
                .expect("assembles");
        assert_eq!(out.bytes, vec![0x11, 0x22]);

        // Bounds are the device's, and differ between them.
        assert!(asm(" DEVICE ZXSPECTRUM48\n PAGE 3\n db 1\n").is_ok());
        let err = asm(" DEVICE ZXSPECTRUM48\n PAGE 4\n db 1\n").expect_err("out of range");
        assert!(err.to_string().contains("4 pages"), "got `{err}`");
        assert!(asm(" DEVICE ZXSPECTRUM128\n PAGE 7\n db 1\n").is_ok());

        // The Next is the one with eight slots and 224 pages.
        assert!(asm(" DEVICE ZXSPECTRUMNEXT\n SLOT 7\n PAGE 223\n db 1\n").is_ok());
        assert!(asm(" DEVICE ZXSPECTRUM128\n SLOT 4\n db 1\n").is_err());

        // The Plus is sized by its cartridge, not its RAM: 32 pages, four
        // slots (#538).
        assert!(asm(" DEVICE AMSTRADCPCPLUS\n SLOT 3\n PAGE 31\n db 1\n").is_ok());
        let err = asm(" DEVICE AMSTRADCPCPLUS\n PAGE 32\n db 1\n").expect_err("out of range");
        assert!(err.to_string().contains("32 pages"), "got `{err}`");
        assert!(asm(" DEVICE AMSTRADCPCPLUS\n SLOT 4\n db 1\n").is_err());

        // `NONE` is no device: no bounds, as with no `DEVICE` line at all.
        assert!(asm(" DEVICE NONE\n PAGE 999\n db 1\n").is_ok());
        assert!(asm(" PAGE 999\n db 1\n").is_ok());

        let unknown = asm(" DEVICE NOTAREALMACHINE\n nop\n").expect_err("unknown");
        assert!(
            unknown.to_string().contains("is not a device"),
            "got `{unknown}`"
        );
    }

    fn cpr_page(artifact: &crate::engine::Artifact, page: usize) -> &[u8] {
        let chunk = 12 + page * (8 + 0x4000);
        assert_eq!(&artifact.bytes[chunk..chunk + 2], b"cb");
        assert_eq!(
            &artifact.bytes[chunk + 2..chunk + 4],
            format!("{page:02}").as_bytes()
        );
        assert_eq!(
            &artifact.bytes[chunk + 4..chunk + 8],
            &0x4000u32.to_le_bytes()
        );
        &artifact.bytes[chunk + 8..chunk + 8 + 0x4000]
    }

    /// #563: emitted bytes are replayed through the live CPC Plus slot map.
    /// Slots initially map to their matching pages; `PAGE` changes the
    /// current slot (slot 3 by default), while `SLOT` selects another one.
    #[test]
    fn savecpr_replays_emission_through_the_live_slot_map() {
        let out = asm(" DEVICE AMSTRADCPCPLUS\n ORG $0000\n DB $10\n \
             ORG $4000\n DB $11\n SLOT 1\n PAGE 5\n DB $55\n \
             ORG $8000\n DB $12\n ORG $C000\n DB $13\n \
             SAVECPR \"mapped.cpr\",6\n")
        .expect("mapped cartridge");
        let cpr = &out.artifacts[0];
        assert_eq!(cpr.name, "mapped.cpr");
        assert_eq!(cpr.format, crate::engine::ArtifactFormat::Cpr);
        assert_eq!(&cpr.bytes[..4], b"RIFF");
        assert_eq!(&cpr.bytes[8..12], b"AMS!");
        assert_eq!(cpr_page(cpr, 0)[0], 0x10);
        assert_eq!(cpr_page(cpr, 1)[0], 0x11);
        assert_eq!(cpr_page(cpr, 2)[0], 0x12);
        assert_eq!(cpr_page(cpr, 3)[0], 0x13);
        assert_eq!(cpr_page(cpr, 5)[1], 0x55);
    }

    /// A page change is a raw-output boundary, not an address-counter reset.
    /// At `$4000` it therefore keeps writing slot 1 even though PAGE remaps
    /// the default current slot 3. Repeated writes use last-write-wins.
    #[test]
    fn page_preserves_the_counter_and_device_writes_overwrite() {
        let out = asm(
            " DEVICE AMSTRADCPCPLUS\n ORG $4000\n DB 1\n PAGE 5\n DB 2\n \
             SLOT 2\n PAGE 1\n ORG $8000\n DB 9\n SAVECPR \"x.cpr\",6\n",
        )
        .expect("counter and overwrite");
        let cpr = &out.artifacts[0];
        assert_eq!(&cpr_page(cpr, 1)[..2], &[9, 2]);
        assert_eq!(cpr_page(cpr, 5)[0], 0);
    }

    /// SAVECPR snapshots where it stands and participates in the same
    /// source-ordered artifact list as span saves.
    #[test]
    fn savecpr_is_a_source_ordered_snapshot() {
        let out = asm(" DEVICE AMSTRADCPCPLUS\n ORG $4000\n DB 1,2\n \
             SAVEBIN \"before.bin\",$4000,2\n \
             SAVECPR \"middle.cpr\",2\n DB 3\n \
             SAVEBIN \"after.bin\",$4000,3\n SAVECPR \"last.cpr\",2\n")
        .expect("ordered snapshots");
        assert_eq!(
            out.artifacts
                .iter()
                .map(|a| a.name.as_str())
                .collect::<Vec<_>>(),
            ["before.bin", "middle.cpr", "after.bin", "last.cpr"]
        );
        assert_eq!(&cpr_page(&out.artifacts[1], 1)[..3], &[1, 2, 0]);
        assert_eq!(&cpr_page(&out.artifacts[3], 1)[..3], &[1, 2, 3]);
    }

    /// The Plus cartridge starts zero-filled for #563; booted-machine seeds
    /// belong to #318 and to the devices whose native saves expose them.
    #[test]
    fn savecpr_serialises_zero_filled_pages_and_validates_its_device() {
        let out =
            asm(" DEVICE AMSTRADCPCPLUS\n SAVECPR \"empty.cpr\",32\n").expect("empty cartridge");
        let cpr = &out.artifacts[0];
        assert_eq!(cpr.bytes.len(), 12 + 32 * (8 + 0x4000));
        assert!(cpr_page(cpr, 31).iter().all(|byte| *byte == 0));
        assert_eq!(
            u32::from_le_bytes(cpr.bytes[4..8].try_into().expect("RIFF size is four bytes"),)
                as usize,
            cpr.bytes.len() - 8
        );

        for src in [
            " SAVECPR \"x.cpr\",1\n",
            " DEVICE ZXSPECTRUM48\n SAVECPR \"x.cpr\",1\n",
            " DEVICE AMSTRADCPCPLUS\n SAVECPR \"x.cpr\",0\n",
            " DEVICE AMSTRADCPCPLUS\n SAVECPR \"x.cpr\",33\n",
        ] {
            assert!(asm(src).is_err(), "{src:?}");
        }
    }

    /// Device directives execute in the shared textual stream, including
    /// includes and macro expansions; no file-local pre-scan may own them.
    #[test]
    fn device_mapping_flows_across_includes_and_macros() {
        let loader = MemoryLoader::new().text("map.asm", " SLOT 1\n PAGE 5\n");
        let included = assemble_sjasmplus_files(
            " DEVICE AMSTRADCPCPLUS\n INCLUDE \"map.asm\"\n ORG $4000\n DB $55\n \
             SAVECPR \"x.cpr\",6\n",
            "main.asm",
            &loader,
        )
        .expect("include mapping");
        assert_eq!(cpr_page(&included.artifacts[0], 5)[0], 0x55);

        let expanded = asm(
            " DEVICE AMSTRADCPCPLUS\n MACRO MAP\n SLOT 2\n PAGE 6\n ENDM\n \
             MAP\n ORG $8000\n DB $66\n SAVECPR \"x.cpr\",7\n",
        )
        .expect("macro mapping");
        assert_eq!(cpr_page(&expanded.artifacts[0], 6)[0], 0x66);
    }

    /// Comparisons answer `$FF`, and the spellings are the dialect's own:
    /// sjasmplus refuses `<>` where the 6502 family takes it.
    #[test]
    fn comparisons_answer_minus_one_in_the_dialects_own_spellings() {
        let ok = |src: &str| asm(src).unwrap_or_else(|e| panic!("{src:?}: {e}")).bytes;
        assert_eq!(
            ok(" db 2=2,2==2,2!=3,2<3,2>3,2<=3,2>=3\n"),
            vec![0xFF, 0xFF, 0xFF, 0xFF, 0x00, 0xFF, 0x00]
        );
        assert!(asm(" db 2<>3\n").is_err(), "sjasmplus refuses `<>`");
        // The shifts still lex as shifts.
        assert_eq!(ok(" db 1<<3, 16>>2\n"), vec![8, 4]);
    }

    /// sjasmplus's `ALIGN` is power-of-two only and says so in as many words.
    /// It defaults to 4 with no operand, and takes a fill byte.
    #[test]
    fn align_is_power_of_two_only() {
        let ok = |src: &str| asm(src).unwrap_or_else(|e| panic!("{src:?}: {e}")).bytes;
        assert_eq!(ok(" db 1\n align 4\n db 2\n"), vec![1, 0, 0, 0, 2]);
        assert_eq!(
            ok(" db 1\n align\n db 2\n"),
            vec![1, 0, 0, 0, 2],
            "defaults to 4"
        );
        assert_eq!(
            ok(" db 1\n align 4,$ff\n db 2\n"),
            vec![1, 0xFF, 0xFF, 0xFF, 2]
        );
        assert_eq!(ok(" db 1\n align 1\n db 2\n"), vec![1, 2]);
        assert_eq!(ok(" db 1,2,3,4\n align 4\n db 9\n"), vec![1, 2, 3, 4, 9]);
        let err = asm(" db 1\n align 3\n").expect_err("refused");
        assert!(err.to_string().contains("Illegal align: 3"), "got `{err}`");
        // The dotted spelling reaches the same parser.
        assert_eq!(ok(" db 1\n .align 4\n db 2\n"), vec![1, 0, 0, 0, 2]);
    }

    #[test]
    fn every_directive_takes_an_optional_dot() {
        let ok = |src: &str| asm(src).unwrap_or_else(|e| panic!("{src:?}: {e}")).bytes;
        assert_eq!(ok(" .db 1,2\n"), vec![1, 2]);
        assert_eq!(ok(" .defb 3\n"), vec![3]);
        assert_eq!(ok(" .byte 4\n"), vec![4]);
        assert_eq!(ok(" .dw $1234\n"), vec![0x34, 0x12]);
        assert_eq!(ok(" .ds 2\n"), vec![0, 0]);
        assert_eq!(
            ok("x .equ 5\n .db x\n"),
            vec![5],
            "`equ`, once it has a label"
        );
        assert_eq!(ok(" .define V 5\n .db V\n"), vec![5]);
        assert_eq!(ok(" .macro m\n .db 7\n .endm\n m\n"), vec![7]);
        assert_eq!(ok(" .dup 2\n .db 8\n .edup\n"), vec![8, 8]);
        assert_eq!(ok(" .rept 2\n .db 9\n .endr\n"), vec![9, 9]);
        assert_eq!(ok(" .if 1\n .db 1\n .endif\n"), vec![1]);
        assert_eq!(
            ok(" .module foo\nbar: .db 1\n .endmodule\n .db foo.bar\n"),
            vec![1, 0]
        );
        assert_eq!(
            ok(" db 1\n"),
            vec![1],
            "and the bare spelling is unaffected"
        );
    }

    /// The dot strips *before* the case test, as it does for the conditionals:
    /// a spelling that is case-sensitive stays case-sensitive with one.
    #[test]
    fn the_dot_does_not_relax_the_case_rule() {
        assert!(
            asm(" .Dup 2\n .db 1\n .edup\n").is_err(),
            "`.Dup` is as unacceptable as `Dup`"
        );
    }

    /// The formatter keeps a dotted `equ`'s label on its line, as it does the
    /// bare one. This is not a layout preference: `.equ` split from its label
    /// does not assemble, and the differential's format-then-assemble check is
    /// what caught it.
    #[test]
    fn the_formatter_keeps_a_dotted_equ_inline() {
        let out = crate::format_sjasmplus("x .equ 5\n db x\n").expect("format");
        assert_eq!(out, "x: .equ 5\n        db x\n");
        assert_eq!(asm(&out).expect("reassemble").bytes, vec![5]);
    }
    /// pasmo shares the Z80 core and must not gain the form — it reads `.db`
    /// as an ordinary label, and accepting it would invent a dialect.
    #[test]
    fn pasmo_does_not_take_a_dotted_directive() {
        assert!(crate::assemble_pasmo(" .db 1\n").is_err());
    }

    /// Declared as well as dispatched. The surface is what the dialect pages
    /// and `cargo xtask surface` read, and the two drifting apart is the
    /// failure the declaration exists to prevent.
    #[test]
    fn the_dotted_spellings_are_declared() {
        let declared: Vec<String> = crate::directives::surfaces()
            .into_iter()
            .filter(|s| s.dialect == "sjasmplus")
            .flat_map(|s| s.directives)
            .flat_map(|d| d.spellings())
            .collect();
        for spelling in [".db", ".org", ".equ", ".macro", ".module", ".include"] {
            assert!(
                declared.iter().any(|s| s == spelling),
                "`{spelling}` is accepted and not declared"
            );
        }
    }

    /// A module left open at end of file assembles, and now says so. The
    /// reference warns once, naming the *innermost* module by its full dotted
    /// path — not one advisory per open module (probe m19).
    #[test]
    fn an_unclosed_module_is_reported() {
        let r = asm("    MODULE foo\nbar: db 1\n").expect("assemble");
        assert_eq!(r.bytes, vec![0x01], "it still assembles");
        assert_eq!(r.warnings.len(), 1);
        assert!(
            r.warnings[0]
                .message
                .contains("`ENDMODULE` missing for module `foo`")
        );
        assert_eq!(
            r.warnings[0].line, 1,
            "reported against the line that opened it"
        );

        let r = asm("    MODULE foo\n    MODULE baz\nbar: db 1\n").expect("assemble");
        assert_eq!(r.warnings.len(), 1, "one advisory, not one per module");
        assert!(r.warnings[0].message.contains("`foo.baz`"));

        assert!(
            asm("    MODULE foo\nbar: db 1\n    ENDMODULE\n")
                .expect("assemble")
                .warnings
                .is_empty()
        );
    }

    // -----------------------------------------------------------------------
    // Forward-referenced conditions (#99,
    // `decisions/forward-conditions-and-passes.md`). Probed against SjASMPlus
    // 1.21.0, 2026-08-23 — bytes *and* warnings.
    // -----------------------------------------------------------------------

    /// A condition may name a symbol defined below it. Pass 1 reads it as
    /// zero and says so; later passes answer from the pass before.
    #[test]
    fn a_condition_may_reach_forward() {
        let r = asm(" IF later\n ld a,1\n ENDIF\nlater: nop\n").expect("assemble");
        assert_eq!(r.bytes, vec![0x00]);
        assert_eq!(r.warnings.len(), 1);
        assert!(
            r.warnings[0]
                .message
                .contains("forward reference of symbol `later`")
        );
        assert_eq!(r.warnings[0].line, 1);
    }

    /// A backward reference is not a forward one, and must not say it is —
    /// the walk binds each label to its address as it passes, so this folds
    /// against a value and warns about nothing.
    #[test]
    fn a_backward_condition_needs_no_pass() {
        let r = asm("later: nop\n IF later\n ld a,1\n ENDIF\n").expect("assemble");
        assert_eq!(r.bytes, vec![0x00]);
        assert!(r.warnings.is_empty(), "{:?}", r.warnings);
    }

    /// The case #99 was really about. Emitting the body moves `later` past 2,
    /// so the condition that admitted the body is false by the end — and the
    /// body is in the binary. The reference ships that and warns twice; so do
    /// we, rather than converging further than it does or refusing what it
    /// builds.
    #[test]
    fn a_condition_that_never_settles_warns_and_ships() {
        let r = asm(" IF later < 2\n ld a,1\n ENDIF\nlater: nop\n").expect("assemble");
        assert_eq!(r.bytes, vec![0x3E, 0x01, 0x00]);
        assert_eq!(r.warnings.len(), 2, "{:?}", r.warnings);
        assert!(
            r.warnings[1]
                .message
                .contains("has a different value in pass 3")
        );
        assert!(
            r.warnings[1]
                .message
                .contains("previous value 0 not equal 2")
        );
        assert_eq!(r.warnings[1].line, 4, "reported on the label's line");
    }

    /// pasmo shares the walk and must not gain the behaviour through it: it
    /// keeps the parse-time-constant rule and the diagnostic that explains it.
    #[test]
    fn pasmo_still_requires_a_constant_condition() {
        assert!(
            crate::assemble_pasmo(" IF later\n ld a,1\n ENDIF\nlater: nop\n").is_err(),
            "pasmo has no forward-condition adoption"
        );
    }

    // -----------------------------------------------------------------------
    // `:` as a statement separator (#98,
    // `decisions/colon-separated-statements.md`). Probes s1-s11 against
    // SjASMPlus 1.21.0, 2026-08-23.
    // -----------------------------------------------------------------------

    /// The plain case, which is what showed this was never about conditionals:
    /// instructions separated by `:` failed exactly the way the colon-inline
    /// `IF` did.
    #[test]
    fn a_colon_separates_statements() {
        assert_eq!(
            asm(" ld a,1 : ld b,2\n").expect("assemble").bytes,
            vec![0x3E, 0x01, 0x06, 0x02]
        );
        assert_eq!(
            asm(" ld a,1:ld b,2\n").expect("assemble").bytes,
            vec![0x3E, 0x01, 0x06, 0x02],
            "the spaces are not what makes it one"
        );
        assert_eq!(
            asm(" ld a,1 : : ld b,2\n").expect("assemble").bytes,
            vec![0x3E, 0x01, 0x06, 0x02],
            "an empty statement between two colons is nothing"
        );
    }

    /// The colon that closes a label is not a separator, and the rule that
    /// tells them apart is positional: first in its statement, nothing but an
    /// identifier before it. That covers a local label and `::` as well.
    #[test]
    fn a_labels_colon_is_not_a_separator() {
        assert_eq!(
            asm("lbl: ld a,1 : ld b,2\n djnz lbl\n")
                .expect("assemble")
                .bytes,
            vec![0x3E, 0x01, 0x06, 0x02, 0x10, 0xFA]
        );
        assert_eq!(
            asm("glob:\n.l: ld a,1 : ld b,2\n").expect("assemble").bytes,
            vec![0x3E, 0x01, 0x06, 0x02]
        );
        assert_eq!(
            asm("gl:: ld a,1 : ld b,2\n").expect("assemble").bytes,
            vec![0x3E, 0x01, 0x06, 0x02],
            "`::` closes a label as one token"
        );
    }

    /// A colon inside a literal separates nothing, and neither does one in a
    /// comment — the comment is found first and rides with its statement.
    #[test]
    fn a_colon_in_a_literal_or_comment_separates_nothing() {
        assert_eq!(
            asm(" db \":\" : db 1\n").expect("assemble").bytes,
            vec![0x3A, 0x01]
        );
        assert_eq!(
            asm(" db ':' : db 1\n").expect("assemble").bytes,
            vec![0x3A, 0x01]
        );
        assert_eq!(
            asm(" ld a,1 ; a:b\n").expect("assemble").bytes,
            vec![0x3E, 0x01]
        );
    }

    /// A block may open, fill and close inside one line's statements — which
    /// is the form #67 filed, and it falls out rather than being handled.
    #[test]
    fn a_conditional_fits_on_one_line() {
        assert_eq!(
            asm(" IF 1 : ld a,1 : ENDIF\n").expect("assemble").bytes,
            vec![0x3E, 0x01]
        );
        assert_eq!(
            asm(" IF 0 : ld a,1 : ENDIF\n ld b,2\n")
                .expect("assemble")
                .bytes,
            vec![0x06, 0x02]
        );
    }

    /// The formatter puts one statement on each line, which is what it already
    /// did to a label sharing a line with an operation. Idempotent, and the
    /// same bytes.
    #[test]
    fn the_formatter_expands_a_colon_line() {
        let out = crate::format_sjasmplus("lbl: ld a,1 : ld b,2\n djnz lbl\n").expect("format");
        assert_eq!(
            out,
            "lbl:\n        ld a,1\n        ld b,2\n        djnz lbl\n"
        );
        assert_eq!(
            crate::format_sjasmplus(&out).expect("idempotent"),
            out,
            "formatting the output changes nothing further"
        );
    }

    /// Each statement gets its own debug span, all naming the line they share.
    /// Nothing collapses, which is why the frozen wire format needed no column.
    #[test]
    fn each_statement_on_a_colon_line_gets_its_own_span() {
        let r = asm(" ld a,1 : ld b,2\n nop\n").expect("assemble");
        let spans: Vec<_> = r
            .debug
            .lines
            .iter()
            .map(|s| (s.line, s.offset, s.length))
            .collect();
        assert_eq!(spans, vec![(1, 0, 2), (1, 2, 2), (2, 4, 1)]);
    }

    /// pasmo shares the whole Z80 core and must not pick the form up through
    /// it — it has no colon separator, and splitting on a character it treats
    /// as ordinary would invent a dialect.
    #[test]
    fn pasmo_does_not_split_on_a_colon() {
        assert!(crate::assemble_pasmo(" ld a,1 : ld b,2\n").is_err());
    }

    // -----------------------------------------------------------------------
    // The two macro spellings (#205). Probes n1–n11 against SjASMPlus 1.21.0,
    // run 2026-08-23.
    // -----------------------------------------------------------------------

    /// The reference takes the definition either way round, and the name-first
    /// form carries parameters like the other (n1, n3, n4, n5).
    #[test]
    fn a_macro_may_be_defined_name_first() {
        assert_eq!(
            asm("mk MACRO a, b\n db a,b\n ENDM\n mk 1,2\n")
                .expect("assemble")
                .bytes,
            vec![0x01, 0x02]
        );
        assert_eq!(
            asm("mk: MACRO a\n db a\n ENDM\n mk 3\n")
                .expect("assemble")
                .bytes,
            vec![0x03],
            "the colon form is the same definition"
        );
        assert_eq!(
            asm("mk macro a\n db a\n endm\n mk 4\n")
                .expect("assemble")
                .bytes,
            vec![0x04],
            "the keyword is case-insensitive here too"
        );
        assert_eq!(
            asm("mk MACRO\n nop\n ENDM\n mk\n").expect("assemble").bytes,
            vec![0x00],
            "no parameters"
        );
    }

    /// Which spelling a line is depends on its **column**, and a line in the
    /// wrong one is not a definition at all. The reference reads a column-0
    /// `MACRO` as a label (n9), and an indented `mk MACRO a` as an
    /// unrecognised instruction (n8). Both are errors there, and here.
    #[test]
    fn the_macro_spellings_are_told_apart_by_indentation() {
        assert!(
            asm(" mk MACRO a\n db a\n ENDM\n mk 8\n").is_err(),
            "an indented name-first header is not a definition"
        );
        assert!(
            asm("MACRO kw a\n db a\n ENDM\n kw 9\n").is_err(),
            "a column-0 keyword-first header is not a definition"
        );
    }

    /// Parameters are comma-separated in both spellings (n2, n7), and a comma
    /// may not stand between the keyword and the name in either (n10, n11).
    /// The last of these is a case the previous grammar accepted and the
    /// reference does not.
    #[test]
    fn macro_parameters_are_comma_separated() {
        assert!(
            asm("mk MACRO a b\n db a,b\n ENDM\n mk 1,2\n").is_err(),
            "space-separated parameters, name-first"
        );
        assert!(
            asm(" MACRO kw a b\n db a,b\n ENDM\n kw 5,6\n").is_err(),
            "space-separated parameters, keyword-first"
        );
        assert!(
            asm("mk MACRO, a\n db a\n ENDM\n mk 10\n").is_err(),
            "a comma after the keyword"
        );
        assert!(
            asm(" MACRO kw, a\n db a\n ENDM\n kw 11\n").is_err(),
            "a comma after the name — the reference calls it an illegal macro name"
        );
    }

    // -----------------------------------------------------------------------
    // Modules (#93's third item). Every case below is a probe against
    // SjASMPlus 1.21.0, run 2026-08-23 — the probe ids are the ones the plan
    // (`docs/plans/2026-08-23-001-feat-sjasmplus-modules-plan.md`) tabulates.
    // -----------------------------------------------------------------------

    /// The base rule: a module prefixes the labels defined inside it, and the
    /// qualified name is how the outside reaches them (m1). Nesting
    /// concatenates with `.` (m5), and `ENDMOD` closes as well as `ENDMODULE`
    /// (m7).
    #[test]
    fn a_module_prefixes_the_labels_defined_inside_it() {
        assert_eq!(
            asm("    MODULE foo\nbar: db 1\n    ENDMODULE\n    db foo.bar\n")
                .expect("assemble")
                .bytes,
            vec![0x01, 0x00]
        );
        assert_eq!(
            asm(
                "    MODULE foo\n    MODULE baz\nbar: db 1\n    ENDMODULE\n    \
                 ENDMODULE\n    db foo.baz.bar\n"
            )
            .expect("assemble")
            .bytes,
            vec![0x01, 0x00]
        );
        assert_eq!(
            asm("    MODULE foo\nbar: db 1\n    ENDMOD\n    db foo.bar\n")
                .expect("assemble")
                .bytes,
            vec![0x01, 0x00]
        );
    }

    /// A reference has **two** candidates and only two: the fully-qualified
    /// name, then the bare global one. Inside `foo`, `bar` finds `foo.bar`
    /// (m2) and `top` finds the global `top` (m13) — but outside, `bar` finds
    /// nothing (m3).
    #[test]
    fn a_reference_tries_the_qualified_name_then_the_global_one() {
        assert_eq!(
            asm("    MODULE foo\nbar: db 1\n    db bar\n    ENDMODULE\n")
                .expect("assemble")
                .bytes,
            vec![0x01, 0x00]
        );
        assert_eq!(
            asm("top: db 9\n    MODULE foo\n    db top\n    ENDMODULE\n")
                .expect("assemble")
                .bytes,
            vec![0x09, 0x00]
        );
        assert!(
            asm("    MODULE foo\nbar: db 1\n    ENDMODULE\n    db bar\n").is_err(),
            "a module's name is not visible unqualified from outside"
        );
    }

    /// The qualified candidate wins when both exist (m31) — so a module may
    /// shadow a global of the same name without the outer one leaking in.
    #[test]
    fn the_qualified_candidate_wins_over_the_global_one() {
        assert_eq!(
            asm("x equ $AA\n    MODULE foo\nx equ $BB\n    db x\n    ENDMODULE\n")
                .expect("assemble")
                .bytes,
            vec![0xBB]
        );
    }

    /// There is **no walk-up**: an inner module does not see an outer module's
    /// unqualified names, only its own and the globals (m8, m32). This is the
    /// rule that makes the two-candidate model a model rather than a shortcut,
    /// so it gets its own test.
    #[test]
    fn an_inner_module_does_not_see_the_outer_modules_names() {
        assert!(
            asm(
                "    MODULE foo\nouter: db 1\n    MODULE baz\n    db outer\n    \
                 ENDMODULE\n    ENDMODULE\n"
            )
            .is_err(),
            "`outer` is `foo.outer`; `foo.baz` reaches neither it nor a global"
        );
        assert_eq!(
            asm("g equ $CC\n    MODULE foo\n    MODULE baz\n    db g\n    \
                 ENDMODULE\n    ENDMODULE\n")
            .expect("assemble")
            .bytes,
            vec![0xCC],
            "the second candidate is the global, at any depth"
        );
    }

    /// The choice between the two candidates cannot be made as the line is
    /// read: either may be defined later (m33, m34). Both directions resolve.
    #[test]
    fn a_forward_reference_picks_the_right_candidate() {
        assert_eq!(
            asm("    MODULE foo\n    db bar\nbar equ $DD\n    ENDMODULE\n")
                .expect("assemble")
                .bytes,
            vec![0xDD]
        );
        assert_eq!(
            asm("    MODULE foo\n    db g\n    ENDMODULE\ng equ $EE\n")
                .expect("assemble")
                .bytes,
            vec![0xEE]
        );
    }

    /// A leading `@` escapes module scoping — on a definition (m4, m15), on a
    /// reference (m9), and on an already-dotted name (m30).
    #[test]
    fn an_at_sign_escapes_the_module_scope() {
        assert_eq!(
            asm("    MODULE foo\n@bar: db 1\n    ENDMODULE\n    db bar\n")
                .expect("assemble")
                .bytes,
            vec![0x01, 0x00]
        );
        assert_eq!(
            asm(
                "    MODULE foo\n    MODULE baz\n@bar: db 1\n    ENDMODULE\n    \
                 ENDMODULE\n    db bar\n"
            )
            .expect("assemble")
            .bytes,
            vec![0x01, 0x00]
        );
        assert_eq!(
            asm("    MODULE foo\nbar: db 1\n    ENDMODULE\ntop: db 2\n    \
                 MODULE foo2\n    db @top\n    ENDMODULE\n")
            .expect("assemble")
            .bytes,
            vec![0x01, 0x02, 0x01]
        );
        assert_eq!(
            asm("    MODULE foo\nbar: db 1\n    ENDMODULE\n    db @foo.bar\n")
                .expect("assemble")
                .bytes,
            vec![0x01, 0x00]
        );
    }

    /// Locals compose *under* modules, not beside them: the leading-`.` rule
    /// runs first and the module prefix wraps its result, so `.loc` under
    /// `glob` inside `foo` is `foo.glob.loc` (m6, m25).
    #[test]
    fn a_local_label_inside_a_module_is_qualified_by_both() {
        assert_eq!(
            asm("    MODULE foo\nglob:\n.loc: db 1\n    db glob.loc\n    ENDMODULE\n")
                .expect("assemble")
                .bytes,
            vec![0x01, 0x00]
        );
        assert_eq!(
            asm("    MODULE foo\nglob:\n.loc: db 1\n    ENDMODULE\n    db foo.glob.loc\n")
                .expect("assemble")
                .bytes,
            vec![0x01, 0x00]
        );
    }

    /// A macro is not module-scoped, but its *expansion* is: the labels a
    /// macro defines take the prefix of wherever it was invoked (m18, m23).
    #[test]
    fn a_macro_expands_into_the_module_that_invoked_it() {
        assert_eq!(
            asm(
                "    MACRO mk\nlbl: db 1\n    ENDM\n    MODULE foo\n    mk\n    \
                 ENDMODULE\n    db foo.lbl\n"
            )
            .expect("assemble")
            .bytes,
            vec![0x01, 0x00]
        );
        assert_eq!(
            asm("    MODULE foo\n    MACRO mk\n    db 1\n    ENDM\n    ENDMODULE\n    mk\n")
                .expect("assemble")
                .bytes,
            vec![0x01],
            "the macro name itself stays global"
        );
    }

    /// `DEFINE` is not module-scoped either (m24), and `equ` is (m11).
    #[test]
    fn equ_is_module_scoped_and_define_is_not() {
        assert_eq!(
            asm("    MODULE foo\nbar equ 7\n    ENDMODULE\n    db foo.bar\n")
                .expect("assemble")
                .bytes,
            vec![0x07]
        );
        assert_eq!(
            asm("    MODULE foo\n    DEFINE V 5\n    ENDMODULE\n    db V\n")
                .expect("assemble")
                .bytes,
            vec![0x05]
        );
    }

    /// Reopening a module name adds to it rather than starting again (m12).
    #[test]
    fn a_module_may_be_reopened() {
        assert_eq!(
            asm(
                "    MODULE foo\nbar: db 1\n    ENDMODULE\n    MODULE foo\nbaz: db 2\n    \
                 ENDMODULE\n    db foo.bar, foo.baz\n"
            )
            .expect("assemble")
            .bytes,
            vec![0x01, 0x02, 0x00, 0x01]
        );
    }

    /// The keyword follows the same strict case rule as the conditionals and
    /// repetition — all-lower or all-upper, never mixed (m21, m26).
    #[test]
    fn the_module_keyword_is_all_one_case() {
        assert_eq!(
            asm("    module foo\nbar: db 1\n    endmodule\n    db foo.bar\n")
                .expect("assemble")
                .bytes,
            vec![0x01, 0x00]
        );
        assert!(
            asm("    Module foo\nbar: db 1\n    EndModule\n").is_err(),
            "the reference answers `Module` with `Unrecognized instruction`"
        );
    }

    /// The reference reads a column-0 `MODULE` as a *label* and the name after
    /// it as an instruction (m27), so `MODULE` is deliberately absent from the
    /// directive set that suppresses column-0 label parsing. Indentation is
    /// load-bearing, in the reference and here.
    #[test]
    fn a_column_zero_module_is_a_label_not_a_directive() {
        assert!(
            asm("MODULE foo\nbar: db 1\nENDMODULE\n    db foo.bar\n").is_err(),
            "`foo` is then an unknown instruction, as in the reference"
        );
    }

    /// The three malformed cases: no name (m10), a dotted name, which is not a
    /// nesting shorthand (m29), and a close with nothing open (m20).
    #[test]
    fn a_malformed_module_is_refused() {
        assert!(
            asm("    MODULE\nbar: db 1\n    ENDMODULE\n").is_err(),
            "no name"
        );
        assert!(
            asm("    MODULE foo.baz\nbar: db 1\n    ENDMODULE\n").is_err(),
            "a dotted name is rejected, not read as nesting"
        );
        assert!(
            asm("    endmodule\nx: db 1\n").is_err(),
            "close with nothing open"
        );
    }

    /// Modules are sjasmplus's alone: pasmo shares the whole Z80 core, and
    /// must not pick the spelling up through it.
    #[test]
    fn pasmo_does_not_have_modules() {
        assert!(
            crate::assemble_pasmo("    MODULE foo\nbar: db 1\n    ENDMODULE\n").is_err(),
            "pasmo has no MODULE"
        );
    }

    /// The keyword is case-insensitive but the **name** is not — measured
    /// against sjasmplus 1.21.0, and not a combination anyone would guess.
    /// Defining `mac` and calling `MAC` is an error there, so it must be here.
    #[test]
    fn the_macro_keyword_is_case_insensitive_but_the_name_is_not() {
        assert_eq!(
            asm(" macro m\n nop\n endm\n m\n").expect("assemble").bytes,
            vec![0x00]
        );
        assert!(
            asm(" MACRO mac\n nop\n ENDM\n MAC\n").is_err(),
            "a macro name is case-sensitive"
        );
    }

    /// A macro containing a loop must be usable more than once — which is most
    /// of what macros are for. The dot-local is scoped to the expansion, so the
    /// second invocation does not collide with the first.
    #[test]
    fn a_macro_local_label_is_scoped_to_its_expansion() {
        assert_eq!(
            asm(" MACRO m\n.loc djnz .loc\n ENDM\n m\n m\n m\n")
                .expect("assemble")
                .bytes,
            vec![0x10, 0xFE, 0x10, 0xFE, 0x10, 0xFE]
        );
    }

    /// Macros compose, and a macro may invoke one defined **later** — the
    /// reference resolves a name when it expands, not when it reads, which is
    /// why every definition is collected before anything expands.
    #[test]
    fn macros_nest_and_may_invoke_one_defined_later() {
        assert_eq!(
            asm(" MACRO inner v\n ld a,v\n ENDM\n MACRO outer w\n inner w\n ENDM\n outer 5\n")
                .expect("assemble")
                .bytes,
            vec![0x3E, 0x05]
        );
        assert_eq!(
            asm(" MACRO outer\n inner\n ENDM\n MACRO inner\n nop\n ENDM\n outer\n")
                .expect("assemble")
                .bytes,
            vec![0x00]
        );
    }

    /// Locals stay distinct through nesting: one outer expansion invoking the
    /// same inner macro twice still gets two separate labels.
    #[test]
    fn locals_stay_distinct_through_nesting() {
        assert_eq!(
            asm(" MACRO m\n.loc djnz .loc\n ENDM\n MACRO two\n m\n m\n ENDM\n two\n")
                .expect("assemble")
                .bytes,
            vec![0x10, 0xFE, 0x10, 0xFE]
        );
    }

    /// An error in generated code must say where the text came from: the
    /// failing line is nowhere in the file the reader has open. Frames are
    /// innermost first, matching the `included from` chain's order.
    #[test]
    fn an_error_in_an_expansion_carries_its_frames() {
        let err = asm(" MACRO inner\n frobnicate\n ENDM\n MACRO outer\n inner\n ENDM\n outer\n")
            .expect_err("frobnicate is not an instruction");
        let span = err.span.as_ref().expect("a span");
        let named: Vec<&str> = span
            .expansion_frames
            .iter()
            .map(|f| f.macro_name.as_str())
            .collect();
        assert_eq!(named, vec!["inner", "outer"], "innermost first");
        assert_eq!(span.line, 7, "and it points at the invocation");
    }

    /// Source with no macros is untouched — no frames, nothing to explain.
    #[test]
    fn an_error_outside_an_expansion_carries_none() {
        let err = asm(" frobnicate\n").expect_err("not an instruction");
        assert!(
            err.span
                .as_ref()
                .is_none_or(|s| s.expansion_frames.is_empty()),
            "{err:?}"
        );
    }

    /// Repetition's count is an expression over the environment, so it folds
    /// where conditions fold rather than in the macro pre-pass — `DUP n+1`
    /// with `n equ 2` repeats three times.
    #[test]
    fn dup_repeats_its_body_a_computed_number_of_times() {
        assert_eq!(
            asm(" DUP 3\n nop\n EDUP\n").expect("assemble").bytes,
            vec![0x00, 0x00, 0x00]
        );
        assert_eq!(
            asm("n equ 2\n DUP n+1\n nop\n EDUP\n")
                .expect("assemble")
                .bytes,
            vec![0x00, 0x00, 0x00]
        );
        assert!(
            asm(" DUP 0\n nop\n EDUP\n")
                .expect("assemble")
                .bytes
                .is_empty(),
            "zero repetitions emit nothing"
        );
    }

    /// `REPT`/`ENDR` is the same block, and the spellings interchange — the
    /// reference accepts a `DUP` closed by `ENDR`.
    #[test]
    fn rept_and_dup_are_the_same_block() {
        let dup = asm(" DUP 2\n nop\n EDUP\n").expect("assemble").bytes;
        assert_eq!(asm(" REPT 2\n nop\n ENDR\n").expect("assemble").bytes, dup);
        assert_eq!(asm(" DUP 2\n nop\n ENDR\n").expect("assemble").bytes, dup);
    }

    /// Blocks nest, and interleave with macros in both directions.
    #[test]
    fn repetition_nests_and_composes_with_macros() {
        assert_eq!(
            asm(" DUP 2\n DUP 2\n nop\n EDUP\n EDUP\n")
                .expect("assemble")
                .bytes,
            vec![0x00; 4]
        );
        assert_eq!(
            asm(" MACRO m\n nop\n ENDM\n DUP 2\n m\n EDUP\n")
                .expect("assemble")
                .bytes,
            vec![0x00, 0x00]
        );
        assert_eq!(
            asm(" MACRO m\n DUP 2\n nop\n EDUP\n ENDM\n m\n")
                .expect("assemble")
                .bytes,
            vec![0x00, 0x00]
        );
    }

    /// Mixed case is not a block keyword, matching the strict rule the
    /// conditionals already follow — the reference calls `Dup` an unrecognised
    /// instruction.
    #[test]
    fn a_mixed_case_repetition_keyword_is_not_one() {
        assert_eq!(
            asm(" dup 2\n nop\n edup\n").expect("assemble").bytes,
            vec![0x00, 0x00]
        );
        assert!(
            asm(" Dup 2\n nop\n Edup\n").is_err(),
            "mixed case is not `DUP`"
        );
    }

    /// A self-recursive macro segfaults sjasmplus (exit 139). We decline to
    /// reproduce that: a crash is not a verdict about anyone's source, and an
    /// assembler that dies is worse than one that explains itself.
    #[test]
    fn runaway_recursion_is_reported_not_crashed_on() {
        let err = asm(" MACRO recur\n recur\n ENDM\n recur\n").expect_err("recursive");
        assert!(err.message.contains("recur"), "names the macro: {err:?}");
        assert!(err.message.contains("recursive"), "{err:?}");
    }

    /// A *plain* label in a macro body stays global, so a second invocation
    /// collides — the reference reports `Duplicate label` for exactly this, so
    /// scoping it would diverge from the tool we claim to match.
    #[test]
    fn a_plain_label_in_a_macro_body_stays_global() {
        let err = asm(" MACRO m\nplain djnz plain\n ENDM\n m\n m\n")
            .expect_err("the second expansion redefines `plain`");
        assert!(err.message.contains("duplicate label"), "{err:?}");
    }

    /// Substitution respects word boundaries: a parameter `v` must leave the
    /// symbol `val` alone. A naive replace would assemble `ld a,5al`.
    #[test]
    fn substitution_stops_at_word_boundaries() {
        assert_eq!(
            asm(" MACRO m v\nval equ 9\n ld a,val\n ENDM\n m 5\n")
                .expect("assemble")
                .bytes,
            vec![0x3E, 0x09]
        );
    }

    /// And it does not reach inside string literals — `db "v"` emits the
    /// letter, not the argument.
    #[test]
    fn substitution_does_not_reach_inside_strings() {
        assert_eq!(
            asm(" MACRO m v\n db \"v\"\n ENDM\n m 5\n")
                .expect("assemble")
                .bytes,
            vec![b'v']
        );
    }

    /// Substitution is textual and happens before the expression is evaluated,
    /// so `val*2` with `val = 5` is `ld a,10`.
    #[test]
    fn a_parameter_substitutes_before_the_expression_is_evaluated() {
        assert_eq!(
            asm(" MACRO m val\n ld a,val*2\n ENDM\n m 5\n")
                .expect("assemble")
                .bytes,
            vec![0x3E, 0x0A]
        );
    }

    /// A diagnostic must name a line the author wrote. An error inside a macro
    /// body reports the **invocation**, which is where a reader looks first —
    /// the expanded line number never existed in their file.
    #[test]
    fn an_error_inside_an_expansion_names_the_invocation() {
        let err = asm(" nop\n MACRO bad\n frobnicate\n ENDM\n nop\n bad\n")
            .expect_err("frobnicate is not an instruction");
        assert_eq!(err.line, 6, "the invocation is on line 6: {err:?}");
    }

    /// A label may sit in front of an invocation, and binds to the address the
    /// expansion starts at — the same rule a label on an `include` line
    /// follows, and the colon is optional as everywhere else.
    ///
    /// Getting this wrong does not mis-assemble; it rejects the line, because
    /// the label is read as the mnemonic. The reference assembles it.
    #[test]
    fn a_label_may_sit_in_front_of_an_invocation() {
        for src in [
            " MACRO m1 v\n ld a,v\n ENDM\nlbl: m1 9\n ld hl,lbl\n",
            " MACRO m1 v\n ld a,v\n ENDM\nlbl m1 9\n ld hl,lbl\n",
        ] {
            // The label is at $0000: it precedes the expansion, not follows it.
            assert_eq!(
                asm(src).expect(src).bytes,
                vec![0x3E, 0x09, 0x21, 0x00, 0x00],
                "{src}"
            );
        }
    }

    /// The formatter lays source out; it does not rewrite programs. Formatting
    /// must therefore give the macro **back**, not the lines it expands to.
    ///
    /// This is a regression test with teeth: expansion is a source pre-pass, so
    /// the obvious wiring — one hook on the shared parse — silently made `fmt`
    /// inline every invocation and delete every definition. Over a file, in
    /// place. The parse the formatter asks for is deliberately not the parse
    /// assembly asks for (`z80::Expand`).
    #[test]
    fn formatting_preserves_a_macro_rather_than_expanding_it() {
        let src = " MACRO setv v\n ld a,v\n ENDM\n setv 9\n";
        let out = crate::format_sjasmplus(src).expect("formats");
        assert!(out.contains("MACRO setv v"), "definition is gone: {out}");
        assert!(out.contains("ENDM"), "terminator is gone: {out}");
        assert!(out.contains("setv 9"), "invocation is gone: {out}");
        assert!(!out.contains("ld a,9"), "expanded into the source: {out}");
        // And assembly of the same text still expands, so the two paths really
        // are different parses rather than one of them being broken.
        assert_eq!(asm(src).expect("assembles").bytes, vec![0x3E, 0x09]);
    }

    /// Arity is checked in both directions, and unterminated definitions are
    /// caught where they begin rather than at end of file.
    ///
    /// The two directions get different words because sjasmplus gives them
    /// different words, and they are different mistakes: too many arguments
    /// means the call is wrong, too few usually means the macro moved on.
    #[test]
    fn arity_and_termination_are_checked() {
        let short = asm(" MACRO m v\n ld a,v\n ENDM\n m\n").expect_err("too few arguments");
        assert!(short.message.contains("not enough arguments"), "{short:?}");
        let long = asm(" MACRO m v\n ld a,v\n ENDM\n m 1,2\n").expect_err("too many arguments");
        assert!(long.message.contains("too many arguments"), "{long:?}");
        let open = asm(" MACRO m\n nop\n").expect_err("no endm");
        assert_eq!(open.line, 1, "reported where the definition opened");
        assert!(open.message.contains("`endm`"), "{open:?}");
    }

    #[test]
    fn number_formats() {
        // All of these are $1234.
        for src in [" ld hl, $1234", " ld hl, 0x1234", " ld hl, 1234h"] {
            assert_eq!(asm(src).expect(src).bytes, vec![0x21, 0x34, 0x12], "{src}");
        }
        // All of these are %1010 = 0x0A.
        for src in [" ld a, %1010", " ld a, 0b1010", " ld a, 1010b"] {
            assert_eq!(asm(src).expect(src).bytes, vec![0x3E, 0x0A], "{src}");
        }
    }

    #[test]
    fn slash_slash_comment() {
        assert_eq!(
            asm(" ld a, 5  // a comment\n").expect("//").bytes,
            vec![0x3E, 0x05]
        );
    }

    #[test]
    fn shares_instruction_syntax_with_pasmo() {
        // Identical bytes to pasmo for the shared instruction syntax.
        let src = "        org $8000\nloop:   ld a, (ix+5)\n        bit 7,(hl)\n        ldir\n        jr loop\n";
        assert_eq!(
            asm(src).expect("sjasm").bytes,
            crate::assemble_pasmo(src).expect("pasmo").bytes
        );
    }

    #[test]
    fn ds_reserves_bytes() {
        assert_eq!(asm("        ds 3\n").expect("ds").bytes, vec![0, 0, 0]);
    }

    #[test]
    fn oversized_byte_truncates_with_a_warning() {
        // sjasmplus keeps the low 8 bits and warns (byte-identical to sjasmplus:
        // `ld a,$1234` -> 3e 34, one warning).
        let a = asm("        ld a,$1234\n").expect("truncate");
        assert_eq!(a.bytes, vec![0x3E, 0x34]);
        assert_eq!(a.warnings.len(), 1);
        assert!(a.warnings[0].message.contains("truncated"));
        // In range: no warning.
        assert!(asm("        ld a,$12\n").expect("ok").warnings.is_empty());
    }

    #[test]
    fn byte_is_db() {
        // sjasmplus's `byte` behaves exactly like `db` — values and strings.
        // Byte-for-byte against `sjasmplus --raw`.
        assert_eq!(
            asm("        byte 1,2,$ff\n").expect("byte vals").bytes,
            vec![0x01, 0x02, 0xFF]
        );
        assert_eq!(
            asm("        byte \"AB\"\n").expect("byte str").bytes,
            vec![0x41, 0x42]
        );
    }

    #[test]
    fn local_labels_scope_under_the_preceding_global() {
        // The same `.loop` recurs under two globals; each `jr .loop` binds to
        // its own scope. Validated byte-for-byte against the sjasmplus binary.
        let src = "        org $8000\n\
                   start:\n.loop:  nop\n        jr .loop\n        nop\n\
                   done:\n.loop:  nop\n        jr .loop\n";
        let a = asm(src).expect("local scoping");
        assert_eq!(a.bytes, vec![0x00, 0x18, 0xFD, 0x00, 0x00, 0x18, 0xFD]);
        // The qualified names are distinct in the symbol table.
        assert_eq!(a.symbols.get("start.loop"), Some(&0x8000));
        assert_eq!(a.symbols.get("done.loop"), Some(&0x8004));
    }

    #[test]
    fn pasmo_rejects_reused_local_label() {
        // pasmo treats `.loop` as an ordinary global, so reuse is a duplicate.
        let src = "start:\n.loop:  nop\ndone:\n.loop:  nop\n";
        let err = crate::assemble_pasmo(src).expect_err("duplicate");
        assert!(err.message.contains("duplicate"), "unexpected: {err}");
    }

    #[test]
    fn location_counter_is_statement_start() {
        // `$` is the current statement's address (matches pasmo and the binary).
        let a = asm("        org $8000\n        jr $\n        ld hl,$\n").expect("pc");
        assert_eq!(a.bytes, vec![0x18, 0xFE, 0x21, 0x02, 0x80]);
    }

    // -----------------------------------------------------------------------
    // Conditional assembly + DEFINE (language-surface U8). Every byte
    // expectation below is pinned by the sjasmplus 1.21.0 probe runs (the
    // u8-probes set); the same programs ride the differential corpus.
    // -----------------------------------------------------------------------

    /// AE4 (R5): taken and untaken branches, with `ELSE`, byte-identical to
    /// the reference (probe p1).
    #[test]
    fn conditional_takes_the_live_branch() {
        let src = "        org $8000\n\
                   \x20       IF 1\n        ld a,1\n        ELSE\n        ld a,2\n        ENDIF\n\
                   \x20       IF 0\n        ld b,1\n        ELSE\n        ld b,2\n        ENDIF\n";
        assert_eq!(asm(src).expect("p1").bytes, vec![0x3E, 0x01, 0x06, 0x02]);
    }

    /// Condition grammar: comparisons (`=`/`==`/`>`/`<`/`>=`/`!=`),
    /// arithmetic truthiness, `&&`/`||`/`!` (probe p2), and the
    /// parenthesised logical forms (probe p45).
    #[test]
    fn condition_expressions_match_the_reference() {
        let src = "        org $8000\n\
                   VAL     equ 5\n\
                   \x20       IF VAL = 5\n        ld a,1\n        ENDIF\n\
                   \x20       IF VAL == 5\n        ld a,2\n        ENDIF\n\
                   \x20       IF VAL > 3\n        ld a,3\n        ENDIF\n\
                   \x20       IF VAL < 3\n        ld a,4\n        ENDIF\n\
                   \x20       IF VAL*2-10\n        ld a,5\n        ENDIF\n\
                   \x20       IF VAL && 0\n        ld a,6\n        ENDIF\n\
                   \x20       IF VAL || 0\n        ld a,7\n        ENDIF\n\
                   \x20       IF !VAL\n        ld a,8\n        ENDIF\n\
                   \x20       IF (VAL = 5) && !(VAL && 0)\n        ld a,9\n        ENDIF\n";
        assert_eq!(
            asm(src).expect("conditions").bytes,
            vec![0x3E, 1, 0x3E, 2, 0x3E, 3, 0x3E, 7, 0x3E, 9]
        );
    }

    /// `IFDEF`/`IFNDEF` test the DEFINE namespace only — a same-named `equ`
    /// constant or label is *not* "defined" (probe p3), and names are
    /// case-sensitive (probe p22).
    #[test]
    fn ifdef_namespace_is_defines_only_and_case_sensitive() {
        let src = "        org $8000\n\
                   \x20       DEFINE flag\n\
                   CONST   equ 7\n\
                   LBL:    nop\n\
                   \x20       IFDEF flag\n        ld a,1\n        ENDIF\n\
                   \x20       IFDEF FLAG\n        ld a,2\n        ENDIF\n\
                   \x20       IFDEF CONST\n        ld a,3\n        ENDIF\n\
                   \x20       IFDEF LBL\n        ld a,4\n        ENDIF\n\
                   \x20       IFNDEF NOPE\n        ld a,5\n        ENDIF\n";
        assert_eq!(asm(src).expect("ifdef").bytes, vec![0x00, 0x3E, 1, 0x3E, 5]);
    }

    /// `DEFINE NAME value` substitutes textually at identifier boundaries —
    /// operands, whole instructions, chains — but never inside strings or
    /// partial identifiers (probes p4/p5/p20/p21/p24/p26).
    #[test]
    fn define_substitutes_textually() {
        // Operand (p4) and expression (p6) positions.
        assert_eq!(
            asm("        DEFINE X 5\n        ld a,X\n")
                .expect("p4")
                .bytes,
            vec![0x3E, 5]
        );
        assert_eq!(
            asm("        DEFINE N 3\n        ld a,N+1\n        db N,N*2\n")
                .expect("p6")
                .bytes,
            vec![0x3E, 4, 3, 6]
        );
        // A whole-instruction replacement on a bare line (p5).
        assert_eq!(
            asm("        DEFINE X ld a,1\n        X\n")
                .expect("p5")
                .bytes,
            vec![0x3E, 1]
        );
        // Chained defines expand at use (p24).
        assert_eq!(
            asm("        DEFINE A1 3\n        DEFINE B1 A1+1\n        db B1\n")
                .expect("p24")
                .bytes,
            vec![4]
        );
        // A DEFINE'd name renames a label definition (p26).
        let r = asm("        org $8000\n        DEFINE L mylab\nL:      nop\n        jr mylab\n")
            .expect("p26");
        assert_eq!(r.bytes, vec![0x00, 0x18, 0xFD]);
        // Identifier boundaries: `NN` is not an occurrence of `N` (p20).
        assert!(asm("        DEFINE N 3\n        db NN\n").is_err(), "p20");
        // Strings are never rewritten (p21).
        assert_eq!(
            asm("        DEFINE N 3\n        db \"N\"\n")
                .expect("p21")
                .bytes,
            vec![0x4E]
        );
        // A duplicate DEFINE is the reference's error (p23).
        let e = asm("        DEFINE X 1\n        DEFINE X 2\n").expect_err("p23");
        assert!(e.message.contains("duplicate"), "unexpected: {e}");
    }

    /// A skipped branch defines nothing — labels, `equ` constants, and
    /// DEFINEs inside an untaken branch do not exist afterwards (probes
    /// p10/p10b), and untaken lines are never parsed at all (probe p31).
    #[test]
    fn skipped_branch_defines_nothing() {
        let src = "        org $8000\n\
                   \x20       IF 0\n\
                   skipped:  nop\n\
                   SKONST  equ 9\n\
                   \x20       DEFINE SKDEF\n\
                   \x20       ENDIF\n\
                   \x20       IFDEF SKDEF\n        ld a,1\n        ENDIF\n\
                   \x20       IFNDEF SKDEF\n        ld a,2\n        ENDIF\n";
        let r = asm(src).expect("skipped defines nothing");
        assert_eq!(r.bytes, vec![0x3E, 2]);
        assert!(!r.symbols.contains_key("skipped"), "skipped label leaked");
        // The skipped `equ` is unknown afterwards (the reference errors too).
        assert!(
            asm("        IF 0\nSK      equ 9\n        ENDIF\n        ld a,SK\n").is_err(),
            "p10b"
        );
        // Untaken lines are skipped without parsing (p31).
        assert_eq!(
            asm("        org $8000\n        IF 0\n        @@!! garbage (((\n        ENDIF\n        ld a,1\n")
                .expect("p31")
                .bytes,
            vec![0x3E, 1]
        );
    }

    /// Nested conditionals: the inner block evaluates only inside a taken
    /// outer branch, and nesting is tracked while skipping (probes p9/p42);
    /// lowercase keywords are the reference's other accepted spelling.
    #[test]
    fn conditionals_nest() {
        let src = "        org $8000\n\
                   \x20       if 1\n\
                   \x20       if 0\n        ld a,1\n        else\n        ld a,2\n        endif\n\
                   \x20       ifdef NOPE\n        ld a,3\n        endif\n\
                   \x20       endif\n";
        assert_eq!(asm(src).expect("p9").bytes, vec![0x3E, 2]);
        let src = "        org $8000\n\
                   \x20       IF 0\n\
                   \x20       IF 1\n        ld a,1\n        ENDIF\n        ld a,2\n\
                   \x20       ENDIF\n        ld a,3\n";
        assert_eq!(asm(src).expect("p42").bytes, vec![0x3E, 3]);
    }

    /// The environment threads across a conditional: an `equ` in a taken
    /// branch feeds later opcode-embedded form selection (probe p38), and a
    /// global label inside a taken branch rescopes later locals (probe p37).
    #[test]
    fn taken_branch_bindings_flow_out() {
        let src = "        org $8000\n\
                   \x20       IF 1\nBITN    equ 5\nPAD     equ 2\n        ENDIF\n\
                   \x20       bit BITN,a\n        ds PAD\n        ld a,1\n";
        assert_eq!(
            asm(src).expect("p38").bytes,
            vec![0xCB, 0x6F, 0, 0, 0x3E, 1]
        );
        let src = "        org $8000\n\
                   first:\n.l:     nop\n\
                   \x20       IF 1\nsecond:\n.l:     nop\n        jr .l\n        ENDIF\n\
                   \x20       jr .l\n";
        assert_eq!(
            asm(src).expect("p37").bytes,
            vec![0x00, 0x00, 0x18, 0xFD, 0x18, 0xFB]
        );
    }

    /// A label on the `IF` line binds at the block's address (probe p27).
    #[test]
    fn label_on_the_if_line_binds() {
        let r =
            asm("        org $8000\nlbl:    IF 1\n        ld a,1\n        ENDIF\n        jr lbl\n")
                .expect("p27");
        assert_eq!(r.bytes, vec![0x3E, 1, 0x18, 0xFC]);
        assert_eq!(r.symbols.get("lbl"), Some(&0x8000));
    }

    /// The block-structure error postures: an unterminated `IF`, a stray
    /// `ENDIF`, junk after `ENDIF` (the reference rejects it; junk after
    /// `ELSE` it ignores — probes p43/p43b/p35/p40), a stray `ELSEIF`, and an
    /// `ELSEIF` after `ELSE` all error clearly.
    #[test]
    fn block_structure_errors() {
        let e = asm("        IF 1\n        ld a,1\n").expect_err("p43");
        assert!(e.message.contains("ENDIF"), "unexpected: {e}");
        let e = asm("        ENDIF\n").expect_err("p43b");
        assert!(e.message.contains("without a matching"), "unexpected: {e}");
        let e = asm("        IF 1\n        ENDIF junk\n").expect_err("p35");
        assert!(e.message.contains("unexpected text"), "unexpected: {e}");
        // Junk after ELSE is tolerated, as the reference does (p40).
        assert_eq!(
            asm("        org $8000\n        IF 0\n        ld a,1\n        ELSE junk\n        ld a,2\n        ENDIF\n")
                .expect("p40")
                .bytes,
            vec![0x3E, 2]
        );
        let e = asm("        ELSEIF 1\n        ENDIF\n").expect_err("stray elseif");
        assert!(e.message.contains("without a matching"), "unexpected: {e}");
        // The reference tolerates an `ELSEIF` after `ELSE` by discarding it and
        // everything to the `ENDIF` (re-probed 2026-08-18). Dropping source
        // silently is worse than saying so, and no real program means it.
        let e = asm(
            "        IF 0\n        ld a,1\n        ELSE\n        ld a,2\n\
             \x20       ELSEIF 1\n        ld a,3\n        ENDIF\n",
        )
        .expect_err("elseif after else");
        assert!(e.message.contains("already closed"), "unexpected: {e}");
    }

    /// `ELSEIF` chains (#67), probed against the reference: the first true leg
    /// wins, a chain can end in `ELSE`, and none-true emits nothing.
    #[test]
    fn elseif_chains_pick_the_first_true_leg() {
        let asmb = |src: &str| asm(src).expect("chain").bytes;
        assert_eq!(
            asmb("        IF 0\n        ld a,1\n        ELSEIF 1\n        ld a,2\n        ENDIF\n"),
            vec![0x3E, 2]
        );
        assert_eq!(
            asmb("        IF 1\n        ld a,1\n        ELSEIF 1\n        ld a,2\n        ENDIF\n"),
            vec![0x3E, 1]
        );
        assert_eq!(
            asmb(
                "        IF 0\n        ld a,1\n        ELSEIF 1\n        ld a,2\n\
                  \x20       ELSEIF 1\n        ld a,3\n        ENDIF\n"
            ),
            vec![0x3E, 2],
            "the first true leg wins"
        );
        assert_eq!(
            asmb(
                "        IF 0\n        ld a,1\n        ELSEIF 0\n        ld a,2\n\
                  \x20       ELSE\n        ld a,3\n        ENDIF\n"
            ),
            vec![0x3E, 3]
        );
        assert_eq!(
            asmb(
                "        IF 0\n        ld a,1\n        ELSEIF 0\n        ld a,2\n        ENDIF\n        nop\n"
            ),
            vec![0x00],
            "no leg taken emits only what follows the block"
        );
    }

    /// Every conditional keyword also has a dotted spelling (#67). The dot does
    /// not relax the case rule, and dotted and undotted mix within one block —
    /// both probed against the reference.
    #[test]
    fn dotted_conditional_spellings() {
        let asmb = |src: &str| asm(src).expect("dotted").bytes;
        assert_eq!(
            asmb("        .IF 1\n        ld a,1\n        .ENDIF\n"),
            vec![0x3E, 1]
        );
        assert_eq!(
            asmb("        .if 1\n        ld a,1\n        .endif\n"),
            vec![0x3E, 1]
        );
        assert_eq!(
            asmb("        .IF 0\n        ld a,1\n        .ELSE\n        ld a,2\n        .ENDIF\n"),
            vec![0x3E, 2]
        );
        assert_eq!(
            asmb("        .IF 1\n        ld a,1\n        ENDIF\n"),
            vec![0x3E, 1],
            "dotted and undotted mix, as the reference allows"
        );
        assert!(
            asm("        .If 1\n        ld a,1\n        .EndIf\n").is_err(),
            "the dot does not relax the all-upper/all-lower rule"
        );
    }

    /// Keywords spell all-lower or all-upper only; a mixed-case `If` is an
    /// ordinary identifier, exactly as the reference treats it (probe p11).
    #[test]
    fn mixed_case_keywords_are_not_conditionals() {
        assert!(
            asm("        If 1\n        ld a,1\n        Endif\n").is_err(),
            "p11"
        );
    }

    /// Formatting a repetition changes the layout and not the program.
    ///
    /// This is a regression test for shipped **data loss**: `emit` had no arm
    /// for `Item::Repeat`, so the node fell through to the plain-line case,
    /// which renders its head — and the body and closer were dropped on the
    /// floor. `fmt` is documented as safe to run over source you have not
    /// read, and it was deleting loop bodies.
    #[test]
    fn a_formatted_repetition_keeps_its_body() {
        for src in [
            " DUP 3\n nop\n EDUP\n ret\n",
            " REPT 2\n inc a\n ENDR\n ret\n",
            " dup 2\n nop\n edup\n ret\n",
            " DUP 2\n DUP 3\n nop\n EDUP\n inc a\n EDUP\n ret\n",
        ] {
            let before = asm(src).expect(src).bytes;
            let formatted = crate::format_sjasmplus(src).expect(src);
            let after = asm(&formatted)
                .unwrap_or_else(|e| panic!("the formatted source assembles: {e:?}\n{formatted}"))
                .bytes;
            assert_eq!(
                before, after,
                "formatting changed the program:\n{formatted}"
            );

            let again = crate::format_sjasmplus(&formatted).expect("formats");
            assert_eq!(formatted, again, "{formatted}");
        }
    }

    /// The closer keeps the spelling and the case the author wrote. sjasmplus
    /// takes `EDUP` and `ENDR` for either opener, so choosing one would be the
    /// formatter rewriting a line rather than laying it out.
    #[test]
    fn a_repetitions_closer_is_not_respelled() {
        let out = crate::format_sjasmplus(" DUP 2\n nop\n ENDR\n").expect("formats");
        assert!(out.contains("ENDR"), "{out}");
        assert!(!out.contains("EDUP"), "the closer was respelled:\n{out}");
        let lower = crate::format_sjasmplus(" dup 2\n nop\n edup\n").expect("formats");
        assert!(lower.contains("edup"), "the closer was re-cased:\n{lower}");
    }
}
