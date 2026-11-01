//! What is left of Asm198x's own ca65 reader once Lang198x's front end and
//! the walk in [`super::ca65_walk`] took over every CPU: the directive
//! surface the CLI declares, and the reader's unit tests, which now check
//! the walk.

use crate::directives::{Category, Directive, Pattern};

/// Assemble to bytes alone, through the walk — the unit tests' entry. The
/// tests were written against this reader's assembler, which the walk has
/// replaced, so they now check the walk.
#[cfg(test)]
pub(crate) fn assemble(
    source: &str,
) -> Result<(Vec<u8>, Vec<crate::engine::Warning>), crate::engine::AsmError> {
    crate::assemble_ca65(source).map(|result| (result.bytes, result.warnings))
}

/// What this dialect accepts beyond the 6502 instruction set.
///
/// The `.` is required: a bare `byte` is a *label definition* in ca65, and real
/// ca65 answers "':' expected" for it. `Sigilled { required: true }` is what
/// keeps that true — see `crate::directives`.
///
/// Dispatch is split, and the declaration covers all of it because it describes
/// the dialect rather than any one parser. The data directives reach
/// [`parse_directive`]; `.include` and `.incbin` are walk-handled; `.segment`
/// is read where segments are assigned; the macro spellings are expanded before
/// parsing.
pub const DIRECTIVES: &[Directive] = &[
    // ca65's expression vocabulary: words that appear *inside* an expression
    // and never begin a line. Declared because the ledger counts what is
    // declared, and leaving these out made eleven working words read as gaps
    // — see `Category::ExpressionWord`.
    //
    // `.max`, `.min` and `.strat` were unreachable until the data-list
    // splitter stopped cutting a call in half at its argument comma; the code
    // behind them was always right.
    // The operator spellings. ca65 writes every operator twice — `&` and
    // `.bitand` are one operator — so the keyword lands on the same token as
    // its symbol twin and inherits its precedence. `.mod` is the exception
    // with no twin: `%` is a binary literal there.
    //
    // `.not` is the one whose place is not the obvious one: it binds looser
    // than everything, so `.not 1 .or 1` is `0`.
    Directive {
        id: "operator",
        pattern: Pattern::Exact(&[
            ".and", ".bitand", ".bitnot", ".bitor", ".bitxor", ".mod", ".not", ".or", ".shl",
            ".shr", ".xor",
        ]),
        category: Category::ExpressionWord,
    },
    Directive {
        id: "expression-word",
        pattern: Pattern::Exact(&[
            ".asize",
            ".bankbyte",
            ".blank",
            ".concat",
            ".const",
            ".def",
            ".defined",
            ".definedmacro",
            ".hibyte",
            ".hiword",
            ".ident",
            ".isize",
            ".ismnem",
            ".ismnemonic",
            ".left",
            ".match",
            ".mid",
            ".right",
            ".sprintf",
            ".lobyte",
            ".loword",
            ".max",
            ".min",
            ".paramcount",
            ".strat",
            ".string",
            ".strlen",
            ".tcount",
            ".xmatch",
        ]),
        category: Category::ExpressionWord,
    },
    Directive {
        id: "bytes",
        pattern: Pattern::Sigilled {
            sigil: '.',
            names: &["byte", "byt"],
            required: true,
        },
        category: Category::Operation,
    },
    Directive {
        id: "words",
        pattern: Pattern::Sigilled {
            sigil: '.',
            names: &["word", "addr"],
            required: true,
        },
        category: Category::Operation,
    },
    Directive {
        id: "dbyt",
        pattern: Pattern::Sigilled {
            sigil: '.',
            names: &["dbyt"],
            required: true,
        },
        category: Category::Operation,
    },
    Directive {
        id: "dword",
        pattern: Pattern::Sigilled {
            sigil: '.',
            names: &["dword"],
            required: true,
        },
        category: Category::Operation,
    },
    // `.lobytes`/`.hibytes`/`.bankbytes` take byte 0, 1 and 2 of each value in
    // the list; `.faraddr` emits all three, little-endian. Byte 2 is the bank
    // byte of a 65816 address, which is why the third has that name and the
    // fourth is a `far` address rather than a 24-bit word.
    Directive {
        id: "lobytes",
        pattern: Pattern::Sigilled {
            sigil: '.',
            names: &["lobytes"],
            required: true,
        },
        category: Category::Operation,
    },
    Directive {
        id: "hibytes",
        pattern: Pattern::Sigilled {
            sigil: '.',
            names: &["hibytes"],
            required: true,
        },
        category: Category::Operation,
    },
    Directive {
        id: "bankbytes",
        pattern: Pattern::Sigilled {
            sigil: '.',
            names: &["bankbytes"],
            required: true,
        },
        category: Category::Operation,
    },
    Directive {
        id: "faraddr",
        pattern: Pattern::Sigilled {
            sigil: '.',
            names: &["faraddr"],
            required: true,
        },
        category: Category::Operation,
    },
    Directive {
        id: "asciiz",
        pattern: Pattern::Sigilled {
            sigil: '.',
            names: &["asciiz"],
            required: true,
        },
        category: Category::Operation,
    },
    Directive {
        id: "res",
        pattern: Pattern::Sigilled {
            sigil: '.',
            names: &["res"],
            required: true,
        },
        category: Category::Operation,
    },
    // Dispatched elsewhere: walk-handled, segment-assigned, or macro-expanded
    // before parsing.
    Directive {
        id: "include",
        pattern: Pattern::Sigilled {
            sigil: '.',
            names: &["include"],
            required: true,
        },
        category: Category::Operation,
    },
    Directive {
        id: "incbin",
        pattern: Pattern::Sigilled {
            sigil: '.',
            names: &["incbin"],
            required: true,
        },
        category: Category::Operation,
    },
    Directive {
        id: "segment",
        pattern: Pattern::Sigilled {
            sigil: '.',
            names: &["segment"],
            required: true,
        },
        category: Category::Operation,
    },
    // Walk-handled: the shared cursor reads these into `Item::Conditional` /
    // `Item::Repeat` before `parse_directive` sees a line.
    //
    // Every head ca65 has, `.ifref`/`.ifnref` included: each one folds, so
    // none of them groups a block the projection cannot read.
    Directive {
        id: "conditional",
        pattern: Pattern::Exact(&[
            ".if",
            ".ifdef",
            ".ifndef",
            ".ifblank",
            ".ifnblank",
            ".ifconst",
            ".ifnconst",
            ".ifp02",
            ".ifp4510",
            ".ifp816",
            ".ifpc02",
            ".ifpsc02",
            ".ifref",
            ".ifnref",
            ".elseif",
            ".else",
            ".endif",
        ]),
        category: Category::Operation,
    },
    Directive {
        id: "repeat",
        pattern: Pattern::Exact(&[".repeat", ".endrepeat"]),
        category: Category::Operation,
    },
    Directive {
        id: "macro",
        pattern: Pattern::Sigilled {
            sigil: '.',
            names: &["macro", "mac"],
            required: true,
        },
        category: Category::Operation,
    },
    Directive {
        id: "endmacro",
        pattern: Pattern::Sigilled {
            sigil: '.',
            names: &["endmacro", "endmac"],
            required: true,
        },
        category: Category::Operation,
    },
    Directive {
        id: "local",
        pattern: Pattern::Sigilled {
            sigil: '.',
            names: &["local"],
            required: true,
        },
        category: Category::Operation,
    },
    // -----------------------------------------------------------------------
    // What ca65 has and we do not.
    //
    // Declared rather than left to fall through, because the fall-through
    // answers `unsupported directive` for `.zzqq` as well as for `.export` —
    // and conflating "ca65 has this and we have not implemented it" with "this
    // is not a thing" tells the reader to go looking for a typo. Same call as
    // the asl family's (#87) and vasm's `adda`: the source is valid and the
    // gap is ours, so the message should say so.
    //
    // Ninety-seven spellings, each confirmed in **statement position** against
    // ca65 V2.18 — offered as a lone operation and, where that failed with
    // `Unexpected '.X'`, offered again after a label to catch the label-first
    // forms. The thirty-seven that failed both ways are ca65's
    // pseudo-*functions* — `.lobyte`, `.strlen`, `.max`, `.sizeof` — which
    // live inside expressions and are not directives at all. Declaring those
    // here would name them in a place they never appear.
    // segments and location
    // The segment shorthands, and the stack that saves and restores the active
    // one. Each is `.segment "NAME"` by another name, so they are tracked in
    // the same source-only node and cost nothing beyond the mapping.
    Directive {
        id: "segment-shorthand",
        pattern: Pattern::Sigilled {
            sigil: '.',
            names: &[
                "code", "data", "bss", "rodata", "zeropage", "pushseg", "popseg",
            ],
            required: true,
        },
        category: Category::Operation,
    },
    Directive {
        id: "align",
        pattern: Pattern::Sigilled {
            sigil: '.',
            names: &["align"],
            required: true,
        },
        category: Category::Operation,
    },
    Directive {
        id: "out",
        pattern: Pattern::Sigilled {
            sigil: '.',
            names: &["out"],
            required: true,
        },
        category: Category::Operation,
    },
    Directive {
        id: "message",
        pattern: Pattern::Sigilled {
            sigil: '.',
            names: &["warning", "error", "fatal"],
            required: true,
        },
        category: Category::Operation,
    },
    Directive {
        id: "assert",
        pattern: Pattern::Sigilled {
            sigil: '.',
            names: &["assert"],
            required: true,
        },
        category: Category::Operation,
    },
    // `.org` names an address without moving the output; `.reloc` gives it
    // back to the segment.
    Directive {
        id: "placement",
        pattern: Pattern::Sigilled {
            sigil: '.',
            names: &["org", "reloc"],
            required: true,
        },
        category: Category::Operation,
    },
    // Symbol visibility, probed against ca65 2.18 + ld65 in the fused
    // assemble+link this dialect performs. See
    // `decisions/symbol-visibility-in-a-fused-assembler.md` and `VisRule`.
    Directive {
        id: "export",
        pattern: Pattern::Sigilled {
            sigil: '.',
            names: &["export", "exportzp"],
            required: true,
        },
        category: Category::Operation,
    },
    Directive {
        id: "import",
        pattern: Pattern::Sigilled {
            sigil: '.',
            names: &["import", "importzp"],
            required: true,
        },
        category: Category::Operation,
    },
    Directive {
        id: "global",
        pattern: Pattern::Sigilled {
            sigil: '.',
            names: &["global", "globalzp"],
            required: true,
        },
        category: Category::Operation,
    },
    // `.forceimport` cannot be satisfied here: defining its name is ca65's
    // `Symbol 'zz' is already an import`, and not defining it is an unresolved
    // external at ld65 even with nothing referencing it — which is what the
    // "force" means. The third dialect to need this category.
    Directive {
        id: "forceimport",
        pattern: Pattern::Sigilled {
            sigil: '.',
            names: &["forceimport"],
            required: true,
        },
        category: Category::RefusedByReference(
            "only usable when a linker resolves it from another module, and asm198x \
             emits a binary",
        ),
    },
    // `.autoimport +`/`-` switches ca65's automatic import of runtime symbols.
    // There is no runtime library to import from here, and it emits nothing.
    Directive {
        id: "autoimport",
        pattern: Pattern::Sigilled {
            sigil: '.',
            names: &["autoimport"],
            required: true,
        },
        category: Category::Ignored,
    },
    // The words that address the listing and the object file's own metadata
    // rather than the program. Probed against V2.18 and ld65: each assembles to
    // the same bytes with the directive present or absent, so accepting and
    // discarding one is what matching the reference means here.
    //
    // `.list off` does not suppress bytes, only listing lines; `.dbg` and
    // `.debuginfo` add records to the object file, which the binary output
    // never carries; `.fileopt`/`.fopt` write an object-file comment. The two
    // option words that are *not* here are the two that change what the parse
    // does: `.linecont +` turns on backslash continuation, and `.case off`
    // makes identifiers case-blind.
    Directive {
        id: "listing",
        pattern: Pattern::Sigilled {
            sigil: '.',
            names: &[
                "list",
                "listbytes",
                "pagelen",
                "pagelength",
                "debuginfo",
                "dbg",
                "fileopt",
                "fopt",
            ],
            required: true,
        },
        category: Category::Ignored,
    },
    // The walk keeps each constructor, destructor and interruptor, and checks
    // what ld65 would of them.
    Directive {
        id: "condes",
        pattern: Pattern::Sigilled {
            sigil: '.',
            names: &["condes", "constructor", "destructor", "interruptor"],
            required: true,
        },
        category: Category::Operation,
    },
    // Walk-handled, like the segment directives: the scope stack is parse
    // state, so these are read before `parse_directive` sees a line.
    Directive {
        id: "scopes",
        pattern: Pattern::Sigilled {
            sigil: '.',
            names: &["proc", "endproc", "scope", "endscope"],
            required: true,
        },
        category: Category::Operation,
    },
    // Walk-handled, like the scopes: a record declares constants rather than
    // statements, and the walker is what reads its lines.
    Directive {
        id: "records",
        pattern: Pattern::Sigilled {
            sigil: '.',
            names: &[
                "struct",
                "endstruct",
                "union",
                "endunion",
                "enum",
                "endenum",
                "tag",
            ],
            required: true,
        },
        category: Category::Operation,
    },
    // `.sizeof` is a name rather than a computation here: a record stores its
    // size as a constant when its declaration closes.
    Directive {
        id: "sizeof",
        pattern: Pattern::Exact(&[".sizeof"]),
        category: Category::ExpressionWord,
    },
    // `.end` stops the assembler reading the file.
    Directive {
        id: "end",
        pattern: Pattern::Sigilled {
            sigil: '.',
            names: &["end"],
            required: true,
        },
        category: Category::Operation,
    },
    // Live text symbols, consumed before ordinary line parsing so their state
    // follows ca65's textual includes.
    Directive {
        id: "text-defines",
        pattern: Pattern::Sigilled {
            sigil: '.',
            names: &["define", "undef", "undefine"],
            required: true,
        },
        category: Category::Operation,
    },
    // Lang198x's expander deletes a macro.
    Directive {
        id: "delmac",
        pattern: Pattern::Sigilled {
            sigil: '.',
            names: &["delmac", "delmacro"],
            required: true,
        },
        category: Category::Operation,
    },
    // ca65's macro packages are not shipped here.
    Directive {
        id: "macpack",
        pattern: Pattern::Sigilled {
            sigil: '.',
            names: &["macpack"],
            required: true,
        },
        category: Category::KnownUnsupported,
    },
    // conditionals over the assembler's own state
    Directive {
        id: "unsupported-conditionals",
        pattern: Pattern::Sigilled {
            sigil: '.',
            names: &["ref", "referenced"],
            required: true,
        },
        category: Category::ExpressionWord,
    },
    // The live reader handles 6502/4510 selectors and the CPU stack.
    // Other known processors keep their explicit unsupported-profile error.
    // `.smart` remains inert on these fixed-width processors.
    Directive {
        id: "cpu",
        pattern: Pattern::Sigilled {
            sigil: '.',
            names: &[
                "setcpu", "pushcpu", "popcpu", "smart", "p02", "p4510", "p816", "pc02", "psc02",
            ],
            required: true,
        },
        category: Category::Operation,
    },
    // The 65816's register widths, which size its immediates. ca65 takes
    // them only on the 65816; elsewhere it says `Command is only valid in
    // 65816 mode`, and so does the walk.
    Directive {
        id: "register-widths",
        pattern: Pattern::Sigilled {
            sigil: '.',
            names: &["a8", "a16", "i8", "i16"],
            required: true,
        },
        category: Category::Operation,
    },
    // The option words that change what the parse does, and so cannot be
    // accepted and discarded: `.linecont +` turns on backslash continuation,
    // `.case off` makes identifiers case-blind, `.charmap`/`.localchar`
    // translate characters on the way out, and `.feature` switches syntax
    // rules on one by one.
    Directive {
        id: "unsupported-parse-options",
        pattern: Pattern::Sigilled {
            sigil: '.',
            names: &[
                "linecont",
                "case",
                "charmap",
                "localchar",
                "feature",
                "null",
            ],
            required: true,
        },
        category: Category::KnownUnsupported,
    },
    // A word ca65's tokenizer knows and its parser accepts nowhere. Probed
    // against V2.18 at statement position, after a label, as a struct storage
    // allocator, as an `.import`/`.export` address-size specifier and inside an
    // expanded macro body: every one answers `Unexpected '.FORCEWORD'` (the
    // address-size specifiers there are `zeropage`, `absolute`, `direct` and
    // `near`, plus `far`/`dword` on a CPU wide enough for them). Reachable
    // source cannot contain it, so refusing it is matching the reference.
    Directive {
        id: "forceword",
        pattern: Pattern::Sigilled {
            sigil: '.',
            names: &["forceword"],
            required: true,
        },
        category: Category::RefusedByReference(
            "a word its tokenizer knows and its parser accepts in no position",
        ),
    },
];

