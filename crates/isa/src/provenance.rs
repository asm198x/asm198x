//! Where each instruction-set specification came from.
//!
//! The specs in this crate are **authored from datasheets and programming
//! manuals**, not extracted from an emulator's decode loop — see
//! `decisions/asm198x-and-shared-isa-spec.md`. This records which document each
//! one was authored from, so the claim is checkable rather than asserted.
//!
//! # Why a table rather than a field
//!
//! The CPUs do not share one shape. Fourteen use [`crate::InstructionSet`];
//! the 68000, 6809, Z8000, TMS9900, PDP-11 and CP1610 each have their own. A
//! field would have to be added to seven different types and would still miss
//! anything added later. One table keyed by module covers every CPU the same
//! way, and is short enough to audit against the library by eye.
//!
//! # `library` is a path, not a URL
//!
//! The primary reference library is a private repository. A citation here names
//! the document and where it sits in that library; it deliberately does not
//! pretend to be a link a reader can follow. Naming the document is what makes
//! the provenance checkable by anyone holding the same datasheet.
//!
//! # What is deliberately absent
//!
//! The CP1610's library folder also holds an instruction reference derived from
//! the jzIntv **emulator**. It is useful for cross-checking and it is not cited
//! here, because an emulator's decode loop is precisely the authority this
//! project's spec rules exclude.

/// One document a specification was authored from.
pub struct Source {
    /// The document's title, as printed on it.
    pub title: &'static str,
    /// Author or publisher, whichever the document leads with.
    pub attribution: &'static str,
    /// Year of the edition used, where the document states one.
    pub year: Option<&'static str>,
    /// Where it sits in the primary reference library.
    pub library: &'static str,
}

/// The sources behind one CPU's specification, keyed by its module name here.
pub struct Chip {
    pub module: &'static str,
    pub sources: &'static [Source],
}

