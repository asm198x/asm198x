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
//! TODO: sjasmplus modules, macros, and `DUP`.

use std::collections::BTreeMap;

use crate::dialect::{Dialect, Oversize};
use crate::dialects::z80::{self, Z80Syntax};
use crate::engine::{AsmError, Operation, Statement};

/// The sjasmplus Z80 dialect. `z80n` selects the target instruction set
/// (sjasmplus emits Z80N when targeting the Next).
pub(crate) struct Sjasmplus {
    pub(crate) z80n: bool,
}

impl Dialect for Sjasmplus {
    fn instruction_set(&self) -> &'static isa::InstructionSet {
        &isa::z80::SET
    }
    fn extension_set(&self) -> Option<&'static isa::InstructionSet> {
        self.z80n.then_some(&isa::z80::NEXT)
    }
    fn parse(&self, source: &str) -> Result<Vec<Statement>, AsmError> {
        z80::assemble(
            &SjasmplusSyntax,
            self.instruction_set(),
            self.extension_set(),
            source,
        )
    }
    /// sjasmplus truncates an over-range byte to its low 8 bits and warns.
    fn oversized_byte_policy(&self) -> Oversize {
        Oversize::TruncateWarn
    }
}

/// sjasmplus's surface syntax.
struct SjasmplusSyntax;

impl Z80Syntax for SjasmplusSyntax {
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

    /// sjasmplus adds `byte` as a spelling of `db` (pasmo has neither).
    fn is_directive(&self, word: &str) -> bool {
        word.eq_ignore_ascii_case("byte") || z80::is_common_directive(word)
    }

    /// `byte` is `db`; everything else is the shared common set.
    fn parse_directive(
        &self,
        word: &str,
        args: &str,
        line: usize,
        consts: &BTreeMap<String, i64>,
    ) -> Result<Option<Operation>, AsmError> {
        let word = if word.eq_ignore_ascii_case("byte") {
            "db"
        } else {
            word
        };
        z80::common_directive(self, word, args, line, consts)
    }

    /// sjasmplus scopes leading-`.` labels under the most recent global label.
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

#[cfg(test)]
mod tests {
    use crate::assemble_sjasmplus as asm;

    #[test]
    fn number_formats() {
        // All of these are $1234.
        for src in ["ld hl, $1234", "ld hl, 0x1234", "ld hl, 1234h"] {
            assert_eq!(asm(src).expect(src).bytes, vec![0x21, 0x34, 0x12], "{src}");
        }
        // All of these are %1010 = 0x0A.
        for src in ["ld a, %1010", "ld a, 0b1010", "ld a, 1010b"] {
            assert_eq!(asm(src).expect(src).bytes, vec![0x3E, 0x0A], "{src}");
        }
    }

    #[test]
    fn slash_slash_comment() {
        assert_eq!(
            asm("ld a, 5  // a comment\n").expect("//").bytes,
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
}