// ---------------------------------------------------------------------------
// Value parsing over the shared expression core
// ---------------------------------------------------------------------------

// The colon is not a cheap identifier character, so this property key cannot
// collide with an authored @size constant or label.

#[cfg(test)]
mod tests {
    use super::assemble;

    fn rom(src: &str) -> Vec<u8> {
        assemble(src).expect("assembles").0
    }

    /// ca65's text layer, folded before the parse. Every value here was read
    /// off ca65 V2.18 first.
    #[test]
    fn the_string_functions_fold_the_way_ca65_folds_them() {
        // The ROM's payload starts after the 16-byte iNES header.
        let code = |src: &str| {
            let r = rom(&format!(".segment \"CODE\"\n{src}\n"));
            r[16..16 + 8].to_vec()
        };
        assert_eq!(&code(".byte .concat(\"ab\",\"cd\")")[..4], b"abcd");
        // One argument is a whole call; ca65 does not require two.
        assert_eq!(&code(".byte .concat(\"a\")")[..1], b"a");

        // `.string` stringifies the *token*, not the value: with `N = 7`,
        // `.string(N)` is `"N"` and not `"7"`. A literal keeps its digits.
        assert_eq!(&code("N = 7\n.byte .string(N)")[..1], b"N");
        assert_eq!(&code(".byte .string(42)")[..2], b"42");
        assert_eq!(&code("lbl:\n.byte .string(lbl)")[..3], b"lbl");

        // `.ident` builds a *name* from text, so it resolves like any other —
        // a forward reference included.
        assert_eq!(&code("foo = 5\n.byte .ident(\"foo\")")[..1], &[5]);
        assert_eq!(
            &code(".byte .ident(.concat(\"f\",\"oo\"))\nfoo = 9")[..1],
            &[9]
        );

        // The folds nest, innermost first, so a numeric function reads a
        // literal by the time it runs.
        assert_eq!(&code(".byte .strlen(.concat(\"ab\",\"cd\"))")[..1], &[4]);
    }

    /// `.paramcount` and `.definedmacro`, read off ca65 V2.18 first. Both are
    /// answered during macro expansion, which is the only place either has a
    /// meaning.
    #[test]
    fn the_macro_predicates_answer_the_way_ca65_answers() {
        let code = |src: &str| {
            let r = rom(&format!(".segment \"CODE\"\n{src}\n"));
            r[16..16 + 4].to_vec()
        };
        let m = ".macro m p1, p2\n.byte .paramcount\n.endmacro\n";

        // The **call site's** count, not the declared one: two parameters
        // called with one answers 1, and a blank argument still counts.
        assert_eq!(&code(&format!("{m}m 1, 2"))[..1], &[2]);
        assert_eq!(&code(&format!("{m}m 1"))[..1], &[1]);
        assert_eq!(&code(&format!("{m}m"))[..1], &[0]);
        assert_eq!(
            &code(".macro m p1, p2, p3\n.byte .paramcount\n.endmacro\nm 1, , 3")[..1],
            &[3]
        );
        // A nested invocation answers for itself, not for the macro around it.
        assert_eq!(
            &code(".macro i\n.byte .paramcount\n.endmacro\n.macro o p1\ni\n.endmacro\no 1")[..1],
            &[0]
        );
        // It reads as a number, so a conditional can branch on it.
        assert_eq!(
            &code(
                ".macro m p1, p2\n.if .paramcount > 1\n.byte 9\n.else\n.byte 8\n.endif\n\
                 .endmacro\nm 1\nm 1,2"
            )[..2],
            &[8, 9]
        );

        // `.definedmacro` is answered where it is *written*: a definition below
        // the line does not count, and the name is case-sensitive.
        assert_eq!(
            &code(
                ".macro d\n.endmacro\n.byte .definedmacro(d), .definedmacro(n), .definedmacro(D)"
            )[..3],
            &[1, 0, 0]
        );
        assert_eq!(
            &code(".byte .definedmacro(d)\n.macro d\n.endmacro")[..1],
            &[0]
        );
        // Inside a body it is answered at the line the macro was *invoked* on,
        // which is where ca65 answers it too.
        assert_eq!(
            &code(".macro d\n.endmacro\n.macro n\n.byte .definedmacro(d)\n.endmacro\nn")[..1],
            &[1]
        );
    }

    /// `.const` and `.ismnem`, read off ca65 V2.18 first.
    #[test]
    fn const_and_ismnem_answer_the_way_ca65_answers() {
        let code = |src: &str| {
            let r = rom(&format!(".segment \"CODE\"\n{src}\n"));
            r[16..16 + 4].to_vec()
        };

        // A literal and a constant above the line are constant; a label and the
        // location counter are not, and neither is a constant defined *below*
        // — which is exactly what a pass walking in source order can see.
        assert_eq!(
            &code("N = 5\nL: .byte .const(5), .const(N), .const(N*2)")[..3],
            &[1, 1, 1]
        );
        assert_eq!(&code("L: .byte .const(L), .const(*)")[..2], &[0, 0]);
        assert_eq!(&code(".byte .const(N)\nN = 5")[..1], &[0]);
        // A known scoped constant is read through the selected scope.
        assert_eq!(
            &code(".scope pa\nv=1\n.byte .const(v)\n.endscope")[..1],
            &[1]
        );

        // `.ismnem` follows the CPU. `bra` and `stz` are 65C02 additions that
        // plain 6502 ca65 does not know, and `nop` is in every one of them.
        assert_eq!(
            &code(".byte .ismnem(lda), .ismnemonic(lda), .ismnem(zzz)")[..3],
            &[1, 1, 0]
        );
        assert_eq!(
            &code(".byte .ismnem(bra), .ismnem(stz), .ismnem(nop)")[..3],
            &[0, 0, 1]
        );
        // The name is read case-insensitively.
        assert_eq!(&code(".byte .ismnem(LDA)")[..1], &[1]);
    }

    /// What `.const` and `.ismnem` refuse.
    #[test]
    fn const_and_ismnem_refuse_what_they_cannot_answer() {
        let refused = |src: &str| {
            assemble(&format!(".segment \"CODE\"\n{src}\n"))
                .expect_err(&format!("should refuse `{src}`"))
        };
        // ca65 answers a name it has never seen with "Symbol is undefined"
        // rather than with 0, so a 0 here would be an invention.
        let e = refused(".byte .const(zzz)");
        assert!(e.to_string().contains("Symbol `zzz` is undefined"), "{e}");

        // Native ld65 rejects an imported name that the query actually reads.
        let e = refused(".import ext\n.byte .const(ext)");
        assert!(e.to_string().contains("Symbol `ext` is undefined"), "{e}");
        // Explicit qualification cannot fall back to the root's N.
        let e = refused("N = 1\n.byte .const(pa::N)");
        assert!(e.to_string().contains("Symbol `pa::N` is undefined"), "{e}");

        // ca65 takes a bare name for `.ismnem` and nothing else.
        refused(".byte .ismnem(\"lda\")");
        refused(".byte .ismnem(1+1)");
    }

