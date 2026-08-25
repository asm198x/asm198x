//! Declarative instruction-set specifications for the 198x family CPUs.
//!
//! This crate is the **single source of truth for instruction encoding**:
//! mnemonic ↔ opcode bytes ↔ operand layout ↔ cycle counts ↔ affected flags.
//! Asm198x consumes it to assemble and disassemble; Emu198x validates its
//! hand-written decoders against it. The spec is *authored* from the primary
//! reference library (datasheets), not extracted from any emulator's decode
//! loop — see `decisions/asm198x-and-shared-isa-spec.md`.
//!
//! The types here describe **fixed-opcode-byte** CPUs (6502, Z80 and its
//! prefixes) as well as **field-based** variable-length encodings (68000-class
//! in [`m68k`]) and computed-operand CPUs (6809 in [`mos6809`]).
//!
//! Everything is `&'static` data so a whole instruction set is a compile-time
//! constant: zero dependencies, no allocation, diffable in review.

/// One encoding row a spec declares — the unit the form audit arbitrates and
/// the coverage metric counts.
///
/// Every spec in this crate enumerates its rows, whatever shape it authors
/// them in ([`Form`] tables, the 6809's `Kind`, the word CPUs' `Class`), so a
/// consumer can ask *what does this spec claim* without knowing which. See
/// `decisions/every-spec-enumerates-its-forms.md`.
///
/// A row is **derived**, never authored: it is computed from the spec data
/// beside it, so there is no second copy to fall out of step. It carries what
/// is true of every encoding regardless of shape, and nothing else — an
/// opcode-byte field would exclude the field-packed and computed-operand
/// specs, which is the whole reason this type exists.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Row {
    /// The mnemonic, as the spec spells it.
    pub mnemonic: &'static str,
    /// What distinguishes this encoding from the mnemonic's others: an
    /// addressing-mode label for the byte-opcode specs, the branch length for
    /// a 6809 branch, the class name for a word CPU.
    pub mode: &'static str,
    /// Undocumented / illegal opcode.
    pub undocumented: bool,
}

/// A complete instruction set for one CPU.
pub struct InstructionSet {
    /// Human name, e.g. `"MOS 6502"`.
    pub cpu: &'static str,
    /// Byte order for multi-byte operands.
    pub endianness: Endianness,
    /// Every mnemonic the CPU understands.
    pub instructions: &'static [Instruction],
}

impl InstructionSet {
    /// Find an instruction by mnemonic (case-sensitive; specs use upper-case).
    #[must_use]
    pub fn instruction(&self, mnemonic: &str) -> Option<&Instruction> {
        self.instructions.iter().find(|i| i.mnemonic == mnemonic)
    }

    /// Find the form for a mnemonic and mode label, scanning *every* entry with
    /// that mnemonic. A mnemonic's forms may be split across entries — e.g. the
    /// Z80 base `LD` and a separate `LD` entry for the IX/IY forms — to keep the
    /// spec readable; this looks across all of them.
    #[must_use]
    pub fn find_form(&self, mnemonic: &str, mode: &str) -> Option<&Form> {
        self.instructions
            .iter()
            .filter(|i| i.mnemonic == mnemonic)
            .find_map(|i| i.form(mode))
    }

    /// Whether any entry uses this mnemonic.
    #[must_use]
    pub fn has_mnemonic(&self, mnemonic: &str) -> bool {
        self.instructions.iter().any(|i| i.mnemonic == mnemonic)
    }