/// Provenance for every CPU this crate specifies.
pub const PROVENANCE: &[Chip] = &[
    Chip {
        module: "mos6502",
        sources: &[
            Source {
                title: "6502 User's Manual",
                attribution: "MOS Technology",
                year: Some("1984"),
                library: "by-topic/cpu-6502/",
            },
            Source {
                title: "Programming the 6502",
                attribution: "Rodnay Zaks",
                year: Some("1983"),
                library: "by-topic/cpu-6502/",
            },
        ],
    },
    Chip {
        module: "mos65816",
        sources: &[Source {
            title: "Programming the 65816",
            attribution: "Eyes and Lichty",
            year: Some("1986"),
            library: "by-topic/cpu-65816/",
        }],
    },
    Chip {
        module: "huc6280",
        sources: &[
            Source {
                title: "HuC6280 Software Manual",
                attribution: "Hudson Soft",
                year: None,
                library: "by-topic/cpu-huc6280/",
            },
            Source {
                title: "HuC6280 Hardware Manual",
                attribution: "Hudson Soft",
                year: None,
                library: "by-topic/cpu-huc6280/",
            },
        ],
    },
    Chip {
        module: "z80",
        sources: &[
            Source {
                title: "Programming the Z80, 2nd edition",
                attribution: "Rodnay Zaks",
                year: Some("1980"),
                library: "by-topic/cpu-z80/",
            },
            Source {
                title: "Z80 Microcomputer Handbook",
                attribution: "Sams",
                year: Some("1985"),
                library: "by-topic/cpu-z80/",
            },
        ],
    },
    Chip {
        module: "sm83",
        sources: &[Source {
            title: "SM83 Programming Manual",
            attribution: "Sharp",
            year: None,
            library: "by-topic/cpu-sm83/",
        }],
    },
    Chip {
        module: "i8080",
        sources: &[
            Source {
                title: "8080/8085 Software Design, Book 2",
                attribution: "Intel",
                year: None,
                library: "by-topic/cpu-8080/",
            },
            Source {
                title: "8085AH Data Sheet",
                attribution: "Intel",
                year: None,
                library: "by-topic/cpu-8080/",
            },
        ],
    },
    Chip {
        module: "m6800",
        sources: &[Source {
            title: "MC6800 8-Bit Microprocessing Unit Data Sheet",
            attribution: "Motorola",
            year: None,
            library: "by-topic/cpu-6800/",
        }],
    },
    Chip {
        module: "cdp1802",
        sources: &[Source {
            title: "User Manual for the CDP1802 COSMAC Microprocessor",
            attribution: "RCA",
            year: None,
            library: "by-topic/cpu-cdp1802/",
        }],
    },
    Chip {
        module: "i8048",
        sources: &[Source {
            title: "MCS-48 Family User's Manual",
            attribution: "Intel",
            year: Some("1978"),
            library: "by-topic/cpu-8048/",
        }],
    },
    Chip {
        module: "scmp",
        sources: &[Source {
            title: "SC/MP Technical Description, 4200079A",
            attribution: "National Semiconductor",
            year: None,
            library: "by-topic/cpu-scmp/",
        }],
    },
    Chip {
        module: "f8",
        sources: &[Source {
            title: "F8 Guide to Programming, 67095664",
            attribution: "Fairchild",
            year: None,
            library: "by-topic/cpu-f8/",
        }],
    },
    Chip {
        module: "s2650",
        sources: &[Source {
            title: "Signetics 2650 Microprocessor Manual",
            attribution: "Signetics",
            year: None,
            library: "by-topic/cpu-2650/",
        }],
    },
    Chip {
        module: "tms7000",
        sources: &[Source {
            title: "TMS7000 Assembly Language Programmer's Guide",
            attribution: "Texas Instruments",
            year: None,
            library: "by-topic/cpu-tms7000/",
        }],
    },
    Chip {
        module: "tms9900",
        sources: &[Source {
            title: "TMS9900 Microprocessor Data Manual",
            attribution: "Texas Instruments",
            year: Some("1976"),
            library: "by-topic/cpu-tms9900/",
        }],
    },
    Chip {
        module: "pdp11",
        sources: &[Source {
            title: "PDP-11 Processor Handbook, EB-19402-20",
            attribution: "Digital Equipment Corporation",
            year: Some("1981"),
            library: "by-topic/cpu-pdp11/",
        }],
    },
    Chip {
        module: "cp1610",
        sources: &[Source {
            title: "CP-1600 Microprocessor User's Manual",
            attribution: "General Instrument",
            year: Some("1975"),
            library: "by-topic/cpu-cp1610/",
        }],
    },
    Chip {
        module: "m68k",
        sources: &[
            Source {
                title: "M68000 Family Programmer's Reference Manual",
                attribution: "Motorola",
                year: Some("1992"),
                library: "by-topic/cpu-68000/",
            },
            Source {
                title: "MC68000 8-/16-/32-Bit Microprocessors User's Manual",
                attribution: "Motorola",
                year: Some("1993"),
                library: "by-topic/cpu-68000/",
            },
        ],
    },
    Chip {
        module: "mos6809",
        sources: &[
            Source {
                title: "MC6809-MC6809E 8-Bit Microprocessor Programming Manual",
                attribution: "Motorola",
                year: Some("1981"),
                library: "by-topic/cpu-6809/",
            },
            Source {
                title: "MC6809E HMOS 8-Bit Microprocessor Data Sheet",
                attribution: "Motorola",
                year: Some("1984"),
                library: "by-topic/cpu-6809/",
            },
        ],
    },
    Chip {
        module: "z8000",
        sources: &[Source {
            title: "Z8000 CPU Technical Manual",
            attribution: "Zilog",
            year: Some("1983"),
            library: "by-topic/cpu-z8000/",
        }],
    },
];

/// The sources behind a module's specification, empty if none are recorded.
#[must_use]
pub fn sources_for(module: &str) -> &'static [Source] {
    PROVENANCE
        .iter()
        .find(|c| c.module == module)
        .map_or(&[], |c| c.sources)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every entry names a document and where it lives. A citation missing
    /// either is not a citation.
    #[test]
    fn every_source_is_complete() {
        for chip in PROVENANCE {
            assert!(!chip.sources.is_empty(), "{} cites nothing", chip.module);
            for s in chip.sources {
                assert!(
                    !s.title.is_empty(),
                    "{}: a source has no title",
                    chip.module
                );
                assert!(
                    !s.attribution.is_empty(),
                    "{}: `{}` has no attribution",
                    chip.module,
                    s.title
                );
                assert!(
                    s.library.starts_with("by-topic/"),
                    "{}: `{}` must say where in the library it sits",
                    chip.module,
                    s.title
                );
            }
        }
    }

    #[test]
    fn a_module_is_named_once() {
        let mut seen = std::collections::BTreeSet::new();
        for chip in PROVENANCE {
            assert!(seen.insert(chip.module), "`{}` listed twice", chip.module);
        }
    }

    #[test]
    fn an_unknown_module_cites_nothing() {
        assert!(sources_for("frobnicate").is_empty());
    }
}
