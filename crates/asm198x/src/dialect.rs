//! The dialect front-end abstraction.
//!
//! A [`Dialect`] is one source syntax: it tokenises its own directives,
//! literals, operators, and label rules, and resolves each instruction's
//! addressing mode against a target [`isa::InstructionSet`] — producing the
//! engine's generic [`Statement`](crate::engine::Statement) stream. Encoding
//! lives in the `isa` spec; the engine lays bytes down. Dialect is an axis
//! independent of CPU: several dialects may target the same spec (acme and
//! ca65 both emit 6502), and one dialect may target several (vasm covers more
//! than one CPU). See `decisions/syntax-stance.md`.

use crate::engine::{AsmError, Statement, Warning};
use crate::source::{SourceLoader, SourceMap};
use crate::span::FileId;

/// What a dialect does with a value too large for the byte operand it's emitted
/// into. The 6502/6809 assemblers (ACME, ca65, lwasm) treat it as an error; the
/// Z80 ones accept it and keep the low 8 bits — pasmo silently, sjasmplus with a
/// non-fatal warning.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Oversize {
    Error,
    Truncate,
    TruncateWarn,
}

pub(crate) use crate::engine::CycleCoverage;

pub(crate) trait Dialect {
    /// The primary instruction set this dialect assembles against.
    fn instruction_set(&self) -> &'static isa::InstructionSet;