    /// Every encoding row this set declares — one per [`Form`].
    ///
    /// The `Form` specs get this for free: a form *is* a row. The specs that
    /// author their encodings some other way build the same rows from their
    /// own shape, so a consumer can count or iterate either without caring.
    pub fn rows(&self) -> impl Iterator<Item = Row> + '_ {
        self.instructions.iter().flat_map(|i| {
            i.forms.iter().map(|f| Row {
                mnemonic: i.mnemonic,
                mode: f.mode,
                undocumented: f.undocumented,
            })
        })
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Endianness {
    Little,
    Big,
}

/// One mnemonic and all the ways it can be encoded.
pub struct Instruction {
    /// Upper-case mnemonic, e.g. `"LDA"`.
    pub mnemonic: &'static str,
    /// One-line description, e.g. `"Load accumulator"`.
    pub summary: &'static str,
    /// One [`Form`] per addressing mode this mnemonic supports.
    pub forms: &'static [Form],
}

impl Instruction {
    /// Find the form for a given addressing-mode label (see [`Form::mode`]).
    #[must_use]
    pub fn form(&self, mode: &str) -> Option<&Form> {
        self.forms.iter().find(|f| f.mode == mode)
    }
}

/// One concrete encoding of an instruction — a single addressing mode.
pub struct Form {
    /// Fixed opcode bytes, in order. One byte for the 6502; a prefix sequence
    /// for prefixed Z80 opcodes (e.g. `&[0xCB, 0x40]`).
    pub opcode: &'static [u8],
    /// Dialect-facing addressing-mode label, e.g. `"immediate"`, `"absolute,x"`.
    /// The assembler's parser maps parsed operand syntax to this label, then
    /// looks the form up by it — so the label strings are a shared contract
    /// between this spec and each CPU's dialect front-end.
    pub mode: &'static str,
    /// Operand bytes emitted after the opcode, in order.
    pub operands: &'static [Operand],
    /// Trailing opcode bytes emitted *after* the operands. Empty for almost
    /// every form; used by the Z80 `DD CB` / `FD CB` group, whose final opcode
    /// byte follows the displacement operand (`DD CB <d> <op>`).
    pub suffix: &'static [u8],
    /// Timing.
    pub cycles: Cycles,
    /// Status flags affected, as a compact string, e.g. `"NZ"` or `"NZCV"`.
    /// Documentation- and disassembler-grade; the assembler ignores it.
    pub flags: &'static str,
    /// Undocumented / illegal opcode.
    pub undocumented: bool,
}

impl Form {
    /// One representative encoding of this form: the opcode bytes, a canonical
    /// value for each operand slot, then any suffix.
    ///
    /// The bytes are a valid instance of the form, not a meaningful program.
    /// What matters is that they are legal for the operand kinds, so a
    /// disassembler reading them writes source a real assembler will accept —
    /// which is how the form audit puts every row it declares to a reference
    /// tool. See `decisions/a-row-can-exemplify-itself.md`.
    ///
    /// It answers "show me one of these", not "encode this source": a user's
    /// operands, expression folding and symbol resolution belong to the
    /// dialect that reads them.
    ///
    /// Allocation-free, because this crate is.
    pub fn exemplar(&self) -> impl Iterator<Item = u8> + '_ {
        self.opcode
            .iter()
            .copied()
            .chain(self.operands.iter().flat_map(|op| {
                let (bytes, len): ([u8; 3], usize) = match op.kind {
                    // A small forward offset, little-endian over the width, so
                    // the target stays near and needs no label.
                    OperandKind::RelativePc => ([0x02, 0x00, 0x00], usize::from(op.bytes)),
                    OperandKind::Displacement => ([0x05, 0x00, 0x00], 1),
                    // Big-endian 16-bit immediate: the Z80N `push nn`, whose
                    // high byte comes first.
                    OperandKind::ImmediateBe => ([0x12, 0x34, 0x00], 2),
                    // $12 / $1234 / $123456, little-endian.
                    OperandKind::Immediate | OperandKind::Address => match op.bytes {
                        1 => ([0x12, 0x00, 0x00], 1),
                        2 => ([0x34, 0x12, 0x00], 2),
                        3 => ([0x56, 0x34, 0x12], 3),
                        _ => ([0x00, 0x00, 0x00], 0),
                    },
                };
                bytes.into_iter().take(len)
            }))
            .chain(self.suffix.iter().copied())
    }

    /// Total encoded length in bytes: opcode bytes, operand bytes, and any
    /// trailing suffix opcode bytes.
    #[must_use]
    pub fn len(&self) -> usize {
        self.opcode.len()
            + self.suffix.len()
            + self
                .operands
                .iter()
                .map(|o| o.bytes as usize)
                .sum::<usize>()
    }

    /// A form is never empty (it always has at least one opcode byte); this
    /// exists only to satisfy the `len`-without-`is_empty` lint cleanly.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        false
    }
}

/// One operand slot in an encoding: what kind of value, and how wide.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Operand {
    pub kind: OperandKind,
    /// Width in bytes. Laid out in the instruction set's [`Endianness`].
    pub bytes: u8,
}

