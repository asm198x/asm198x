//! Which machines used each CPU.
//!
//! The instruction reference exists to help someone write code for a machine,
//! and a CPU on its own is an abstraction. This is the join back: from an
//! instruction set to the hardware people actually had.
//!
//! # Where this comes from, and why it is a copy
//!
//! The umbrella reference library records, per chip, the machines it served.
//! That library is a **private repository**, so this workspace cannot read it
//! at build time and the mapping is copied here instead. `cargo xtask machines
//! --check` re-reads the library when it is available and reports any
//! disagreement, which is the only way a copy like this stays honest.
//!
//! # `catalogued` is about links, not importance
//!
//! A machine is `catalogued` when the Code198x catalogue has a page for it, at
//! `https://code198x.com/<slug>/`. Five here do not: the COSMAC VIP, the MK14,
//! the VC 4000, the Arcadia 2001 and the PDP-11. They are named anyway, without
//! a link — a machine missing from a catalogue has not stopped existing, and
//! linking to a 404 would be worse than naming it plainly. Every slug marked
//! `catalogued` was verified to resolve on 2026-08-20.

/// A machine that used a given CPU.
pub struct Machine {
    /// Its slug in the Code198x catalogue, which is also its URL there.
    pub slug: &'static str,
    /// Its name, for display — used whether or not a link is available.
    pub name: &'static str,
    /// Whether the catalogue has a page to link to.
    pub catalogued: bool,
}

/// One CPU's machines, keyed by its module name in this crate.
pub struct Cpu {
    pub module: &'static str,
    pub machines: &'static [Machine],
}

/// Every CPU whose machines the reference library records.
///
/// A CPU absent here is one the library lists no machines for — the 6800,
/// TMS7000 and Z8000 today. That is a gap in the library rather than a claim
/// that nothing used them.
pub const MACHINES: &[Cpu] = &[
    Cpu {
        module: "mos6502",
        machines: &[
            Machine {
                slug: "nintendo-entertainment-system",
                name: "Nintendo Entertainment System",
                catalogued: true,
            },
            Machine {
                slug: "apple-ii",
                name: "Apple II",
                catalogued: true,
            },
            Machine {
                slug: "atari-800",
                name: "Atari 8-bit",
                catalogued: true,
            },
            Machine {
                slug: "bbc-micro",
                name: "BBC Micro",
                catalogued: true,
            },
            Machine {
                slug: "commodore-vic-20",
                name: "Commodore VIC-20",
                catalogued: true,
            },
            Machine {
                slug: "commodore-pet",
                name: "Commodore PET",
                catalogued: true,
            },
            Machine {
                slug: "kim-1",
                name: "MOS KIM-1",
                catalogued: true,
            },
            Machine {
                slug: "oric-atmos",
                name: "Oric Atmos",
                catalogued: true,
            },
        ],
    },
    Cpu {
        module: "mos65816",
        machines: &[
            Machine {
                slug: "apple-ii",
                name: "Apple II",
                catalogued: true,
            },
            Machine {
                slug: "super-nintendo",
                name: "Nintendo Super Nintendo",
                catalogued: true,
            },
        ],
    },
    Cpu {
        module: "huc6280",
        machines: &[Machine {
            slug: "pc-engine",
            name: "NEC PC Engine",
            catalogued: true,
        }],
    },
    Cpu {
        module: "z80",
        machines: &[
            Machine {
                slug: "sinclair-zx-spectrum",
                name: "Sinclair ZX Spectrum",
                catalogued: true,
            },
            Machine {
                slug: "amstrad-cpc",
                name: "Amstrad CPC",
                catalogued: true,
            },
            Machine {
                slug: "msx",
                name: "MSX",
                catalogued: true,
            },
            Machine {
                slug: "nec-pc-88",
                name: "NEC PC-8801",
                catalogued: true,
            },
            Machine {
                slug: "sega-master-system",
                name: "Sega Master System",
                catalogued: true,
            },
        ],
    },
    Cpu {
        module: "sm83",
        machines: &[Machine {
            slug: "game-boy",
            name: "Nintendo Game Boy",
            catalogued: true,
        }],
    },
    Cpu {
        module: "i8080",
        machines: &[
            Machine {
                slug: "altair-8800",
                name: "MITS Altair 8800",
                catalogued: true,
            },
            Machine {
                slug: "imsai-8080",
                name: "IMSAI 8080",
                catalogued: true,
            },
        ],
    },
    Cpu {
        module: "cdp1802",
        machines: &[Machine {
            slug: "rca-cosmac-vip",
            name: "RCA COSMAC VIP",
            catalogued: false,
        }],
    },
    Cpu {
        module: "i8048",
        machines: &[
            Machine {
                slug: "magnavox-odyssey2",
                name: "Magnavox Odyssey²",
                catalogued: true,
            },
            Machine {
                slug: "commodore-amiga",
                name: "Commodore Amiga",
                catalogued: true,
            },
        ],
    },
    Cpu {
        module: "scmp",
        machines: &[Machine {
            slug: "science-of-cambridge-mk14",
            name: "Science of Cambridge MK14",
            catalogued: false,
        }],
    },
    Cpu {
        module: "f8",
        machines: &[Machine {
            slug: "fairchild-channel-f",
            name: "Fairchild Channel F",
            catalogued: true,
        }],
    },
    Cpu {
        module: "s2650",
        machines: &[
            Machine {
                slug: "interton-vc-4000",
                name: "Interton VC 4000",
                catalogued: false,
            },
            Machine {
                slug: "emerson-arcadia-2001",
                name: "Emerson Arcadia 2001",
                catalogued: false,
            },
        ],
    },
    Cpu {
        module: "tms9900",
        machines: &[Machine {
            slug: "ti-99-4a",
            name: "Texas Instruments TI-99/4A",
            catalogued: true,
        }],
    },
    Cpu {
        module: "pdp11",
        machines: &[Machine {
            slug: "dec-pdp-11",
            name: "DEC PDP-11",
            catalogued: false,
        }],
    },
    Cpu {
        module: "cp1610",
        machines: &[Machine {
            slug: "intellivision",
            name: "Mattel Intellivision",
            catalogued: true,
        }],
    },
    Cpu {
        module: "m68k",
        machines: &[Machine {
            slug: "commodore-amiga",
            name: "Commodore Amiga",
            catalogued: true,
        }],
    },
    Cpu {
        module: "mos6809",
        machines: &[
            Machine {
                slug: "dragon-32",
                name: "Dragon 32/64",
                catalogued: true,
            },
            Machine {
                slug: "trs-80",
                name: "Tandy TRS-80",
                catalogued: true,
            },
        ],
    },
];

