//! The pasmo-family Z80 dialect (vanilla pasmo and PasmoNext).
//!
//! A thin surface over the shared Z80 core in [`crate::dialects::z80`]: pasmo's
//! comment style (`;`) and number formats (`$hex`, `%binary`, decimal, `'c'`)
//! are all that differ from the shared operand resolution, expression parser,
//! and directives. The `z80n` flag selects the **target** instruction set —
//! plain Z80 (vanilla pasmo) or the Spectrum Next's Z80N (pasmonext) — which is
//! a target property, not a syntax one (see `decisions/syntax-stance.md`).

use crate::dialect::{Dialect, Oversize};
use crate::dialects::z80::{self, Z80Syntax};
use crate::engine::{AsmError, Statement};
use crate::source::{SourceLoader, SourceMap};

/// The pasmo-family Z80 dialect. `z80n` selects the target: `false` for a plain
/// Z80 (vanilla pasmo), `true` for the Spectrum Next's Z80N (pasmonext).
pub(crate) struct Pasmo {
    pub(crate) z80n: bool,
}

impl Dialect for Pasmo {
    fn instruction_set(&self) -> &'static isa::InstructionSet {
        &isa::z80::SET
    }
    fn extension_set(&self) -> Option<&'static isa::InstructionSet> {
        self.z80n.then_some(&isa::z80::NEXT)
    }
    fn parse(&self, source: &str) -> Result<Vec<Statement>, AsmError> {
        z80::assemble(
            &PasmoSyntax,
            self.instruction_set(),
            self.extension_set(),
            source,
        )
    }
    fn parse_ast(&self, source: &str) -> Result<Option<crate::ast::Program>, AsmError> {
        Ok(Some(z80::parse_program(
            &PasmoSyntax,
            self.instruction_set(),
            self.extension_set(),
            source,
        )?))
    }
    /// The incbin-capable parse (language-surface U3): the same
    /// environment-threaded walk as sjasmplus's, resolving `incbin` lazily
    /// through the loader. pasmo's `include` is *not* recognised yet (U4), so
    /// includes still error exactly as on the single-file path.
    fn parse_multi(
        &self,
        map: &mut SourceMap,
        loader: &dyn SourceLoader,
    ) -> Result<Vec<Statement>, AsmError> {
        crate::ast::lower(z80::parse_program_multi(
            &PasmoSyntax,
            self.instruction_set(),
            self.extension_set(),
            map,
            loader,
        )?)
    }
    /// pasmo silently truncates an over-range byte to its low 8 bits.
    fn oversized_byte_policy(&self) -> Oversize {
        Oversize::Truncate
    }
}

/// pasmo's surface syntax.
struct PasmoSyntax;