/// The genuinely CPU-agnostic operand categories the assembler needs in order
/// to lay bytes down. Addressing-mode *flavour* (zero-page vs absolute, which
/// index register) lives in the [`Form::mode`] label and the dialect parser;
/// here we only describe the bytes on the wire.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum OperandKind {
    /// A literal value (immediate).
    Immediate,
    /// A big-endian immediate — laid down high byte first regardless of the
    /// instruction set's [`Endianness`]. The Z80N `push nn` (`ED 8A`) special
    /// case: uniquely in the Z80 set its 16-bit operand is stored big-endian.
    ImmediateBe,
    /// An absolute address or zero-page offset — distinguished by `bytes`.
    Address,
    /// A signed, PC-relative displacement (branches).
    RelativePc,
    /// A signed 8-bit offset added to an index register, e.g. the `d` in the
    /// Z80 `(IX+d)`. Emitted as one byte; range −128..=127.
    Displacement,
}

/// Per-form timing. Extra cycles are conditional and additive.
#[derive(Clone, Copy, Debug)]
pub struct Cycles {
    pub base: u8,
    /// Extra cycle when an indexed access crosses a page boundary.
    pub page_cross: u8,
    /// Extra cycle when a branch is taken (a further page-cross cycle on top
    /// is also possible on the 6502).
    pub branch_taken: u8,
}

impl Cycles {
    /// Fixed-cost form: `base` cycles, no conditional extras.
    #[must_use]
    pub const fn fixed(base: u8) -> Self {
        Self {
            base,
            page_cross: 0,
            branch_taken: 0,
        }
    }

    /// Indexed read that costs one more cycle across a page boundary.
    #[must_use]
    pub const fn page_crossing(base: u8) -> Self {
        Self {
            base,
            page_cross: 1,
            branch_taken: 0,
        }
    }

    /// Relative branch: `base` if not taken, `+1` if taken.
    #[must_use]
    pub const fn branch(base: u8) -> Self {
        Self {
            base,
            page_cross: 1,
            branch_taken: 1,
        }
    }
}

pub mod cdp1802;
pub mod cp1610;
pub mod f8;
pub mod huc6280;
pub mod i8048;
pub mod i8080;
pub mod m6800;
pub mod m68k;
pub mod machines;
pub mod mos6502;
pub mod mos65816;
pub mod mos6809;
pub mod pdp11;
pub mod provenance;
pub mod s2650;
pub mod scmp;
pub mod sm83;
pub mod tms7000;
pub mod tms9900;
pub mod z80;
pub mod z8000;

#[cfg(test)]
mod row_tests {
    /// [`InstructionSet::rows`] must agree with the count the coverage metric
    /// already derives (`instructions.iter().map(|i| i.forms.len()).sum()`),
    /// for every `Form` spec.
    ///
    /// This is what lets the metric move onto rows without a single reported
    /// number changing. A denominator that shifted while the seam landed would
    /// be indistinguishable from coverage falling, which is the one thing the
    /// stamp exists to make legible.
    #[test]
    fn rows_agree_with_the_denominator_the_metric_already_uses() {
        let sets: &[(&str, &crate::InstructionSet)] = &[
            ("1802", &crate::cdp1802::SET),
            ("2650", &crate::s2650::SET),
            ("6502", &crate::mos6502::SET),
            ("6800", &crate::m6800::SET),
            ("65816", &crate::mos65816::SET),
            ("8048", &crate::i8048::SET),
            ("8080", &crate::i8080::SET),
            ("F8", &crate::f8::SET),
            ("SC/MP", &crate::scmp::SET),
            ("TMS7000", &crate::tms7000::SET),
            ("Z80", &crate::z80::SET),
            ("HuC6280", &crate::huc6280::SET),
            ("SM83", &crate::sm83::SET),
            // The 68000 is absent deliberately. It authors its own `Form`
            // inside its own `Spec` — `base: u16` plus a size encoding plus
            // operand slots, with no mode label — so it is neither an
            // `InstructionSet` nor covered by this agreement. Whether its
            // field encoding enumerates as rows, or wants representatives
            // rather than a product, is one of the open questions in
            // `decisions/every-spec-enumerates-its-forms.md`.
        ];
        for (name, set) in sets {
            let legacy: usize = set.instructions.iter().map(|i| i.forms.len()).sum();
            assert_eq!(set.rows().count(), legacy, "{name}");
            assert!(legacy > 0, "{name} declares no forms");
        }
    }