    /// The token-list half, read off ca65 V2.18 first.
    ///
    /// A token list is unevaluated source, so these answer over what is
    /// *written*. The comparison rules are the part worth pinning: `.match`
    /// asks what each token **is** and `.xmatch` asks what it **says**.
    #[test]
    fn the_token_functions_fold_the_way_ca65_folds_them() {
        let code = |src: &str| {
            let r = rom(&format!(".segment \"CODE\"\n{src}\n"));
            r[16..16 + 6].to_vec()
        };

        // A comma is a token, and a string is one however much is inside it.
        assert_eq!(&code(".byte .tcount({1, 2, 3})")[..1], &[5]);
        assert_eq!(
            &code(".byte .tcount({}), .tcount({abc}), .tcount({\"a,b\"})")[..3],
            &[0, 1, 1]
        );
        assert_eq!(
            &code(".byte .tcount({#$12}), .tcount({pa::v}), .tcount({.byte 1})")[..3],
            &[2, 3, 2]
        );
        // Whitespace separates and is not a token, and `<<` is one where `+ +`
        // is two.
        assert_eq!(
            &code(".byte .tcount({++}), .tcount({<<}), .tcount({:+})")[..3],
            &[2, 1, 1]
        );
        assert_eq!(&code(".byte .blank({}), .blank({x})")[..2], &[1, 0]);

        // `.match` compares kinds. Two numbers match whatever they are worth,
        // and so do two strings — but `a` is the accumulator and `b` is an
        // identifier, so those do not.
        assert_eq!(
            &code(".byte .match({1},{2}), .match({\"a\"},{\"b\"}), .match({abc},{abd})")[..3],
            &[1, 1, 1]
        );
        assert_eq!(
            &code(".byte .match({a},{b}), .match({x},{y}), .match({a},{A})")[..3],
            &[0, 0, 1]
        );
        // `s` is a register on a 65816 and an ordinary identifier here, so
        // this same line answers 0 through the `ca65-816` dialect. ca65 takes
        // the register names from the CPU.
        assert_eq!(&code(".byte .match({s},{q})")[..1], &[1]);
        // Each dot-keyword and each punctuation mark is its own kind, and a
        // character literal is not a number.
        assert_eq!(
            &code(".byte .match({.byte},{.word}), .match({+},{-}), .match({'a'},{1})")[..3],
            &[0, 0, 0]
        );
        assert_eq!(
            &code(".byte .match({a},{a b}), .match({},{})")[..2],
            &[0, 1]
        );

        // `.xmatch` adds the value: numbers by what they are worth, so `1` and
        // `$1` agree, and identifiers case-sensitively, so `lda` and `LDA` do
        // not.
        assert_eq!(
            &code(".byte .xmatch({1},{2}), .xmatch({1},{$1})")[..2],
            &[0, 1]
        );
        assert_eq!(
            &code(".byte .xmatch({abc},{abd}), .xmatch({lda},{LDA})")[..2],
            &[0, 0]
        );
        assert_eq!(
            &code(".byte .xmatch({\"a\"},{\"b\"}), .xmatch({.byte},{.byte})")[..2],
            &[0, 1]
        );

        // The sub-list functions splice tokens back as source: `.left(3, …)`
        // takes `1`, `,` and `2`, which is a two-item byte list.
        assert_eq!(&code(".byte .left(1, {1, 2, 3})")[..1], &[1]);
        assert_eq!(&code(".byte .left(3, {1, 2, 3})")[..2], &[1, 2]);
        assert_eq!(
            &code(".byte .right(1, {1, 2, 3}), .mid(2, 1, {1, 2, 3})")[..2],
            &[3, 2]
        );
        // Past the end stops at the end rather than refusing.
        assert_eq!(
            &code(".byte .left(9, {1}), .right(9, {1}), .mid(0, 9, {1})")[..3],
            &[1, 1, 1]
        );

        // A string-valued function inside a list folds first and is then one
        // token, exactly as ca65 counts it — while a numeric one stays four.
        assert_eq!(
            &code(".byte .tcount({.concat(\"a\",\"b\")}), .tcount({.strlen(\"ab\")})")[..2],
            &[1, 4]
        );
    }

    /// `.sprintf`, one row per case measured on ca65.
    ///
    /// ca65's formatter is not C's: `%x` is signed where `%X` is not, `%s` and
    /// `%c` align the opposite way to everything else, `#` on `%x` shows a
    /// prefix even for zero, and `+` and a space mark unsigned conversions
    /// too. ca65's manual calls the format "similar to the one of the C printf
    /// function", so Lang198x gives C's result in each of those cases, a named
    /// divergence (`docs/plans/2026-10-27-ca65-walk/plan.md`). Those rows hold
    /// C's answer; the rest are ca65's.
    #[test]
    fn sprintf_formats_the_way_ca65_formats() {
        let text = |src: &str| {
            let r = rom(&format!(".segment \"CODE\"\n{src}\n"));
            let body = &r[16..];
            let end = body.iter().position(|b| *b == 0).unwrap_or(0);
            String::from_utf8_lossy(&body[..end]).into_owned()
        };
        #[rustfmt::skip]
        let cases: &[(&str, &str, &str)] = &[
            ("%d", "5", "5"),
            ("%d", "-5", "-5"),
            ("%d", "0", "0"),
            ("%i", "5", "5"),
            ("%i", "-5", "-5"),
            ("%u", "5", "5"),
            ("%u", "-5", "18446744073709551611"),
            ("%x", "255", "ff"),
            ("%x", "-255", "ffffffffffffff01"),
            ("%x", "-1", "ffffffffffffffff"),
            ("%X", "255", "FF"),
            ("%X", "-255", "FFFFFFFFFFFFFF01"),
            ("%X", "-1", "FFFFFFFFFFFFFFFF"),
            ("%o", "9", "11"),
            ("%o", "-9", "1777777777777777777767"),
            ("%c", "65", "A"),
            ("%s", "\"ab\"", "ab"),
            ("%+d", "5", "+5"),
            ("%+d", "-5", "-5"),
            ("%+d", "0", "+0"),
            ("% d", "5", " 5"),
            ("% d", "-5", "-5"),
            ("% d", "0", " 0"),
            ("%#x", "255", "0xff"),
            ("%#x", "-255", "0xffffffffffffff01"),
            ("%#x", "-1", "0xffffffffffffffff"),
            ("%#X", "255", "0XFF"),
            ("%#X", "-255", "0XFFFFFFFFFFFFFF01"),
            ("%#X", "-1", "0XFFFFFFFFFFFFFFFF"),
            ("%#o", "9", "011"),
            ("%#o", "-9", "01777777777777777777767"),
            ("%#d", "5", "5"),
            ("%#d", "-5", "-5"),
            ("%#d", "0", "0"),
            ("%#s", "\"ab\"", "ab"),
            ("%#c", "65", "A"),
            ("%+u", "5", "5"),
            ("%+u", "-5", "18446744073709551611"),
            ("%+x", "255", "ff"),
            ("%+x", "-255", "ffffffffffffff01"),
            ("%+x", "-1", "ffffffffffffffff"),
            ("%+X", "255", "FF"),
            ("%+X", "-255", "FFFFFFFFFFFFFF01"),
            ("%+X", "-1", "FFFFFFFFFFFFFFFF"),
            ("%+o", "9", "11"),
            ("%+o", "-9", "1777777777777777777767"),
            ("%+c", "65", "A"),
            ("%+s", "\"ab\"", "ab"),
            ("% u", "5", "5"),
            ("% u", "-5", "18446744073709551611"),
            ("% x", "255", "ff"),
            ("% x", "-255", "ffffffffffffff01"),
            ("% x", "-1", "ffffffffffffffff"),
            ("% X", "255", "FF"),
            ("% X", "-255", "FFFFFFFFFFFFFF01"),
            ("% X", "-1", "FFFFFFFFFFFFFFFF"),
            ("% o", "9", "11"),
            ("% o", "-9", "1777777777777777777767"),
            ("% s", "\"ab\"", "ab"),
            ("% c", "65", "A"),
            ("%6d", "5", "     5"),
            ("%6d", "-5", "    -5"),
            ("%6d", "0", "     0"),
            ("%-6d", "5", "5     "),
            ("%-6d", "-5", "-5    "),
            ("%-6d", "0", "0     "),
            ("%06d", "5", "000005"),
            ("%06d", "-5", "-00005"),
            ("%06d", "0", "000000"),
            ("%-06d", "5", "5     "),
            ("%-06d", "-5", "-5    "),
            ("%-06d", "0", "0     "),
            ("%+06d", "5", "+00005"),
            ("%+06d", "-5", "-00005"),
            ("%+06d", "0", "+00000"),
            ("%6x", "255", "    ff"),
            ("%6x", "-255", "ffffffffffffff01"),
            ("%6x", "-1", "ffffffffffffffff"),
            ("%-6x", "255", "ff    "),
            ("%-6x", "-255", "ffffffffffffff01"),
            ("%-6x", "-1", "ffffffffffffffff"),
            ("%06x", "255", "0000ff"),
            ("%06x", "-255", "ffffffffffffff01"),
            ("%06x", "-1", "ffffffffffffffff"),
            ("%#08x", "255", "0x0000ff"),
            ("%#08x", "-255", "0xffffffffffffff01"),
            ("%#08x", "-1", "0xffffffffffffffff"),
            ("%+#08x", "255", "0x0000ff"),
            ("%+#08x", "-255", "0xffffffffffffff01"),
            ("%+#08x", "-1", "0xffffffffffffffff"),
            ("%#08X", "255", "0X0000FF"),
            ("%#08X", "-255", "0XFFFFFFFFFFFFFF01"),
            ("%#08X", "-1", "0XFFFFFFFFFFFFFFFF"),
            ("%+#08X", "255", "0X0000FF"),
            ("%+#08X", "-255", "0XFFFFFFFFFFFFFF01"),
            ("%+#08X", "-1", "0XFFFFFFFFFFFFFFFF"),
            ("%6X", "255", "    FF"),
            ("%6X", "-255", "FFFFFFFFFFFFFF01"),
            ("%6X", "-1", "FFFFFFFFFFFFFFFF"),
            ("%06X", "255", "0000FF"),
            ("%06X", "-255", "FFFFFFFFFFFFFF01"),
            ("%06X", "-1", "FFFFFFFFFFFFFFFF"),
            ("%6o", "9", "    11"),
            ("%6o", "-9", "1777777777777777777767"),
            ("%06o", "9", "000011"),
            ("%06o", "-9", "1777777777777777777767"),
            ("%6u", "5", "     5"),
            ("%6u", "-5", "18446744073709551611"),
            ("%06u", "5", "000005"),
            ("%06u", "-5", "18446744073709551611"),
            ("%6s", "\"ab\"", "    ab"),
            ("%-6s", "\"ab\"", "ab    "),
            ("%06s", "\"ab\"", "    ab"),
            ("%.2s", "\"ab\"", "ab"),
            ("%.6s", "\"ab\"", "ab"),
            ("%6c", "65", "     A"),
            ("%-6c", "65", "A     "),
            ("%06c", "65", "     A"),
            ("%.1c", "65", "A"),
            ("%.3d", "5", "005"),
            ("%.3d", "-5", "-005"),
            ("%.3d", "0", "000"),
            ("%.4x", "255", "00ff"),
            ("%.4x", "-255", "ffffffffffffff01"),
            ("%.4x", "-1", "ffffffffffffffff"),
            ("%.4X", "255", "00FF"),
            ("%.4X", "-255", "FFFFFFFFFFFFFF01"),
            ("%.4X", "-1", "FFFFFFFFFFFFFFFF"),
            ("%.4o", "9", "0011"),
            ("%.4o", "-9", "1777777777777777777767"),
            ("%.4u", "5", "0005"),
            ("%.4u", "-5", "18446744073709551611"),
            ("%.0d", "5", "5"),
            ("%.0d", "-5", "-5"),
            ("%.0d", "0", ""),
        ];
        for (spec, value, want) in cases {
            let src = format!(".byte .sprintf(\"{spec}\", {value}), 0");
            assert_eq!(&text(&src), want, "`{spec}` of `{value}`");
        }
    }

