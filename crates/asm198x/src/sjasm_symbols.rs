//! SjASMPlus 1.21.0 map layouts, independently exercised against the reference
//! and their consumers. See `docs/sjasmplus-symbol-exports.md` for probes.
//! Layout code accepts plain symbol data and never reads/writes files.
use std::collections::BTreeMap;
use std::fmt::Write;

#[derive(Clone, Copy)]
pub(crate) enum Kind {
    Label,
    Constant,
    Variable,
    Structure,
}

pub(crate) struct Symbol {
    pub value: i64,
    pub page: Option<u16>,
    pub kind: Kind,
}

pub(crate) fn cspect(symbols: &BTreeMap<String, Symbol>, page_size: u32) -> String {
    let mut text = String::new();
    for (name, symbol) in symbols {
        let kind = match symbol.kind {
            Kind::Constant => 1,
            Kind::Variable => 2,
            Kind::Structure => 4,
            Kind::Label if symbol.page.is_none() => 3,
            Kind::Label => 0,
        };
        let page = if kind == 0 {
            u32::from(symbol.page.unwrap_or_default())
        } else {
            0
        };
        let logical = symbol.value as u32 & 0xffff;
        let physical = (symbol.value as u32 & (page_size - 1)) + page * page_size;
        let mut name = name.clone();
        if let Some((dot, _)) = name
            .rmatch_indices('.')
            .find(|(dot, _)| symbols.contains_key(&name[..*dot]))
        {
            name.replace_range(dot..dot + 1, "@");
        }
        writeln!(
            text,
            "{logical:08X} {physical:08X} {kind:02X} {}",
            name.to_ascii_uppercase()
        )
        .expect("String write");
    }
    text
}

pub(crate) fn labels(
    symbols: &BTreeMap<String, Symbol>,
    device: Option<(&str, u32)>,
    virtual_labels: bool,
) -> String {
    let mut text = String::new();
    let mask = if virtual_labels {
        0xffff
    } else {
        device.map_or(0x3fff, |(_, size)| size - 1)
    };
    for (name, symbol) in symbols {
        let mut page = if virtual_labels { None } else { symbol.page };
        if device.is_some_and(|(name, _)| name == "ZXSPECTRUM48") {
            page = match page {
                Some(0) => None,
                Some(1) => Some(5),
                Some(3) => Some(0),
                other => other,
            };
        }
        if let Some(page) = page {
            // Reference uses decimal page numbers, including its low-byte wrap.
            write!(text, "{:02}", page & 255).expect("String write");
        }
        writeln!(text, ":{:04X} {name}", symbol.value as u32 & mask).expect("String write");
    }
    text
}