    /// A word CPU declares one row per entry: its operands are fields of a
    /// single opcode word, so the entry *is* the encoding and the class is
    /// what tells one from another.
    ///
    /// Enumerating register numbers or mode bits as separate rows would
    /// multiply each table by its own operand space — measuring the CPU rather
    /// than the spec, and giving the audit a denominator nobody could arbitrate
    /// against a reference in reasonable time.
    #[test]
    fn a_word_cpu_declares_one_row_per_entry() {
        let cases: &[(&str, usize, usize)] = &[
            (
                "CP-1610",
                crate::cp1610::INSTRUCTIONS.len(),
                crate::cp1610::rows().count(),
            ),
            (
                "PDP-11",
                crate::pdp11::INSTRUCTIONS.len(),
                crate::pdp11::rows().count(),
            ),
            (
                "TMS9900",
                crate::tms9900::INSTRUCTIONS.len(),
                crate::tms9900::rows().count(),
            ),
        ];
        for (name, entries, rows) in cases {
            assert_eq!(rows, entries, "{name}");
            assert!(*entries > 0, "{name} declares no instructions");
        }
    }

    /// Within one mnemonic, rows must have **distinct** modes.
    ///
    /// This is the property a `(mnemonic, mode)` key needs, and it is weaker
    /// than "every mode is non-empty" on purpose: the Z80 spec spells "no
    /// addressing mode" as `""` for its 47 no-operand instructions, where the
    /// 6502 spells the same thing `"implied"`. Both are fine — `find_form`
    /// already looks forms up by that label — and normalising one to the other
    /// here would make a row disagree with the spec it is derived from.
    ///
    /// What is *not* fine is two rows of one mnemonic sharing a mode, which
    /// would collapse two encodings into one for anything keying on the pair,
    /// and would already be a lookup bug in `find_form`.
    #[test]
    fn a_mnemonics_rows_have_distinct_modes() {
        let sets: &[(&str, Vec<crate::Row>)] = &[
            ("6809", crate::mos6809::rows().collect()),
            ("CP-1610", crate::cp1610::rows().collect()),
            ("PDP-11", crate::pdp11::rows().collect()),
            ("TMS9900", crate::tms9900::rows().collect()),
            ("Z8000", crate::z8000::rows().collect()),
            ("Z80", crate::z80::SET.rows().collect()),
            ("6502", crate::mos6502::SET.rows().collect()),
            ("SM83", crate::sm83::SET.rows().collect()),
        ];
        for (cpu, rows) in sets {
            assert!(!rows.is_empty(), "{cpu} yields no rows");
            let mut seen = std::collections::BTreeSet::new();
            for row in rows {
                assert!(!row.mnemonic.is_empty(), "{cpu}: a row has no mnemonic");
                assert!(
                    seen.insert((row.mnemonic, row.mode)),
                    "{cpu}: `{}` declares two rows for mode `{}`",
                    row.mnemonic,
                    row.mode
                );
            }
        }
    }

    /// Every row of a word CPU names its own class, so the mode a consumer
    /// sees is the one the spec states rather than a name derived from a
    /// `Debug` derive — which is what the documentation generator used to do,
    /// by leaking a formatted string per row.
    #[test]
    fn a_word_cpu_row_names_its_class() {
        for (insn, row) in crate::tms9900::INSTRUCTIONS
            .iter()
            .zip(crate::tms9900::rows())
        {
            assert_eq!(row.mnemonic, insn.mnemonic);
            assert_eq!(row.mode, insn.class.name());
            assert!(!row.mode.is_empty());
        }
    }