    /// The pass folds a constants environment as it walks, so `%d` may be
    /// given a constant defined above the line — and the cases ca65 refuses
    /// itself are refused here for the same reason.
    #[test]
    fn sprintf_reads_the_constants_above_it() {
        let code = |src: &str| {
            let r = rom(&format!(".segment \"CODE\"\n{src}\n"));
            r[16..16 + 4].to_vec()
        };
        assert_eq!(&code("N = 5\n.byte .sprintf(\"n=%d\", N)")[..3], b"n=5");
        assert_eq!(&code("N = 5\n.byte .sprintf(\"%d\", N*2+1)")[..2], b"11");

        // A label's address and a constant defined *below* are both out of
        // reach of a pass that walks in source order — and ca65 answers both
        // with "Constant expression expected", so nothing is lost.
        for src in [
            "L: .byte .sprintf(\"%d\", L)",
            ".byte .sprintf(\"%d\", N)\nN = 5",
        ] {
            assemble(&format!(".segment \"CODE\"\n{src}\n"))
                .expect_err(&format!("ca65 refuses `{src}`"));
        }

        // A constant defined inside a scope is not recorded: folding it flat
        // would let an inner `v` answer a `v` written outside, where ca65
        // resolves two different symbols.
        assemble(".segment \"CODE\"\n.scope pa\nv = 1\n.endscope\n.byte .sprintf(\"%d\", v)\n")
            .expect_err("an inner scope's constant is not in scope outside it");
    }

    /// What `.sprintf` refuses. ca65 refuses each of these itself.
    #[test]
    fn sprintf_refuses_what_ca65_refuses() {
        for src in [
            // Only eight conversions, and `%b`, `%f`, `%ld` and `%*d` are not
            // among them however ordinary they look in C.
            ".byte .sprintf(\"%q\", 1)",
            ".byte .sprintf(\"%b\", 1)",
            ".byte .sprintf(\"%f\", 1)",
            ".byte .sprintf(\"%ld\", 1)",
            ".byte .sprintf(\"%*d\", 5, 1)",
            // The two halves cannot be swapped.
            ".byte .sprintf(\"%d\", \"hi\")",
            ".byte .sprintf(\"%s\", 5)",
            // A character runs from 1 to 255.
            ".byte .sprintf(\"%c\", 0)",
            ".byte .sprintf(\"%c\", 256)",
            ".byte .sprintf(\"%c\", -1)",
            // The argument count is exact in both directions.
            ".byte .sprintf(\"%d %d\", 1)",
            ".byte .sprintf(\"%d\", 1, 2)",
        ] {
            assemble(&format!(".segment \"CODE\"\n{src}\n"))
                .expect_err(&format!("ca65 refuses `{src}`"));
        }
    }

    /// What the three refuse. ca65 refuses each of these itself.
    #[test]
    fn the_string_functions_refuse_what_ca65_refuses() {
        for src in [
            // `.concat` joins strings, and a number is not one.
            ".byte .concat(\"a\", 5)",
            // `.string` takes one name or number — not a string, and not an
            // expression over them.
            ".byte .string(\"hi\")",
            "N = 7\n.byte .string(N*2)",
            ".byte .string(1, 2)",
            // `.ident` needs the text a name is built from.
            ".byte .ident(foo)",
        ] {
            assemble(&format!(".segment \"CODE\"\n{src}\n"))
                .expect_err(&format!("ca65 refuses `{src}`"));
        }
    }

    #[test]
    fn rom_has_nrom_shape() {
        let r = rom(".segment \"CODE\"\nrts\n");
        assert_eq!(r.len(), 16 + 0x8000 + 0x2000);
    }

    #[test]
    fn segment_outside_the_nes_config_is_rejected_with_help() {
        // `RODATA` has no memory area in the curriculum's `nes.cfg`, so `ld65`
        // rejects it — and so do we. The message names the valid segments
        // rather than a bare "unknown segment".
        let err = assemble(".segment \"RODATA\"\n .byte 1\n").expect_err("rejected");
        let msg = err.to_string();
        assert!(msg.contains("RODATA"), "got `{msg}`");
        assert!(msg.contains("not in the NES config"), "got `{msg}`");
        assert!(
            msg.contains("CODE") && msg.contains("VECTORS"),
            "got `{msg}`"
        );
    }

    /// The expression-function seam: a name followed by `(` is a call, and an
    /// unimplemented `.`-word says so rather than reading as an undefined
    /// symbol.
    #[test]
    fn expression_functions_extract_bytes() {
        let r = rom(".code\nV = $123456\n lda #.lobyte(V)\n lda #.hibyte(V)\n lda #.bankbyte(V)\n");
        assert_eq!(&r[16..22], &[0xA9, 0x56, 0xA9, 0x34, 0xA9, 0x12]);
        // Nested, and over an expression rather than a bare symbol.
        let n = rom(".code\n lda #.lobyte($1234+1)\n lda #.lobyte(.hibyte($123456))\n");
        assert_eq!(&n[16..20], &[0xA9, 0x35, 0xA9, 0x34]);

        // The word extractions, which have no `Expr` node and need none.
        let w = rom(".code\nV = $123456\n .word .loword(V)\n .word .hiword(V)\n");
        assert_eq!(&w[16..20], &[0x56, 0x34, 0x12, 0x00]);

        // Two arguments, and the wrong count refused rather than ignored.
        let m = rom(".code\n lda #.max(3, 7)\n lda #.min(3, 7)\n lda #.min(.max(1,5), 9)\n");
        assert_eq!(&m[16..22], &[0xA9, 0x07, 0xA9, 0x03, 0xA9, 0x05]);
        for (src, why) in [
            (
                ".code\n lda #.max(3)\n",
                "one argument to a two-argument function",
            ),
            (
                ".code\n lda #.lobyte(1, 2)\n",
                "two arguments to a one-argument function",
            ),
        ] {
            assert!(assemble(src).is_err(), "{why}");
        }

        // A string argument yields a number; the wrong argument kind is named.
        let t = rom(".code\n lda #.strlen(\"hello\")\n lda #.strat(\"abc\", 1)\n");
        assert_eq!(&t[16..20], &[0xA9, 0x05, 0xA9, 0x62]);
        for (src, want) in [
            (
                ".code\n lda #.strlen(5)\n",
                "expected a string in `.strlen`",
            ),
            (
                ".code\n lda #.lobyte(\"hi\")\n",
                "a string is not a value here",
            ),
        ] {
            let e = assemble(src).expect_err(src).to_string();
            assert!(e.contains(want), "expected `{want}`, got `{e}`");
        }

        // `.defined` is positional — the whole reason it is answered in the
        // projection rather than folded while parsing.
        let d = rom(
            ".code\n lda #.defined(L)\nL = 7\n lda #.defined(L)\n lda #.def(L)\n\
             .if .defined(L)\n lda #$33\n .else\n lda #$44\n .endif\n",
        );
        assert_eq!(
            &d[16..24],
            &[0xA9, 0x00, 0xA9, 0x01, 0xA9, 0x01, 0xA9, 0x33]
        );
        // A label counts as defined, not just an `=` constant.
        let l = rom(".code\nhere: lda #1\n lda #.defined(here)\n");
        assert_eq!(&l[18..20], &[0xA9, 0x01]);

        // A `.`-word we do not implement — with a plain argument, since a
        // string literal fails earlier, in the tokenizer.
        let err = assemble(".code\nV = 1\n lda #.bank(V)\n").expect_err("not implemented");
        assert!(
            err.to_string().contains("is not lowered yet"),
            "got `{err}`"
        );
        // `.sizeof` *is* implemented, and refuses a name that declared no size
        // — ca65's `Size of 'V' is unknown`.
        let err = assemble(".code\nV = 1\n lda #.sizeof(V)\n").expect_err("no size");
        assert!(
            err.to_string().contains("size of `V` is unknown"),
            "got `{err}`"
        );
    }

    #[test]
    fn the_segment_shorthands_are_their_spelled_out_segments() {
        // `.code` is `.segment "CODE"`, `.zeropage` is ZEROPAGE, `.bss` is
        // BSS — the same placement, reached by the shorter spelling.
        let long = rom(".segment \"ZEROPAGE\"\npos: .res 1\n\
             .segment \"BSS\"\nbuf: .res 4\n\
             .segment \"CODE\"\n lda pos\n sta buf\n");
        let short = rom(".zeropage\npos: .res 1\n.bss\nbuf: .res 4\n.code\n lda pos\n sta buf\n");
        assert_eq!(long, short);
        // `pos` is zero-page, `buf` is RAM at the config's $0300 (OAM has the
        // page below it).
        assert_eq!(&short[16..21], &[0xA5, 0x00, 0x8D, 0x00, 0x03]);
    }

    /// `.align` pads within the segment, so the boundary is measured from the
    /// segment's start rather than from the CPU address — the distinction only
    /// shows on a boundary the segment base is not itself a multiple of.
    #[test]
    fn align_pads_within_the_segment() {
        // CODE is based at $8000, which is not a multiple of 3; the pad still
        // lands the next byte at segment offset 3.
        let r = rom(".code\n .byte 1\n .align 3\n .byte 2\n");
        assert_eq!(&r[16..20], &[1, 0, 0, 2]);
        let f = rom(".code\n .byte 1\n .align 4, $ff\n .byte 2\n");
        assert_eq!(&f[16..21], &[1, 0xFF, 0xFF, 0xFF, 2]);
        let none = rom(".code\n .byte 1,2,3,4\n .align 4\n .byte 9\n");
        assert_eq!(&none[16..21], &[1, 2, 3, 4, 9]);
    }

    /// A label on the `.align` line binds *before* the pad, not after it —
    /// probe-pinned, and the opposite of what an alignment directive reads like.
    #[test]
    fn a_label_on_an_align_line_binds_before_the_pad() {
        let r = rom(
            ".code\n .byte 1\nhere: .align 4\n .byte 2\n             .segment \"VECTORS\"\n .word here, 0, 0\n",
        );
        // $8001 — where `here` stood, not $8004 where the next byte landed.
        assert_eq!(&r[16 + 0x7FFA..16 + 0x7FFC], &[0x01, 0x80]);
    }

    #[test]
    fn popseg_restores_the_segment_pushseg_saved() {
        // The reservation between the pair happens in ZEROPAGE; the code
        // either side of it stays in CODE, contiguous across the interruption.
        let r = rom(".code\n lda #1\n\
             .pushseg\n.zeropage\ntmp: .res 1\n.popseg\n\
             ldx #2\n");
        assert_eq!(&r[16..20], &[0xA9, 0x01, 0xA2, 0x02]);
    }

    #[test]
    fn a_shorthand_for_a_segment_the_config_lacks_is_rejected_too() {
        // `ca65` accepts `.data`; `ld65` then has no memory area for it. The
        // refusal is the same one the spelled-out form gets.
        let err = assemble(".data\n .byte 1\n").expect_err("rejected");
        assert!(err.to_string().contains("DATA"), "got `{err}`");
    }

    #[test]
    fn header_and_code_and_vectors_place_correctly() {
        let src = "\
.segment \"HEADER\"\n\
    .byte \"NES\", $1A, 2, 1\n\
.segment \"CODE\"\n\
reset:\n\
    sei\n\
nmi:\n\
    rti\n\
irq:\n\
    rti\n\
.segment \"VECTORS\"\n\
    .word nmi, reset, irq\n";
        let r = rom(src);
        // iNES magic.
        assert_eq!(&r[..5], &[0x4E, 0x45, 0x53, 0x1A, 0x02]);
        // CODE at $8000 (file offset 16): sei, rti, rti.
        assert_eq!(&r[16..19], &[0x78, 0x40, 0x40]);
        // reset=$8000, nmi=$8001, irq=$8002. VECTORS at $FFFA (file off 16+0x7FFA).
        let v = 16 + 0x7FFA;
        assert_eq!(&r[v..v + 6], &[0x01, 0x80, 0x00, 0x80, 0x02, 0x80]);
    }