    /// An optional extension set whose forms are *also* available — e.g. the
    /// Z80N opcodes a PasmoNext dialect adds on top of standard Z80. A dialect
    /// without one (the default) rejects those opcodes as unknown.
    fn extension_set(&self) -> Option<&'static isa::InstructionSet> {
        None
    }

    /// What an empty cycle capture means for this dialect — see
    /// [`CycleCoverage`]. The default is the honest floor: a new dialect
    /// declares coverage only once its instruction lowering warrants it.
    fn cycle_coverage(&self) -> CycleCoverage {
        CycleCoverage::None
    }

    /// Parse source into the engine's statement stream, resolving each
    /// instruction's addressing mode (so form sizes are stable across passes).
    ///
    /// # Errors
    /// Returns an [`AsmError`] on any tokenising or mode-resolution failure.
    fn parse(&self, source: &str) -> Result<Vec<Statement>, AsmError>;

    /// [`parse`](Self::parse), plus any non-fatal advisories the parse raised.
    ///
    /// The default returns none, so a dialect that has nothing to say is
    /// unchanged. This exists because several references *warn* where they
    /// could refuse — sjasmplus on a forward reference in a condition and on a
    /// label whose value never settled, ACME on an oversized addressing mode —
    /// and reproducing a reference's bytes without its warning is only half of
    /// matching it. Reaching one of those without the channel would mean
    /// shipping the same questionable binary in silence.
    ///
    /// # Errors
    /// As [`parse`](Self::parse).
    fn parse_warned(&self, source: &str) -> Result<(Vec<Statement>, Vec<Warning>), AsmError> {
        self.parse(source).map(|s| (s, Vec::new()))
    }

    /// Parse a multi-file program (language-surface U2, KTD8): the root is
    /// `FileId(0)` in `map`, and an include-capable dialect resolves its
    /// include directives through `loader`, minting further ids in `map` as
    /// the walk reaches them live (KTD1). The default is the single-file
    /// behaviour — parse the root, no include resolution — so a dialect gains
    /// includes by overriding this (sjasmplus in U2; the rest in U4–U6).
    ///
    /// # Errors
    /// As [`parse`](Self::parse), plus include-resolution failures (missing
    /// target, cycle, depth) at the directive's span.
    /// [`parse_multi`](Self::parse_multi), plus its advisories — the
    /// multi-file half of [`parse_warned`](Self::parse_warned), defaulted the
    /// same way.
    ///
    /// # Errors
    /// As [`parse_multi`](Self::parse_multi).
    fn parse_multi_warned(
        &self,
        map: &mut SourceMap,
        loader: &dyn SourceLoader,
    ) -> Result<(Vec<Statement>, Vec<Warning>), AsmError> {
        self.parse_multi(map, loader).map(|s| (s, Vec::new()))
    }

    fn parse_multi(
        &self,
        map: &mut SourceMap,
        loader: &dyn SourceLoader,
    ) -> Result<Vec<Statement>, AsmError> {
        let _ = loader;
        let root = map
            .contents(FileId(0))
            .map(str::to_owned)
            .unwrap_or_default();
        self.parse(&root)
    }

    /// Parse into the semantic AST (`crate::ast`) — the source-preserving tree
    /// the formatter and bidirectional emit consume (U5). Defaults to `None`: a
    /// dialect without an AST front-end has no formatter yet and stays on
    /// [`parse`](Self::parse) for assembly. The Z80 dialects override it.
    ///
    /// # Errors
    /// Returns an [`AsmError`] on any parse failure.
    fn parse_ast(&self, _source: &str) -> Result<Option<crate::ast::Program>, AsmError> {
        Ok(None)
    }

    /// Whether emitting bytes before any origin is set is an error. ACME's `*=`
    /// is mandatory before code or data — it rejects an implicit origin with
    /// "Program counter undefined" — so a forgotten `*=` fails loudly rather than
    /// silently assembling at `$0000`. Off by default: a flat binary at origin 0
    /// is a legitimate default for the Z80/6809 tools (`org` optional).
    fn requires_explicit_origin(&self) -> bool {
        false
    }

    /// Whether a second `org` moves the *output* as well as the address.
    ///
    /// Most references here mean "put the next byte at this address", so an
    /// `org` above the current position pads the gap and one below it is an
    /// error. lwasm's flat output means the other thing: `org` sets the
    /// address the code claims and the bytes keep landing where they were,
    /// contiguous — `org $1000 / fcb 1 / org $2000 / fcb 2` is two bytes, not
    /// four thousand and one, and an `org` below the current address is
    /// ordinary rather than refused (probed against lwtools 4.25 with
    /// `--raw`).
    ///
    /// On by default, which is the padding meaning.
    fn org_moves_output(&self) -> bool {
        true
    }

    /// Whether each `org` boundary starts another address-placed run. ACME
    /// builds one memory image from source-ordered regions, then places them by
    /// address; keeping the regions separate also prevents an unwritten
    /// forward gap from overwriting an earlier region it crosses. Most flat
    /// assemblers retain the default `false` and require monotonically
    /// increasing origins.
    fn org_starts_address_run(&self) -> bool {
        false
    }

    /// Whether a later address run overwrites bytes already placed there.
    /// Meaningful only with [`Self::org_starts_address_run`]. ACME warns and
    /// lets the later region win; section/linker dialects refuse overlaps.
    fn later_run_overwrites(&self) -> bool {
        false
    }

    /// Whether a backward `org` discards an initial region containing only
    /// reservations. lwasm raw output starts at the first region that writes
    /// bytes; other dialects retain their normal gap semantics.
    fn org_drops_unwritten_prefix(&self) -> bool {
        false
    }

    /// Whether a value may be written as a **negative** number, at any width.
    ///
    /// The accepted range for `n` bytes is either `-(2^(8n-1))..=2^(8n)-1` —
    /// signed or unsigned, whichever the source meant — or `0..=2^(8n)-1`,
    /// with no negatives at all. That is a property of the reference tool,
    /// not of the width, and the tools genuinely split on it (probed
    /// 2026-08-25):
    ///
    /// | tool | `-1` as a byte or word |
    /// |---|---|
    /// | acme, asl, lwasm, vasm, sjasmplus, pasmo, rgbasm | `ff` / `ff ff` |
    /// | ca65 | `Range error (-1 not in [0..255])` |
    ///
    /// ca65 is the only one that refuses, and it refuses everywhere — data
    /// directives and instruction operands alike, at byte, word and dword.
    /// So this is one answer per dialect rather than one per directive.
    ///
    /// Defaults to `true`, which is six of the seven references. Returning
    /// `false` is not a stricter setting to be preferred on taste: it makes
    /// the assembler refuse source, so it must be what the reference does.
    fn accepts_negative_values(&self) -> bool {
        true
    }

    /// How to handle a value outside the accepted range in a **data
    /// directive** (`!byte`, `defw`, `fcb`), at byte or word width. Defaults
    /// to [`Oversize::Error`]; the tools that truncate override it (pasmo
    /// silently, sjasmplus and rgbasm with a warning).
    fn oversized_byte_policy(&self) -> Oversize {
        Oversize::Error
    }

    /// The same question for an **instruction operand**, which is not always
    /// the same answer. Defaults to whatever the data directives do, because
    /// six of the seven references treat them alike.
    ///
    /// lwasm is the exception, and only probing found it: `fcb $1ff` truncates
    /// to `ff` without a word, while `ldb #$1ff` is a hard `Byte overflow`.
    /// Same tool, same width, same value, opposite answers — so the two
    /// questions cannot share one method (probed 2026-08-25, asm198x#290).
    fn oversized_operand_policy(&self) -> Oversize {
        self.oversized_byte_policy()
    }

    /// Whether the formatter keeps a colon on an `equ` label (`name: equ …`).
    /// Defaults to `true` (the Z80 dialects): a bare `equ` label whose spelling
    /// collides with a mnemonic re-parses as an instruction, so the colon forces
    /// it to stay a label. The Intel-8080 dialect overrides to `false` — its
    /// `equ` keyword already disambiguates the label, and a colon (`name: equ …`)
    /// fails to reassemble there. Only consulted by [`crate::ast::emit`].
    fn equ_label_colon(&self) -> bool {
        true
    }

    /// The number of emitted bytes per **address unit** — how the location
    /// counter (labels, `*`/`$`, `org`) relates to the byte stream. Almost every
    /// CPU is byte-addressed, so this is `1`. The **CP1610** is *word*-addressed:
    /// its 10-bit "decle" is stored as a 2-byte word and `asl` counts addresses
    /// in decles, so labels advance by one per two bytes emitted — it returns `2`.
    /// Code must be a whole number of units long (the CP1610's is always
    /// decle-aligned).
    fn addr_unit(&self) -> i64 {
        1
    }

    /// Where this dialect's image begins, when its container fixes that rather
    /// than the program's own lowest section.
    ///
    /// A Game Boy ROM starts at file offset 0 whatever the program put there,
    /// so a program of one section at `$10` still emits the leading sixteen
    /// bytes. A flat dialect answers `None` and the image starts wherever the
    /// program does.
    fn image_base(&self) -> Option<i64> {
        None
    }

    /// The size this dialect's toolchain pads a finished image to, given the
    /// bytes placed so far, or `None` to leave it as laid out.
    ///
    /// Only a dialect whose container has a shape of its own answers: a Game
    /// Boy ROM is a whole number of `$4000` banks, so `rgblink` writes
    /// `(highest bank + 1) * $4000` bytes. Every flat dialect leaves the image
    /// exactly as long as what was written.
    fn image_size(&self, _image: &[u8]) -> Option<usize> {
        None
    }

    /// The values this dialect's toolchain accepts for an `equ`/`=` constant,
    /// or `None` for no constraint.
    ///
    /// A property of the toolchain, not of the CPU. Probed against every
    /// reference installed here: acme, sjasmplus, rgbasm, vasm, lwasm and ca65
    /// all take `$12345678`, and acme, sjasmplus, rgbasm, vasm and lwasm all
    /// take negatives. **pasmo alone constrains**, and only upward — `$FFFF`
    /// assembles, `$10000` does not, while `-65536` is fine.
    ///
    /// So the default is no constraint, which matches six of the seven, and
    /// pasmo narrows it. The engine previously checked every dialect against
    /// `0..=0xFF_FFFF` — a 65816 long address applied to all twenty-one —
    /// which refused source five references assemble, in both directions
    /// (#228).
    fn equ_range(&self) -> Option<std::ops::RangeInclusive<i64>> {
        None
    }

    /// The byte that fills space the source reserved but did not define — an
    /// `org` gap, or a `ds`/`rmb`/`res`/`block` reservation.
    ///
    /// This is a property of the dialect's **toolchain**, not of the CPU, which
    /// is why it lives here and not in the `isa` spec: the same 8080 program
    /// assembled by asl and converted with `p2bin` reserves `$FF`, while pasmo,
    /// sjasmplus, acme, lwasm and rgbasm all reserve `$00`. Defaults to `0x00`;
    /// the asl-family dialects override it, since `p2bin` fills the gaps asl
    /// leaves with `$FF`.
    fn gap_fill(&self) -> u8 {
        0x00
    }

    /// Whether the image ends at the last byte the source actually *wrote*, so
    /// space reserved past that point is absent rather than filled.
    ///
    /// The two toolchain models differ here, and the difference is only visible
    /// on a trailing reservation. pasmo, sjasmplus and lwasm **materialise** a
    /// reservation — `ds 3` at the end of a program contributes three `$00`
    /// bytes to the image. asl **reserves** it: nothing is written, and `p2bin`
    /// materialises only the gaps that fall *inside* the written range, so a
    /// trailing `ds` contributes nothing and the binary simply ends earlier.
    ///
    /// Defaults to `false` (materialise); the asl-family dialects override it.
    ///
    /// # Known remaining divergence: a *leading* reservation
    ///
    /// `p2bin` writes from the lowest written address to the highest, so a
    /// reservation *before* any data shifts the image's start rather than
    /// filling: `ds 3` then `db 9` gives a one-byte file loading at 3, where we
    /// emit `FF FF FF 09` at the origin. Closing that means advancing
    /// [`Assembly::origin`](crate::engine::Assembly) past a leading gap, which
    /// reaches the container writers and the debug sidecar's offsets — larger
    /// than this fill fix, and tracked in
    /// [#90](https://github.com/asm198x/asm198x/issues/90). Interior and
    /// trailing reservations, the cases #66 was filed for, match byte-for-byte.
    fn trims_trailing_gap(&self) -> bool {
        false
    }
}