/// The machines recorded for a module, empty if none are.
#[must_use]
pub fn machines_for(module: &str) -> &'static [Machine] {
    MACHINES
        .iter()
        .find(|c| c.module == module)
        .map_or(&[], |c| c.machines)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_machine_is_named_and_slugged() {
        for cpu in MACHINES {
            assert!(!cpu.machines.is_empty(), "{} lists no machines", cpu.module);
            for m in cpu.machines {
                assert!(!m.name.is_empty(), "{}: a machine has no name", cpu.module);
                assert!(
                    !m.slug.is_empty()
                        && m.slug
                            .chars()
                            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'),
                    "{}: `{}` is not a slug",
                    cpu.module,
                    m.slug
                );
            }
        }
    }

    #[test]
    fn a_module_is_named_once() {
        let mut seen = std::collections::BTreeSet::new();
        for cpu in MACHINES {
            assert!(seen.insert(cpu.module), "`{}` listed twice", cpu.module);
        }
    }

    /// The uncatalogued five are deliberate, not oversights. If one gains a
    /// catalogue page the flag should flip — and this test is where someone
    /// will notice the list is worth rechecking.
    #[test]
    fn the_uncatalogued_machines_are_the_ones_we_expect() {
        let mut absent: Vec<&str> = MACHINES
            .iter()
            .flat_map(|c| c.machines)
            .filter(|m| !m.catalogued)
            .map(|m| m.slug)
            .collect();
        absent.sort_unstable();
        absent.dedup();
        assert_eq!(
            absent,
            [
                "dec-pdp-11",
                "emerson-arcadia-2001",
                "interton-vc-4000",
                "rca-cosmac-vip",
                "science-of-cambridge-mk14"
            ]
        );
    }
}