    #[test]
    fn zeropage_label_uses_zp_addressing() {
        let src = "\
.segment \"ZEROPAGE\"\n\
counter: .res 1\n\
.segment \"CODE\"\n\
    sta counter\n";
        let r = rom(src);
        // sta zp = $85 $00 (counter at $00), not abs $8D.
        assert_eq!(&r[16..18], &[0x85, 0x00]);
    }

    #[test]
    fn anonymous_labels_resolve_by_direction() {
        // Byte-for-byte against ca65 + ld65 -t none. CODE at $8000:
        //   ldx #0 / : inx / bne :- / jmp :+ / nop / : rts
        let src = "\
.segment \"CODE\"\n\
    ldx #0\n\
:   inx\n\
    bne :-\n\
    jmp :+\n\
    nop\n\
:   rts\n";
        let r = rom(src);
        assert_eq!(
            &r[16..26],
            &[0xA2, 0x00, 0xE8, 0xD0, 0xFD, 0x4C, 0x09, 0x80, 0xEA, 0x60]
        );
    }

    #[test]
    fn anonymous_label_multi_distance() {
        // `:--` counts two anonymous labels back. ca65 + ld65: ea ea 4c 00 80.
        let src = "\
.segment \"CODE\"\n\
:   nop\n\
:   nop\n\
    jmp :--\n";
        let r = rom(src);
        assert_eq!(&r[16..21], &[0xEA, 0xEA, 0x4C, 0x00, 0x80]);
    }

    #[test]
    fn dword_dbyt_asciiz_match_reference_bytes() {
        // Byte-for-byte against `ca65 --cpu 6502` + `ld65 -t none`:
        //   .dword $12345678 -> 78 56 34 12 (32-bit little-endian)
        //   .dbyt  $1234     -> 12 34       (16-bit big-endian)
        //   .asciiz "hi"     -> 68 69 00    (string + one terminating NUL)
        let r = rom(".segment \"CODE\"\n.dword $12345678\n.dbyt $1234\n.asciiz \"hi\"\n");
        assert_eq!(
            &r[16..25],
            &[0x78, 0x56, 0x34, 0x12, 0x12, 0x34, 0x68, 0x69, 0x00]
        );
    }

    #[test]
    fn cheap_locals_scope_to_global() {
        let src = "\
.segment \"CODE\"\n\
one:\n\
@loop:\n\
    jmp @loop\n\
two:\n\
@loop:\n\
    jmp @loop\n";
        let r = rom(src);
        // one@loop at $8000: jmp $8000. two@loop at $8003: jmp $8003.
        assert_eq!(&r[16..22], &[0x4C, 0x00, 0x80, 0x4C, 0x03, 0x80]);
    }

    // -----------------------------------------------------------------------
    // Conditionals and repetition. Measured against ca65 V2.18; folded once, in
    // source order, before layout — `decisions/conditionals-in-multipass-dialects.md`.
    // -----------------------------------------------------------------------

    fn bytes(src: &str) -> Vec<u8> {
        let rom = crate::assemble_ca65(src).expect(src).bytes;
        // The NES ROM is header + 32K PRG; the code sits at the PRG's start.
        rom[16..].to_vec()
    }

    /// `.out` prints its string and emits nothing. ca65 writes the text bare,
    /// with no file/line prefix, so the note carries the text alone.
    #[test]
    fn out_is_a_note_and_emits_nothing() {
        let r = crate::assemble_ca65(".segment \"CODE\"\n.out \"hello there\"\nnop\n")
            .expect("assembles");
        assert_eq!(r.bytes[16], 0xEA);
        let note = r.warnings.first().expect("one note");
        assert_eq!(note.message, "hello there");
        assert_eq!(note.kind, crate::engine::WarningKind::Note);
        assert!(note.to_string().contains("note:"), "got `{note}`");
    }

    /// `.out` takes one string: ca65 answers a value list with `Unexpected
    /// trailing garbage characters` and a bare number with `String constant
    /// expected`. Both are refused here too.
    #[test]
    fn out_refuses_anything_but_one_string() {
        for src in [
            ".segment \"CODE\"\n.out \"a\", 5\n",
            ".segment \"CODE\"\n.out 5\n",
        ] {
            let err = crate::assemble_ca65(src).expect_err(src);
            assert!(
                err.to_string().contains("takes one string")
                    || err.to_string().contains("expected a string"),
                "got `{err}`"
            );
        }
    }

    /// `.export` and `.exportzp` require the name be defined somewhere in the
    /// program — ca65 answers `Exported symbol 'nope' was never defined` with
    /// nothing referencing it, which is what makes this a check and not a
    /// no-op. The name may be defined below the directive.
    #[test]
    fn an_exported_name_must_be_defined() {
        for word in [".export", ".exportzp"] {
            let src = format!(".segment \"CODE\"\n{word} foo\nfoo: .byte 1\n");
            crate::assemble_ca65(&src).expect("defined below");

            let src = format!(".segment \"CODE\"\n{word} nope\n.byte 1\n");
            let err = crate::assemble_ca65(&src).expect_err(word);
            assert!(
                err.to_string().contains("`nope` was never defined"),
                "{word}: got `{err}`"
            );
        }
        // A list, and repeating an export is not an error.
        crate::assemble_ca65(
            ".segment \"CODE\"\n.export foo, bar\n.export foo\nfoo: .byte 1\nbar: .byte 2\n",
        )
        .expect("a list");
    }

    /// `.export name := expr` defines the name as it exports it. Only the
    /// export spellings take the form: ca65 answers `Unexpected trailing
    /// garbage characters` for `.global bar := 9` and `.import baz := 3`.
    #[test]
    fn export_assigns_as_well_as_exports() {
        let r = crate::assemble_ca65(".segment \"CODE\"\n.export k := 7\n.byte k\n")
            .expect("defines k");
        assert_eq!(r.bytes[16], 7);
        for word in [".global", ".import"] {
            let src = format!(".segment \"CODE\"\n{word} v := 1\n.byte 1\n");
            let err = crate::assemble_ca65(&src).expect_err(word);
            assert!(
                err.to_string().contains("takes no value"),
                "{word}: got `{err}`"
            );
        }
    }

    /// `.import` claims a name defined elsewhere, so defining it here is
    /// ca65's `Symbol 'zz' is already an import` — reported at the definition,
    /// not at the import. Leaving it undefined is fine until something reads
    /// it, and then it is the ordinary undefined-symbol refusal (ld65 calls it
    /// an unresolved external).
    #[test]
    fn an_imported_name_may_not_be_defined_here() {
        for word in [".import", ".importzp"] {
            let src = format!(".segment \"CODE\"\n{word} zz\n.byte 1\n");
            crate::assemble_ca65(&src).expect("unreferenced is fine");

            let src = format!(".segment \"CODE\"\n{word} zz\n.byte 1\nzz: .byte 2\n");
            let err = crate::assemble_ca65(&src).expect_err(word);
            let message = err.to_string();
            assert!(message.contains("already an import"), "{word}: {message}");
            assert!(
                message.contains("line 4"),
                "reported at the definition: {message}"
            );

            let src = format!(".segment \"CODE\"\n{word} zz\n.byte zz\n");
            crate::assemble_ca65(&src).expect_err("referencing it cannot resolve");
        }
    }

    /// `.global` is export-if-defined and import-if-not, so both are legal and
    /// there is nothing to check. The `zp` spelling still warns.
    #[test]
    fn global_checks_nothing_either_way() {
        crate::assemble_ca65(".segment \"CODE\"\n.global g\ng: .byte 1\n").expect("defined");
        crate::assemble_ca65(".segment \"CODE\"\n.global g\n.byte 1\n").expect("not defined");
    }

    /// The `zp` spellings warn for a **label** that is not in the zero page.
    /// A constant never draws the warning, whatever its value — probed with
    /// `K = 7` and `K = $10`, both silent.
    #[test]
    fn exporting_an_absolute_label_as_zeropage_warns() {
        let r = crate::assemble_ca65(".segment \"CODE\"\n.exportzp foo\nfoo: .byte 1\n")
            .expect("assembles");
        let w = r.warnings.first().expect("one warning");
        assert!(w.message.contains("absolute but exported zeropage"), "{w}");

        for quiet in [
            ".segment \"ZEROPAGE\"\np: .res 1\n.segment \"CODE\"\n.exportzp p\n.byte 1\n",
            ".segment \"CODE\"\nK = 7\n.exportzp K\n.byte 1\n",
            ".segment \"CODE\"\nK = $10\n.exportzp K\n.byte 1\n",
        ] {
            let r = crate::assemble_ca65(quiet).expect(quiet);
            assert!(r.warnings.is_empty(), "{quiet}: got {:?}", r.warnings);
        }
        // `.globalzp` carries the same warning — it exports when the name is
        // defined here, and this one is.
        let r = crate::assemble_ca65(".segment \"CODE\"\n.globalzp foo\nfoo: .byte 1\n")
            .expect("assembles");
        assert_eq!(r.warnings.len(), 1);
    }

    /// `.forceimport` cannot be satisfied: defining the name is `already an
    /// import`, and not defining it is an unresolved external at ld65 even
    /// with nothing referencing it. Refused rather than counted a gap.
    #[test]
    fn forceimport_is_refused_as_the_reference_refuses_it() {
        let err = crate::assemble_ca65(".segment \"CODE\"\n.forceimport zz\n.byte 1\n")
            .expect_err("cannot be satisfied");
        let message = err.to_string();
        assert!(message.contains("linker resolves it"), "{message}");
        assert!(!message.contains("does not implement"), "{message}");
    }

    /// `.autoimport +`/`-` switches ca65's automatic runtime imports. There is
    /// no runtime library here and it emits nothing.
    #[test]
    fn autoimport_is_accepted_and_discarded() {
        for sign in ["+", "-"] {
            let r =
                crate::assemble_ca65(&format!(".segment \"CODE\"\n.autoimport {sign}\n.byte 1\n"))
                    .expect(sign);
            assert_eq!(r.bytes[16], 1);
        }
    }

    /// `.warning` says its piece and assembles; `.error` and `.fatal` do not.
    /// ca65 prefixes the text with `User warning:`/`User error:` — its own
    /// classification, not the source's words, so it is not reproduced.
    #[test]
    fn the_message_words_carry_their_severity() {
        let r = crate::assemble_ca65(".segment \"CODE\"\n.warning \"soft\"\nnop\n")
            .expect("a warning assembles");
        assert_eq!(r.bytes[16], 0xEA);
        let w = r.warnings.first().expect("one warning");
        assert_eq!(w.message, "soft");
        assert_eq!(w.kind, crate::engine::WarningKind::Advisory);

        for word in ["error", "fatal"] {
            let src = format!(".segment \"CODE\"\n.{word} \"hard\"\nnop\n");
            let err = crate::assemble_ca65(&src).expect_err(&src);
            assert!(err.to_string().contains("hard"), "got `{err}`");
        }
    }

    /// The message words read one string and skip the rest of the line, where
    /// `.out` refuses the same shape. Both spellings were probed; the manual
    /// reads alike for the two.
    #[test]
    fn a_message_word_ignores_what_follows_its_string() {
        for src in [
            ".segment \"CODE\"\n.warning \"a\", 5\nnop\n",
            ".segment \"CODE\"\n.warning \"a\" \"b\"\nnop\n",
        ] {
            let r = crate::assemble_ca65(src).expect(src);
            assert_eq!(r.warnings.first().expect("one warning").message, "a");
        }
        let err = crate::assemble_ca65(".segment \"CODE\"\n.warning 5\n").expect_err("no string");
        assert!(err.to_string().contains("expected a string"), "got `{err}`");
    }