impl Z80Syntax for PasmoSyntax {
    fn strip_comment<'a>(&self, line: &'a str) -> &'a str {
        line.find(';').map_or(line, |idx| &line[..idx])
    }

    /// pasmo's `incbin` (language-surface U3), listed so a column-0 spelling
    /// reads as an operation, not a label. `include` waits for U4.
    fn is_directive(&self, word: &str) -> bool {
        self.is_incbin(word) || z80::is_common_directive(word)
    }

    /// pasmo's binary-inclusion directive (language-surface U3),
    /// walk-handled. Probe-pinned to the **plain form only**: the trait's
    /// defaults keep the `,offset[,length]` tail a parse error (pasmo:
    /// `End line expected but ','found`) and `<file>` a literal file name.
    fn is_incbin(&self, word: &str) -> bool {
        word.eq_ignore_ascii_case("incbin")
    }

    /// pasmo numbers: `$hex`/`0xhex`, `%binary`, `'c'` char, decimal, and the
    /// radix *suffixes* `h` (hex), `b` (binary), `o`/`q` (octal).
    fn parse_number(&self, tok: &str, line: usize) -> Result<i64, AsmError> {
        let t = tok.trim();
        let bad = || AsmError::new(line, format!("invalid number `{tok}`"));
        if let Some(hex) = t
            .strip_prefix('$')
            .or_else(|| t.strip_prefix("0x"))
            .or_else(|| t.strip_prefix("0X"))
        {
            i64::from_str_radix(hex, 16).map_err(|_| bad())
        } else if let Some(bin) = t.strip_prefix('%') {
            i64::from_str_radix(bin, 2).map_err(|_| bad())
        } else if t.starts_with('\'') && t.ends_with('\'') && t.chars().count() == 3 {
            t.chars().nth(1).map(|c| c as i64).ok_or_else(bad)
        } else {
            // A trailing radix letter (`h`/`b`/`o`/`q`) selects the base;
            // otherwise the token is decimal.
            let (body, radix) = match t.chars().last() {
                Some('h' | 'H') => (&t[..t.len() - 1], 16),
                Some('b' | 'B') => (&t[..t.len() - 1], 2),
                Some('o' | 'O' | 'q' | 'Q') => (&t[..t.len() - 1], 8),
                _ => (t, 10),
            };
            i64::from_str_radix(body, radix).map_err(|_| bad())
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::assemble_pasmonext as asm;

    /// U4 — comments are carried as AST trivia (leading own-line + trailing
    /// same-line), not stripped, and do not change the emitted bytes (AE1).
    #[test]
    fn comments_are_carried_as_trivia() {
        use super::{Pasmo, PasmoSyntax};
        use crate::dialect::Dialect;
        use crate::dialects::z80;

        let src = "; header\nstart:\n  ld a, 5   ; load five\n  ret\n";
        let d = Pasmo { z80n: false };
        let prog = z80::parse_program(&PasmoSyntax, d.instruction_set(), d.extension_set(), src)
            .expect("parses");

        // The header comment is leading trivia on the first node (`start:`).
        assert!(
            prog.nodes[0]
                .trivia
                .leading
                .iter()
                .any(|c| c.text == "; header"),
            "own-line comment attaches as leading trivia"
        );
        // The `ld a,5` line carries its same-line comment as trailing trivia.
        assert!(
            prog.nodes.iter().any(|n| n
                .trivia
                .trailing
                .as_ref()
                .is_some_and(|c| c.text == "; load five")),
            "same-line comment attaches as trailing trivia"
        );
        // Comments never reach the encoder — bytes are unchanged.
        let with = crate::assemble_pasmo(src).expect("assembles").bytes;
        let without = crate::assemble_pasmo("start:\n  ld a, 5\n  ret\n")
            .expect("assembles")
            .bytes;
        assert_eq!(with, without, "comments do not change bytes");
    }

    #[test]
    fn oversized_byte_truncates_silently() {
        // pasmo keeps the low 8 bits of an over-range byte immediate and does
        // not warn (byte-identical to pasmo: `ld a,$1234` -> 3e 34).
        let a = crate::assemble_pasmo("        ld a,$1234\n").expect("truncate");
        assert_eq!(a.bytes, vec![0x3E, 0x34]);
        assert!(a.warnings.is_empty());
        // `defb` truncates too.
        assert_eq!(
            crate::assemble_pasmo("        defb $1234\n")
                .expect("defb")
                .bytes,
            vec![0x34]
        );
    }

    #[test]
    fn loads_registers_and_immediates() {
        assert_eq!(asm("ld a, 0").expect("ld a,0").bytes, vec![0x3E, 0x00]);
        assert_eq!(asm("ld a, c").expect("ld a,c").bytes, vec![0x79]);
        // 16-bit immediate, little-endian.
        assert_eq!(
            asm("ld hl, $5800").expect("ld hl").bytes,
            vec![0x21, 0x00, 0x58]
        );
        assert_eq!(
            asm("ld bc, 767").expect("ld bc").bytes,
            vec![0x01, 0xFF, 0x02]
        );
        assert_eq!(
            asm("ld (hl), $0F").expect("ld (hl),n").bytes,
            vec![0x36, 0x0F]
        );
    }

    #[test]
    fn port_io_uses_eight_bit_operand() {
        assert_eq!(asm("out ($FE), a").expect("out").bytes, vec![0xD3, 0xFE]);
        assert_eq!(asm("in a, ($FE)").expect("in").bytes, vec![0xDB, 0xFE]);
    }

    #[test]
    fn sixteen_bit_add_and_indirect() {
        assert_eq!(asm("add hl, de").expect("add").bytes, vec![0x19]);
        assert_eq!(asm("ld a, (bc)").expect("ld a,(bc)").bytes, vec![0x0A]);
        assert_eq!(
            asm("ld ($5800), hl").expect("ld (nn),hl").bytes,
            vec![0x22, 0x00, 0x58]
        );
    }

    #[test]
    fn equ_defines_a_constant() {
        let a = asm("COBBLE equ %00000001\n        ld (hl), COBBLE\n").expect("equ");
        assert_eq!(a.bytes, vec![0x36, 0x01]);
        assert_eq!(a.symbols.get("COBBLE"), Some(&0x0001));
    }

    #[test]
    fn relative_jumps_resolve_against_labels() {
        let a = asm("        org $8000\n.loop:  nop\n        jr .loop\n").expect("jr");
        assert_eq!(a.bytes, vec![0x00, 0x18, 0xFD]);
        assert_eq!(a.symbols.get(".loop"), Some(&0x8000));
    }

    #[test]
    fn location_counter_is_statement_start() {
        // `$` is the address of the current statement. Validated byte-for-byte
        // against the pasmonext binary.
        let a = asm("        org $8000\n        jr $\n        ld hl,$\n        jp $+3\n        dw $\n        ld bc,$-1\n")
            .expect("location counter");
        assert_eq!(
            a.bytes,
            vec![
                0x18, 0xFE, 0x21, 0x02, 0x80, 0xC3, 0x08, 0x80, 0x08, 0x80, 0x01, 0x09, 0x80
            ]
        );
    }

    #[test]
    fn dollar_hex_is_still_a_number() {
        // `$5800` is a hex literal, not `$` (PC) followed by `5800`.
        assert_eq!(
            asm("ld hl, $5800").expect("hex").bytes,
            vec![0x21, 0x00, 0x58]
        );
    }

    #[test]
    fn condition_codes_disambiguate_from_registers() {
        assert!(asm("jr c, $0000").is_ok());
        assert_eq!(asm("ld b, c").expect("ld b,c").bytes, vec![0x41]);
        assert_eq!(asm("ret c").expect("ret c").bytes, vec![0xD8]);
        assert_eq!(asm("ret nc").expect("ret nc").bytes, vec![0xD0]);
    }

    #[test]
    fn arithmetic_respects_c_precedence() {
        // $5800 + 23*32 = $5AE0.
        assert_eq!(
            asm("ld hl, $5800 + 23*32").expect("precedence").bytes,
            vec![0x21, 0xE0, 0x5A]
        );
        assert_eq!(
            asm("ld hl, (1+2)*3").expect("parens").bytes,
            vec![0x21, 0x09, 0x00]
        );
        let a = asm("ROW equ 64\n        ld a, ROW / 8\n").expect("div");
        assert_eq!(a.bytes, vec![0x3E, 0x08]);
    }

    #[test]
    fn im_selects_mode_by_literal() {
        assert_eq!(asm("        im 1\n").expect("im 1").bytes, vec![0xED, 0x56]);
        assert_eq!(asm("        im 2\n").expect("im 2").bytes, vec![0xED, 0x5E]);
    }

    #[test]
    fn defs_reserves_zero_bytes() {
        assert_eq!(asm("        ds 4\n").expect("ds").bytes, vec![0, 0, 0, 0]);
    }

    #[test]
    fn ed_block_move_assembles() {
        assert_eq!(asm("        ldir\n").expect("ldir").bytes, vec![0xED, 0xB0]);
    }

    #[test]
    fn base_pasmo_and_pasmonext_agree_on_standard_z80() {
        let src = "        org $8000\n.loop:  ld a, 0\n        ldir\n        bit 7,(hl)\n        jr .loop\n";
        let base = crate::assemble_pasmo(src).expect("pasmo");
        let next = crate::assemble_pasmonext(src).expect("pasmonext");
        assert_eq!(base.bytes, next.bytes);
    }

    #[test]
    fn defb_string_expands_to_char_bytes() {
        assert_eq!(
            asm("        defb \"AB\", 0\n").expect("defb").bytes,
            vec![0x41, 0x42, 0x00]
        );
    }

    #[test]
    fn ds_count_folds_a_constant_expression() {
        // `ds`/`defs` accepts a literal or an expression of `equ` constants
        // (e.g. `ds MAX*2`), not only a bare number — matching pasmo (#34).
        assert_eq!(asm("        ds 3\n").expect("literal").bytes, vec![0, 0, 0]);
        let a = asm("MAX equ 2\n        defs MAX*2\n").expect("expr count");
        assert_eq!(a.bytes, vec![0, 0, 0, 0]);
        // A non-constant (undefined symbol) count is still an error.
        assert!(asm("        ds nope\n").is_err());
    }

    #[test]
    fn cb_bit_ops_assemble() {
        assert_eq!(
            asm("        bit 7,(hl)\n").expect("bit").bytes,
            vec![0xCB, 0x7E]
        );
        assert_eq!(
            asm("        set 0,a\n").expect("set").bytes,
            vec![0xCB, 0xC7]
        );
        assert_eq!(asm("        rlc b\n").expect("rlc").bytes, vec![0xCB, 0x00]);
    }

    #[test]
    fn ix_iy_ops_assemble() {
        assert_eq!(
            asm("        push ix\n").expect("push ix").bytes,
            vec![0xDD, 0xE5]
        );
        assert_eq!(
            asm("        ld a,(ix+5)\n").expect("ld a,(ix+d)").bytes,
            vec![0xDD, 0x7E, 0x05]
        );
        assert_eq!(
            asm("        ld (ix+5),$0a\n").expect("ld (ix+d),n").bytes,
            vec![0xDD, 0x36, 0x05, 0x0A]
        );
        assert_eq!(
            asm("        bit 7,(iy-1)\n").expect("bit (iy+d)").bytes,
            vec![0xFD, 0xCB, 0xFF, 0x7E]
        );
    }

    #[test]
    fn z80n_opcodes_follow_the_target_not_the_dialect() {
        assert_eq!(
            crate::assemble_pasmonext("        swapnib\n")
                .expect("z80n on")
                .bytes,
            vec![0xED, 0x23]
        );
        let err = crate::assemble_pasmo("        swapnib\n").expect_err("z80n off");
        assert!(err.message.contains("SWAPNIB"), "unexpected: {err}");
    }
}