    /// The Z8000 is thirteen tables, and every one of them contributes.
    ///
    /// A chain that silently dropped a family would look exactly like a spec
    /// that never had it — which is the failure #225 is a case of, one layer
    /// down. So each table is asserted present by a mnemonic only it declares.
    #[test]
    fn every_z8000_family_reaches_the_rows() {
        let rows: Vec<crate::Row> = crate::z8000::rows().collect();
        let has = |m: &str| rows.iter().any(|r| r.mnemonic == m);
        for (family, mnemonic) in [
            ("dyadic", crate::z8000::INSTRUCTIONS[0].mnemonic),
            ("control transfer", crate::z8000::CONTROL[0].mnemonic),
            ("monadic", crate::z8000::MONO[0].mnemonic),
            ("stack", crate::z8000::STACK[0].mnemonic),
            ("shift", crate::z8000::SHIFTS[0].mnemonic),
            ("extend", crate::z8000::EXTENDS[0].mnemonic),
            ("bit", crate::z8000::BITS[0].mnemonic),
            ("multiply/divide", crate::z8000::MULDIV[0].mnemonic),
            ("block", crate::z8000::BLOCK[0].mnemonic),
            ("simple I/O", crate::z8000::SIMPLE_IO[0].mnemonic),
            ("block I/O", crate::z8000::BLOCK_IO[0].mnemonic),
            ("control", crate::z8000::CONTROLS[0].mnemonic),
            ("misc", crate::z8000::MISC[0].mnemonic),
        ] {
            assert!(has(mnemonic), "the {family} family reaches no row");
        }
        let entries = crate::z8000::INSTRUCTIONS.len()
            + crate::z8000::CONTROL.len()
            + crate::z8000::MONO.len()
            + crate::z8000::STACK.len()
            + crate::z8000::SHIFTS.len()
            + crate::z8000::EXTENDS.len()
            + crate::z8000::BITS.len()
            + crate::z8000::MULDIV.len()
            + crate::z8000::BLOCK.len()
            + crate::z8000::SIMPLE_IO.len()
            + crate::z8000::BLOCK_IO.len()
            + crate::z8000::CONTROLS.len()
            + crate::z8000::MISC.len();
        assert!(
            rows.len() >= entries,
            "the two mode-bearing families expand, so rows ({}) cannot be fewer \
             than entries ({entries})",
            rows.len()
        );
    }

    /// An exemplar is exactly as long as the form says it is.
    ///
    /// [`Form::len`] is what the assembler advances the program counter by, and
    /// an exemplar that disagreed with it would be a different instruction from
    /// the one the row names — which the audit would then arbitrate under the
    /// wrong name.
    #[test]
    fn an_exemplar_is_as_long_as_the_form_claims() {
        let mut checked = 0usize;
        for set in [
            &crate::z80::SET,
            &crate::mos6502::SET,
            &crate::sm83::SET,
            &crate::i8080::SET,
            &crate::m6800::SET,
            &crate::mos65816::SET,
        ] {
            for insn in set.instructions {
                for form in insn.forms {
                    assert_eq!(
                        form.exemplar().count(),
                        form.len(),
                        "{} {}",
                        insn.mnemonic,
                        form.mode
                    );
                    checked += 1;
                }
            }
        }
        assert!(checked > 0);
    }

    /// An exemplar starts with the opcode and ends with the suffix, so the
    /// operand values sit where the encoding puts them. The Z80's `DD CB`
    /// group is the case that matters: its final opcode byte comes *after* the
    /// displacement operand, and an exemplar that appended the suffix in the
    /// wrong place would encode a different instruction.
    #[test]
    fn an_exemplar_keeps_the_suffix_last() {
        let form = crate::z80::SET
            .instructions
            .iter()
            .flat_map(|i| i.forms)
            .find(|f| !f.suffix.is_empty())
            .expect("the Z80 declares suffixed forms");
        let bytes: Vec<u8> = form.exemplar().collect();
        assert!(bytes.starts_with(form.opcode), "{bytes:02X?}");
        assert!(bytes.ends_with(form.suffix), "{bytes:02X?}");
        assert_eq!(bytes.len(), form.len());
    }

    /// A row carries its form's `undocumented` flag rather than inventing one,
    /// so a marked form stays marked through the seam.
    ///
    /// The two CPUs that declare undocumented rows treat them differently on
    /// output, and deliberately: the Z80's `SLL` is a working instruction that
    /// real software uses, so it disassembles; the 6809's three do not, so they
    /// are input-only (`decisions/undocumented-opcodes-are-input-only.md`).
    /// The flag is the same either way — what a consumer does with it is the
    /// consumer's call.
    #[test]
    fn rows_carry_the_undocumented_marker() {
        let marked = crate::z80::SET.rows().filter(|r| r.undocumented).count();
        assert_eq!(marked, 8, "the Z80 declares eight undocumented forms");

        let six = |name| {
            crate::mos6809::rows()
                .filter(|r| r.undocumented && r.mnemonic == name)
                .count()
        };
        assert_eq!(
            crate::mos6809::rows().filter(|r| r.undocumented).count(),
            3,
            "the 6809 declares `reset`, `rhf` and `hcf`"
        );
        for name in ["reset", "rhf", "hcf"] {
            assert_eq!(six(name), 1, "{name} is one inherent row");
        }
        // And nothing documented was swept up by the marker.
        for name in ["nop", "swi", "sync"] {
            assert_eq!(six(name), 0, "{name} is documented");
        }
    }
}