    /// A message inside an untaken branch is never said: conditionals fold
    /// before layout, so the statement does not reach the emit pass.
    #[test]
    fn a_message_in_an_untaken_branch_stays_quiet() {
        let r = crate::assemble_ca65(
            ".segment \"CODE\"\n.if 0\n.error \"no\"\n.warning \"nor\"\n.endif\nnop\n",
        )
        .expect("assembles");
        assert!(r.warnings.is_empty(), "got `{:?}`", r.warnings);
    }

    /// `.assert` fires only when its condition is zero, and the action decides
    /// whether that stops the assembly. The default message is ca65's own.
    #[test]
    fn assert_fires_on_zero_and_the_action_decides() {
        let r = crate::assemble_ca65(".segment \"CODE\"\n.assert 1, error, \"never\"\nnop\n")
            .expect("true assertion is silent");
        assert!(r.warnings.is_empty(), "got `{:?}`", r.warnings);

        let r = crate::assemble_ca65(".segment \"CODE\"\n.assert 0, warning, \"soft\"\nnop\n")
            .expect("a warning assembles anyway");
        assert_eq!(r.bytes[16], 0xEA);
        let w = r.warnings.first().expect("one warning");
        assert_eq!(w.message, "soft");
        assert_eq!(w.kind, crate::engine::WarningKind::Advisory);

        let err = crate::assemble_ca65(".segment \"CODE\"\n.assert 0, error, \"boom\"\nnop\n")
            .expect_err("an error does not");
        assert!(err.to_string().contains("boom"), "got `{err}`");

        let r = crate::assemble_ca65(".segment \"CODE\"\n.assert 0, warning\nnop\n")
            .expect("assembles");
        assert_eq!(
            r.warnings.first().expect("one warning").message,
            "Assertion failed"
        );
    }

    /// The condition folds against the finished symbol table, so it may name a
    /// label defined below it. Real ca65 defers such an assertion to `ld65`;
    /// with assembly and linking fused there is no moment between the two.
    #[test]
    fn assert_sees_a_label_defined_below_it() {
        let src = ".segment \"CODE\"\n.assert later = $8001, error, \"moved\"\nnop\nlater:\n";
        crate::assemble_ca65(src).expect("the label is $8001");
        let src = ".segment \"CODE\"\n.assert later = $1234, error, \"moved\"\nnop\nlater:\n";
        let err = crate::assemble_ca65(src).expect_err("it is not $1234");
        assert!(err.to_string().contains("moved"), "got `{err}`");
    }

    /// The four action words ca65 accepts, and nothing else. `lderror` and
    /// `ldwarning` name the link stage, which is not a separate step here.
    #[test]
    fn assert_takes_the_four_action_words() {
        for (action, fatal) in [
            ("warning", false),
            ("ldwarning", false),
            ("error", true),
            ("lderror", true),
        ] {
            let src = format!(".segment \"CODE\"\n.assert 0, {action}, \"m\"\nnop\n");
            let r = crate::assemble_ca65(&src);
            assert_eq!(r.is_err(), fatal, "{action}");
        }
        let err = crate::assemble_ca65(".segment \"CODE\"\n.assert 0, oops, \"m\"\n")
            .expect_err("bad action");
        assert!(err.to_string().contains("action"), "got `{err}`");
    }

    #[test]
    fn a_conditional_picks_one_branch() {
        assert_eq!(bytes("N=1\n.if N\n nop\n.endif\n rts\n")[..2], [0xEA, 0x60]);
        assert_eq!(bytes(".if 0\n nop\n.else\n rts\n.endif\n")[..1], [0x60]);
        assert_eq!(
            bytes(
                ".if 0\n lda #1\n.elseif 0\n lda #2\n.elseif 1\n lda #3\n.else\n lda #4\n.endif\n"
            )[..2],
            [0xA9, 0x03]
        );
    }

    #[test]
    fn a_label_may_share_a_conditional_or_repetition_head() {
        let image = rom(
            ".segment \"CODE\"\nLB: .if 1\n .byte $11\n.endif\nLR: .repeat 1\n .byte $22\n.endrepeat\n .word LB, LR\n",
        );
        assert_eq!(&image[16..22], &[0x11, 0x22, 0x00, 0x80, 0x01, 0x80]);
    }

    #[test]
    fn a_dead_conditional_branch_need_not_parse() {
        assert_eq!(bytes(".if 0\n .byte\n.endif\n .byte 1\n")[..1], [1]);
        crate::assemble_ca65(".if 1\n .byte\n.endif\n")
            .expect_err("the same malformed line is an error when reached");
    }

    #[test]
    fn a_blank_macro_argument_may_leave_invalid_source_in_the_dead_branch() {
        assert_eq!(
            bytes(
                ".macro fill v\n .ifblank v\n .byte $ff\n .else\n .byte v\n .endif\n.endmacro\n fill\n"
            )[..1],
            [0xFF]
        );
    }

    /// `.ifdef` tests what is defined **above** it. A definition inside an
    /// untaken branch is invisible afterwards, which is ca65's rule and the
    /// shared walk's `emit = false` rule alike.
    #[test]
    fn an_untaken_branch_defines_nothing() {
        assert_eq!(
            bytes(".if 0\nN = 5\n.endif\n.ifdef N\n nop\n.endif\n rts\n")[..1],
            [0x60]
        );
    }

    /// A condition folds against the constants above it and **may not reach
    /// forward** — ca65 answers `Constant expression expected`, and a ca65
    /// label is never constant because `ld65` relocates it.
    #[test]
    fn a_condition_may_not_reach_forward() {
        crate::assemble_ca65(".if later\n nop\n.endif\nlater = 1\n")
            .expect_err("ca65: Constant expression expected");
    }

    #[test]
    fn a_repetition_assembles_its_body_n_times() {
        assert_eq!(
            bytes(".repeat 3\n nop\n.endrepeat\n")[..3],
            [0xEA, 0xEA, 0xEA]
        );
        assert_eq!(bytes(".repeat 0\n nop\n.endrepeat\n rts\n")[..1], [0x60]);
        crate::assemble_ca65(".repeat -1\n nop\n.endrepeat\n").expect_err("ca65: Range error");
    }

    /// The loop variable is **0-based**, and baked into each use — ca65
    /// resolves a symbol once, in a later pass, against one table, and a loop
    /// variable holds a different value on every iteration.
    #[test]
    fn the_loop_variable_is_zero_based_and_live() {
        assert_eq!(
            bytes(".repeat 3, i\n lda #i\n.endrepeat\n")[..6],
            [0xA9, 0x00, 0xA9, 0x01, 0xA9, 0x02]
        );
        assert_eq!(
            bytes(".repeat 4, i\n .byte i*2\n.endrepeat\n")[..4],
            [0, 2, 4, 6]
        );
        // A nested loop shadows, innermost first.
        assert_eq!(
            bytes(".repeat 2, i\n.repeat 2, j\n lda #i*16+j\n.endrepeat\n.endrepeat\n")[..8],
            [0xA9, 0x00, 0xA9, 0x01, 0xA9, 0x10, 0xA9, 0x11]
        );
        // And a condition inside the body sees it.
        assert_eq!(
            bytes(".repeat 3, i\n.if i\n nop\n.endif\n.endrepeat\n rts\n")[..3],
            [0xEA, 0xEA, 0x60]
        );
    }

    /// **Scoped to the loop**, unlike acme's `!for` variable which survives its
    /// block. Two dialects, two rules — which is the point of arbitrating each
    /// against its own reference rather than sharing one.
    #[test]
    fn the_loop_variable_does_not_outlive_the_loop() {
        crate::assemble_ca65(".repeat 2, i\n nop\n.endrepeat\n lda #i\n")
            .expect_err("ca65: Symbol 'i' is undefined");
    }

    /// A directive ca65 has and we do not is refused as *that*, and a word it
    /// does not have is refused as that. The fall-through used to answer
    /// `unsupported directive` for both, which tells a reader with valid
    /// source to go looking for a typo.
    #[test]
    fn a_real_directive_is_told_apart_from_a_typo() {
        let err = |src: &str| super::assemble(src).expect_err(src).to_string();
        // `.export` sat here, then `.condes`, each implemented in turn.
        for d in [".charmap $41, $42", ".macpack cpu"] {
            let e = err(&format!("\t{d}\n"));
            assert!(
                e.contains("is a real directive here"),
                "`{d}` should name itself a real ca65 directive, got: {e}"
            );
        }
        assert!(
            err("\t.zzqq\n").contains("is not a control command ca65 knows"),
            "and a word ca65 does not have should say so"
        );
    }

    #[test]
    fn text_defines_substitute_and_can_be_undefined() {
        let rom = rom(
            ".segment \"CODE\"\n.define VALUE $42\n.byte VALUE\n.undefine VALUE\n\
             .ifndef VALUE\n.byte $43\n.endif\n",
        );
        assert_eq!(&rom[16..18], &[0x42, 0x43]);
    }

    /// #481, probe-pinned against ca65 V2.18: an untaken branch's lines are
    /// never read, so a definition inside one is invisible afterwards — and
    /// so is anything else in there, a line that would not parse included.
    #[test]
    fn an_untaken_branch_defines_nothing_and_need_not_parse() {
        let rom = rom(
            ".segment \"CODE\"\n.if 0\n.define six 6\nthis is lexable garbage but not a statement\n\
             .end\n.endif\n.ifdef six\n.byte 6\n.else\n.byte 1\n.endif\n",
        );
        assert_eq!(rom[16], 1, "the untaken definition must be invisible");
    }

    /// #481: a taken branch's definition flows to the rest of the source —
    /// the following top-level line included (probed: `a9 06`).
    #[test]
    fn a_taken_branch_definition_flows_on() {
        let first = rom(".segment \"CODE\"\n.if 1\n.define six 6\n.endif\n.byte six\n");
        assert_eq!(first[16], 6);
        // ... across a later block too.
        let second =
            rom(".segment \"CODE\"\n.if 1\n.define five 5\n.endif\n.if 1\n.byte five\n.endif\n");
        assert_eq!(second[16], 5);
    }

    /// #481: `.undefine` has the same positional, selection-time semantics.
    #[test]
    fn an_undefine_inside_a_taken_branch_takes_effect() {
        let rom = rom(
            ".segment \"CODE\"\n.define six 6\n.if 1\n.undefine six\n.endif\n\
             .ifdef six\n.byte 6\n.else\n.byte 2\n.endif\n",
        );
        assert_eq!(rom[16], 2);
    }

    /// #481, the shape the old diagnostic refused and ca65 accepts (probed:
    /// `a9 07` three times): each iteration defines, uses, and undefines in
    /// execution order.
    #[test]
    fn a_repeat_body_applies_definitions_per_iteration() {
        let rom = rom(
            ".segment \"CODE\"\n.repeat 3\n.define val 7\n.byte val\n.undefine val\n.endrepeat\n",
        );
        assert_eq!(&rom[16..19], &[7, 7, 7]);
    }

    /// Probed: ca65 refuses a second `.define` of a live name — which is what
    /// makes the iteration above need its `.undefine`.
    #[test]
    fn redefining_a_live_text_symbol_is_refused() {
        let error = crate::assemble_ca65(
            ".segment \"CODE\"\n.repeat 2\n.define val 7\n.byte val\n.endrepeat\n",
        )
        .expect_err("iteration two redefines");
        assert!(error.to_string().contains("already defined"), "{error}");
        let error = crate::assemble_ca65(".define one 1\n.define one 2\n")
            .expect_err("ca65 refuses to redefine");
        assert!(error.to_string().contains("already defined"), "{error}");
    }

    /// Probed: `.undefine` of a name that is not defined is an error
    /// (`No such macro`), not a no-op.
    #[test]
    fn undefining_an_unknown_text_symbol_is_refused() {
        let error = crate::assemble_ca65(".undefine nothing\n")
            .expect_err("ca65 refuses to undefine the undefined");
        assert!(
            error.to_string().contains("no define-style macro is named"),
            "{error}"
        );
    }

    /// Probed: `.ifdef` of a *live* text symbol substitutes the argument like
    /// any other use — `.define hi 8` then `.ifdef hi` is ca65's `Identifier
    /// expected`, because the question lands on the value.
    #[test]
    fn ifdef_of_a_live_text_symbol_lands_on_its_value() {
        let error = crate::assemble_ca65(".define hi 8\n.ifdef hi\n.byte 1\n.endif\n")
            .expect_err("the argument was substituted");
        assert!(error.to_string().contains("expected a name"), "{error}");
    }

    /// Probed: `.end` inside a taken branch stops the reader before the
    /// closer, and ca65 reports the block unclosed even though the text
    /// closes it further down.
    #[test]
    fn end_inside_a_taken_branch_leaves_the_block_unclosed() {
        let error =
            crate::assemble_ca65(".segment \"CODE\"\n.if 1\n.byte 4\n.end\n.endif\n.byte 5\n")
                .expect_err("the reader stops before the closer");
        assert!(error.to_string().contains("never closed"), "{error}");
    }

    /// #481: an untaken branch touches no state — a label inside one neither
    /// binds nor resets the cheap-local scope, and a constant neither.
    #[test]
    fn an_untaken_branch_touches_no_binding_state() {
        // The cheap local before the block still resolves across it (probed).
        let rom = rom(".segment \"CODE\"\nfoo:\n@b: .byte 1\n.if 0\nother:\n.endif\n.word @b\n");
        assert_eq!(&rom[16..19], &[1, 0x00, 0x80]);
        // The constant never folds (probed: `Symbol 'k' is undefined`).
        let error = crate::assemble_ca65(".segment \"CODE\"\n.if 0\nk = 9\n.endif\n.byte k\n")
            .expect_err("an untaken constant must not leak");
        assert!(error.to_string().contains("k"), "{error}");
    }

    /// ca65 spells every operator twice, and both spellings are one operator:
    /// the keyword lands on its symbol twin's token, so it inherits the same
    /// precedence rather than getting a second, nearly-right one.
    #[test]
    fn a_keyword_operator_is_its_symbol_twin() {
        let byte = |src: &str| {
            super::assemble(&format!(".segment \"CODE\"\n.byte {src}\n"))
                .unwrap_or_else(|e| panic!("{src}: {e}"))
                .0
        };
        for (keyword, symbol) in [
            ("4 + 1 .bitand 1", "4 + 1 & 1"),
            ("4 + 2 .bitor 1", "4 + 2 | 1"),
            ("6 .bitxor 3 + 1", "6 ^ 3 + 1"),
            ("1 .shl 2 + 1", "1 << 2 + 1"),
            ("$80 .shr 3", "$80 >> 3"),
            (".bitnot 0 .bitand $FF", "~0 & $FF"),
            ("1 .and 0 .or 1", "1 && 0 || 1"),
            ("0 .or 1 .and 0", "0 || 1 && 0"),
            (".not 1 .or 1", "! 1 || 1"),
        ] {
            assert_eq!(byte(keyword), byte(symbol), "`{keyword}` vs `{symbol}`");
        }
    }

    /// The two things about this layer that are not the obvious reading, both
    /// measured against ca65 V2.18.
    #[test]
    fn the_logical_layer_reads_as_ca65_reads_it() {
        // The value is fenced, because the linked image is padded and a `0`
        // answer would otherwise be indistinguishable from the padding.
        let byte = |src: &str| {
            let image = super::assemble(&format!(".segment \"CODE\"\n.byte $AA,{src},$AA\n"))
                .unwrap_or_else(|e| panic!("{src}: {e}"))
                .0;
            let at = image
                .windows(3)
                .position(|w| w[0] == 0xAA && w[2] == 0xAA)
                .unwrap_or_else(|| panic!("{src}: no fenced value in the image"));
            vec![image[at + 1]]
        };
        // `.not` binds looser than everything, so the `.or` happens first and
        // the `.not` negates the lot. Read as an ordinary prefix this is 1.
        assert_eq!(byte(".not 1 .or 1"), vec![0]);
        // The logical operators answer 1 or 0 whatever they are given, where
        // the bitwise ones answer the bits.
        assert_eq!(byte("2 .and 3"), vec![1]);
        assert_eq!(byte("2 .bitand 3"), vec![2]);
        // `.mod` sits with `*` and `/`, and takes C's sign rule.
        assert_eq!(byte("7 .mod 4 + 1"), vec![4]);
        assert_eq!(byte("7 .mod (0 - 4)"), vec![3]);
    }

    /// The plural extractors, over the NES path — the differential covers the
    /// flat 65816 dialect, and this dialect has its own directive table.
    #[test]
    fn the_extractors_take_a_byte_from_each_value() {
        // The image is a linked NES ROM — header, then padded segments — so the
        // values are found behind a `$AA,$55` fence rather than at a fixed
        // offset.
        let bytes = |src: &str| {
            let image = super::assemble(&format!(".segment \"CODE\"\n .byte $AA,$55\n{src}\n"))
                .unwrap_or_else(|e| panic!("{src}: {e}"))
                .0;
            let at = image
                .windows(2)
                .position(|w| w == [0xAA, 0x55])
                .unwrap_or_else(|| panic!("{src}: no fence in the image"));
            image[at + 2..].to_vec()
        };
        assert_eq!(&bytes(" .lobytes $1234, $5678")[..2], &[0x34, 0x78]);
        assert_eq!(&bytes(" .hibytes $1234, $5678")[..2], &[0x12, 0x56]);
        // Byte 2 of the value, above 24 bits as well as within them.
        assert_eq!(&bytes(" .bankbytes $123456, $12345678")[..2], &[0x12, 0x34]);
        // Three bytes, little-endian, per value.
        assert_eq!(
            &bytes(" .faraddr $112233, $445566")[..6],
            &[0x33, 0x22, 0x11, 0x66, 0x55, 0x44]
        );
        // Masked, not refused: ca65 answers `.hibytes 0-1` with $FF.
        assert_eq!(
            &bytes(" .lobytes 0-1\n .hibytes 0-1\n .bankbytes 0-1")[..3],
            &[0xFF; 3]
        );
    }

    /// `.forceword` is a word ca65's tokenizer knows and its parser takes
    /// nowhere — statement position, after a label, as a struct storage
    /// allocator, as an address-size specifier, inside an expanded macro: every
    /// one answers `Unexpected '.FORCEWORD'`. Refusing it is matching V2.18,
    /// not a gap, so it reads as a refusal rather than as an unimplemented
    /// directive.
    #[test]
    fn a_word_the_reference_never_accepts_is_refused_as_one() {
        let e = super::assemble(".segment \"CODE\"\n .forceword $12\n")
            .expect_err("ca65 refuses it, so we must");
        let text = e.to_string();
        assert!(
            text.contains("accepts in no position"),
            "should name the reference's own refusal, got: {text}"
        );
        assert!(
            !text.contains("the gap is ours"),
            "and must not read as an unimplemented directive: {text}"
        );
    }

    /// The nine conditional heads beyond `.if`/`.ifdef`/`.ifndef`, each folded
    /// the way ca65 V2.18 folds it.
    #[test]
    fn the_conditional_heads_fold_as_ca65_folds_them() {
        // Each case is a head, then the byte the taken branch emits.
        let taken = |head: &str| {
            let src = format!(
                ".segment \"CODE\"\nV = 5\nLA:\nLB:\n{head}\n .byte $11\n .else\n .byte $22\n .endif\nLC: .byte 0\n"
            );
            let image = super::assemble(&src)
                .unwrap_or_else(|e| panic!("{head}: {e}"))
                .0;
            let at = image
                .iter()
                .position(|&b| b == 0x11 || b == 0x22)
                .unwrap_or_else(|| panic!("{head}: neither branch emitted"));
            image[at]
        };
        // `.ifblank` asks whether anything follows it on the line.
        assert_eq!(taken(".ifblank"), 0x11);
        assert_eq!(taken(".ifblank x"), 0x22);
        assert_eq!(taken(".ifnblank"), 0x22);
        assert_eq!(taken(".ifnblank x"), 0x11);
        // `.ifconst` asks whether the expression *is* constant, and answers
        // rather than failing when it is not.
        assert_eq!(taken(".ifconst 1+1"), 0x11);
        assert_eq!(taken(".ifconst V*2"), 0x11);
        assert_eq!(taken(".ifnconst 1"), 0x22);
        // A label is not constant; a difference of two labels above the line in
        // one segment is; a forward one is not, and neither is a label that has
        // been multiplied.
        assert_eq!(taken(".ifconst LB-LA"), 0x11);
        assert_eq!(taken(".ifconst LA"), 0x22);
        // `LC` stands below the line, and a name this line has not seen yet is
        // not constant however it is used.
        assert_eq!(taken(".ifconst LC-LA"), 0x22);
        assert_eq!(taken(".ifconst LA*2"), 0x22);
        assert_eq!(taken(".ifconst .lobyte(LA)"), 0x22);
        // `*` is a label of the active segment, so it cancels against itself
        // and against nothing else.
        assert_eq!(taken(".ifconst *-*"), 0x11);
        assert_eq!(taken(".ifconst *"), 0x22);
        // The CPU tests: this leg is a 6502 and refuses `.setcpu`, so the
        // answer cannot change while a file is read.
        assert_eq!(taken(".ifp02"), 0x11);
        for head in [".ifp816", ".ifpc02", ".ifpsc02", ".ifp4510"] {
            assert_eq!(taken(head), 0x22, "{head}");
        }
    }

    /// What a scope does to a name, measured against ca65 V2.18. The
    /// byte-for-byte agreement is the differential fixture's job; these are the
    /// rules that fixture would not show if it broke.
    #[test]
    fn a_scope_qualifies_the_names_inside_it() {
        // A name in two procs is two symbols, and each `.word` reads its own.
        let image = rom(".segment \"CODE\"\n\
             .proc one\ninner: nop\n.endproc\n\
             .proc two\ninner: nop\n.endproc\n\
             .word one::inner, two::inner, one, two\n");
        assert_eq!(&image[16..20], &[0xEA, 0xEA, 0x00, 0x80]);
        assert_eq!(&image[20..24], &[0x01, 0x80, 0x00, 0x80]);

        // `.scope` opens one and defines nothing, so its name is not a symbol.
        super::assemble(".segment \"CODE\"\n.scope s\n nop\n.endscope\n .word s\n")
            .expect_err("ca65: Symbol 's' is undefined");

        // Lookup walks outward, and `::` is the top level from anywhere.
        let image = rom(".segment \"CODE\"\nv = $11\n.proc p\nv = $22\n .byte v, ::v\n.endproc\n");
        assert_eq!(&image[16..18], &[0x22, 0x11]);

        // A name defined inside is invisible outside it.
        super::assemble(".segment \"CODE\"\n.proc p\ninner: nop\n.endproc\n .word inner\n")
            .expect_err("ca65: Symbol 'inner' is undefined");
    }

    /// The ways a scope can be written wrongly, each answered the way ca65
    /// answers it.
    #[test]
    fn a_scope_has_to_be_opened_and_closed() {
        let err = |src: &str| {
            super::assemble(&format!(".segment \"CODE\"\n{src}"))
                .expect_err(src)
                .to_string()
        };
        // The closers are paired by kind: ca65 answers `.endscope` over an open
        // `.proc` with `No open .SCOPE`, not with a mismatch report.
        assert!(err(".proc p\n nop\n.endscope\n").contains("No open .SCOPE"));
        assert!(err(".scope s\n nop\n.endproc\n").contains("No open .PROC"));
        assert!(err(".endproc\n").contains("No open .PROC"));
        // ca65: `Local scope was not closed`.
        assert!(err(".proc p\n nop\n").contains("was not closed"));
        // ca65: `Duplicate scope 'sa'` — one name per parent.
        assert!(err(".scope sa\n.endscope\n.scope sa\n.endscope\n").contains("Duplicate scope"));
        // ca65 V2.19 reads a scope named before it is opened: `p::inner` is
        // the label inside the `.proc` below (measured).
        assert!(
            super::assemble(".segment \"CODE\"\n .word p::inner\n.proc p\ninner: nop\n.endproc\n")
                .is_ok()
        );
    }

    /// Cheap locals belong to the scope they are written in, so the same `@l`
    /// in two procs is two labels rather than a duplicate-symbol error.
    #[test]
    fn a_cheap_local_belongs_to_its_scope() {
        let image = rom(".segment \"CODE\"\n\
             .proc one\n@l: nop\n bne @l\n.endproc\n\
             .proc two\n@l: nop\n bne @l\n.endproc\n");
        // Each branch reaches its own `@l`: nop, bne -3, twice over.
        assert_eq!(&image[16..22], &[0xEA, 0xD0, 0xFD, 0xEA, 0xD0, 0xFD]);
    }

    /// The listing words are accepted and change nothing; the option words that
    /// change the parse are still refused, and say so.
    #[test]
    fn a_listing_word_is_accepted_and_a_parse_option_is_not() {
        let plain = rom(".segment \"CODE\"\n .byte $AA\n");
        let noisy = rom(".segment \"CODE\"\n\
             .debuginfo on\n .fileopt comment, \"x\"\n .fopt author, \"y\"\n\
             .list off\n .listbytes 4\n .pagelen 60\n .pagelength 60\n .dbg line\n\
             .byte $AA\n");
        assert_eq!(plain, noisy, "the listing words change no byte");
        // `.linecont +` and `.case off` change what the parse does, so they are
        // a gap rather than something to discard.
        for word in [".linecont +", ".case off", ".feature labels_without_colons"] {
            let e = super::assemble(&format!(".segment \"CODE\"\n {word}\n"))
                .expect_err(word)
                .to_string();
            assert!(
                e.contains("is a real directive here"),
                "`{word}` should read as our gap, got: {e}"
            );
        }
    }

    /// A record declares constants and emits nothing; `.tag` is what emits.
    #[test]
    fn a_record_declares_a_layout() {
        let image = rom(".segment \"CODE\"\n\
             .struct Point\npx .byte\npy .byte\npw .word\n.endstruct\n\
             .union U\nua .byte\nub .word\n.endunion\n\
             .enum E\nea\neb = 10\nec\n.endenum\n\
             .byte Point::px, Point::py, Point::pw, .sizeof(Point)\n\
             .byte U::ua, U::ub, .sizeof(U)\n\
             .byte E::ea, E::eb, E::ec\n");
        // Struct fields run on; a union lays every member at zero and is as
        // wide as its widest; an enum counts on from an explicit value.
        assert_eq!(&image[16..20], &[0, 1, 2, 4]);
        assert_eq!(&image[20..23], &[0, 0, 2]);
        assert_eq!(&image[23..26], &[0, 10, 11]);

        // `.tag` reserves one instance, and the label binds at its start.
        let image = rom(".segment \"CODE\"\n\
             .struct Point\npx .byte\npy .byte\n.endstruct\n\
             .byte $AA\np: .tag Point\n .word p\n");
        assert_eq!(&image[16..21], &[0xAA, 0, 0, 0x01, 0x80]);
    }

    /// The ways a record can be written wrongly.
    #[test]
    fn a_record_has_to_be_closed_by_its_own_word() {
        let err = |src: &str| {
            super::assemble(&format!(".segment \"CODE\"\n{src}"))
                .expect_err(src)
                .to_string()
        };
        // ca65 pairs them strictly: inside an `.enum`, `.endstruct` is read as
        // a member name and answered `Identifier expected`.
        assert!(err(".union U\nua .byte\n.endstruct\n").contains("Expected `.ENDUNION`"));
        assert!(err(".struct S\nsa .byte\n").contains("Expected `.ENDSTRUCT`"));
        assert!(err(".endstruct\n").contains("Unexpected end of a record"));
        // A storage allocator ca65 does not have, and a `.tag` naming nothing.
        assert!(err(".struct S\nsa .zzz\n.endstruct\n").contains("not a control command"));
        assert!(err(".struct S\nsa .tag Nope\n.endstruct\n").contains("Unknown struct/union"));
        // A *field* may repeat (`.tag Inner 3`); a statement may not, which is
        // ca65's `Unexpected trailing garbage characters`.
        assert!(
            err(".struct S\nsa .byte\n.endstruct\nq: .tag S 2\n")
                .contains("expected the end of the line")
        );
    }

    /// Naming the processor this leg assembles changes nothing; naming another
    /// is our gap, and naming a word ca65 does not know is a fault in the
    /// source. Three different answers, and the third is ca65's own.
    #[test]
    fn the_processor_words_accept_the_processor_we_assemble() {
        let plain = rom(".segment \"CODE\"\n lda #1\n");
        let named = rom(".segment \"CODE\"\n.setcpu \"6502\"\n.p02\n.smart on\n\
             .pushcpu\n.popcpu\n lda #1\n");
        assert_eq!(plain, named, "naming a 6502 changes no byte");

        let err = |src: &str| {
            super::assemble(&format!(".segment \"CODE\"\n{src}\n nop\n"))
                .expect_err(src)
                .to_string()
        };
        // ca65's other processors: each a different opcode set, so taking one
        // silently would assemble the wrong instructions. They come with the
        // other CPUs, stage 4 of Lang198x's plan.
        for src in [".setcpu \"6502DTV\"", ".pdtv", ".psweet16"] {
            assert!(
                err(src).contains("is not lowered yet"),
                "`{src}` should read as our gap: {}",
                err(src)
            );
        }
        // A name ca65 does not know is its `CPU not supported`, not our gap.
        let e = err(".setcpu \"zz\"");
        assert!(e.contains("CPU not supported"), "got: {e}");
        // The width commands are the 65816's, and ca65 refuses them here too.
        let e = err(".a16");
        assert!(e.contains("only valid in 65816 mode"), "got: {e}");
    }

    /// `.ref` asks whether a name has been *used* above the line — a different
    /// question from `.defined`, answered from a different record.
    #[test]
    fn a_reference_is_counted_from_above_the_line() {
        let image = rom(".segment \"CODE\"\n\
             L: nop\n .byte .ref(L)\n .word L\n .byte .ref(L), .referenced(L)\n");
        assert_eq!(&image[16..21], &[0xEA, 0x00, 0x00, 0x80, 0x01]);
        assert_eq!(image[21], 0x01, "`.referenced` is the long spelling");

        // A use inside a branch that is never taken is not a use: the dead leg
        // never reaches the projection, which is where the record is kept.
        let image = rom(".segment \"CODE\"\nM: nop\n.if 0\n .word M\n.endif\n .byte .ref(M)\n");
        assert_eq!(&image[16..18], &[0xEA, 0x00]);

        // A name nothing has mentioned, and one that is never defined at all.
        let image = rom(".segment \"CODE\"\n .byte .ref(ZZ)\n");
        assert_eq!(image[16], 0x00);
    }

    /// `.ifref`/`.ifnref` branch on the same record.
    #[test]
    fn the_reference_conditionals_read_the_same_record() {
        let taken = |src: &str| {
            let image = rom(&format!(
                ".segment \"CODE\"\nL: nop\n{src}\n .byte $11\n .else\n .byte $22\n .endif\n"
            ));
            // The branch byte is found rather than indexed: a `.word L` before
            // the head moves it along.
            *image[16..]
                .iter()
                .find(|&&b| b == 0x11 || b == 0x22)
                .unwrap_or_else(|| panic!("{src}: neither branch emitted"))
        };
        assert_eq!(taken(".ifref L"), 0x22, "nothing has used `L` yet");
        assert_eq!(taken(" .word L\n.ifref L"), 0x11);
        assert_eq!(taken(".ifnref L"), 0x11);
        assert_eq!(taken(" .word L\n.ifnref L"), 0x22);
    }

    /// `.org` moves the address a statement reports and leaves its bytes where
    /// they were going; `.reloc` gives the address back to the segment.
    #[test]
    fn org_moves_the_address_and_not_the_bytes() {
        let image = rom(".segment \"CODE\"\n .byte $11\n\
             .org $2000\nL: .byte $22\n .word L, *\n\
             .reloc\nM: .byte $33\n .word M\n");
        // The `$22` still lands next to the `$11` — only the address moved.
        assert_eq!(&image[16..18], &[0x11, 0x22]);
        // `L` reads from the new origin. The first word advances the counter,
        // so `*` is three bytes past that origin when its operand is read.
        assert_eq!(&image[18..22], &[0x00, 0x20, 0x03, 0x20]);
        // After `.reloc` the segment's own addressing is back: `M` is the
        // seventh byte of CODE, which is based at $8000.
        assert_eq!(image[22], 0x33);
        assert_eq!(&image[23..25], &[0x06, 0x80]);
    }

    /// `.end` stops the assembler reading, so what follows is not parsed at
    /// all — not even far enough to be an error.
    #[test]
    fn end_stops_the_read() {
        let image = rom(".segment \"CODE\"\n .byte $11\n.end\n .byte $22\n\
             .zzqq this is not a directive and never becomes one\n");
        assert_eq!(image[16], 0x11);
        assert_eq!(image[17], 0x00, "nothing below `.end` was assembled");
    }

    /// A pseudo-function is declared as what it is.
    ///
    /// This test used to assert the opposite — that `.lobyte` and `.strlen`
    /// were deliberately absent, because "naming them here would describe them
    /// in a place they never appear". That objection was right about the
    /// model as it stood: every category described a *statement*, so declaring
    /// an expression function claimed it began a line.
    ///
    /// `Category::ExpressionWord` names the place instead, so the objection no
    /// longer holds and the eleven that work stop reading as gaps. What has
    /// not changed is the other half: a function we do **not** implement stays
    /// undeclared, because declaring it would claim we do.
    #[test]
    fn a_pseudo_function_is_declared_as_an_expression_word() {
        let ca65: Vec<crate::directives::Directive> = crate::directives::surfaces()
            .into_iter()
            .filter(|s| s.dialect == "ca65")
            .flat_map(|s| s.directives)
            .collect();
        let kind = |word: &str| {
            ca65.iter()
                .find(|d| d.spellings().iter().any(|s| s == word))
                .map(|d| d.category)
        };
        for f in [
            ".lobyte", ".strlen", ".max", ".defined", ".sizeof", ".tcount",
        ] {
            assert_eq!(
                kind(f),
                Some(crate::directives::Category::ExpressionWord),
                "`{f}` is implemented, so it is declared as what it is"
            );
        }
        for f in [".time", ".version"] {
            assert_eq!(
                kind(f),
                None,
                "`{f}` is not implemented, so nothing claims it"
            );
        }
        for d in [".export", ".proc", ".segment", ".byte"] {
            assert!(kind(d).is_some(), "`{d}` should be declared");
        }
    }
    #[test]
    fn single_source_numeric_admission_checks_skipped_captures() {
        for source in [
            ".if 0\n.byte <$100000000\n.endif\n",
            ".repeat 0\n.byte <$100000000\n.endrepeat\n",
            ".macro unused\n.byte <$100000000\n.endmacro\n",
        ] {
            let error = assemble(source).expect_err("native lexical overflow");
            let span = error.span.expect("source location");
            assert_eq!(span.file, crate::span::FileId(0));
            assert_eq!(span.line, 2);
        }
        assert!(
            assemble(
                ".if 0\n.include 3\n.setcpu \"not-a-cpu\"\n.byte MISSING,,\n.endif\n.byte 7\n"
            )
            .is_ok()
        );
        assert!(assemble(".end .sprintf(\"%d\",4294967300)\n.byte <$100000000\n").is_ok());
    }
}
