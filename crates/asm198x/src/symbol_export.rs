//! Emulator symbol exports. Layouts are specified by RGBDS's `.sym`
//! specification (<https://rgbds.gbdev.io/sym>) and VICE's monitor label
//! commands (<https://vice-emu.sourceforge.io/vice_12.html>).
//! These are address-label exports: constants remain available in `--sym`
//! and Debug198x, but are not fabricated into emulator memory locations.

use std::collections::BTreeMap;
use std::fmt::{self, Write};

use crate::AssemblyResult;
use debug198x::{DebugInfo, SymbolKind};

/// Rendering selected for the CLI's symbol artifact.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum SymbolFormat {
    /// Asm198x's existing sorted `name = value` rendering.
    #[default]
    Native,
    /// VICE monitor `al C:address .label` commands.
    Vice,
    /// Game Boy bank:address symbols, compatible with NO$-style consumers.
    NoCash,
}

/// A symbol cannot be represented faithfully in the selected format.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct SymbolExportError {
    /// Original source spelling of the symbol that could not be exported.
    pub symbol: String,
    /// The representation constraint that prevented export.
    pub reason: &'static str,
}

impl fmt::Display for SymbolExportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "cannot export symbol `{}`: {}", self.symbol, self.reason)
    }
}

impl std::error::Error for SymbolExportError {}

/// Render symbols without touching the filesystem. `info` is the same record
/// used for Debug198x (including linked ca65/vasm records); RGBASM banked
/// addresses come from the assembly's captured bank annotations and symbols.
///
/// # Errors
/// Refuses unknown banks, relocatable or oversized addresses, unsupported
/// names, and VICE names that collide after its required dot prefix is added.
/// No partial output is returned on failure.
pub fn render_symbol_export(
    result: &AssemblyResult,
    info: &DebugInfo,
    format: SymbolFormat,
) -> Result<String, SymbolExportError> {
    if format == SymbolFormat::Native {
        return Ok(crate::render_sym(info));
    }
    let mut rows = BTreeMap::new();
    for symbol in &info.symbols {
        let (section, offset, space) = match &symbol.kind {
            SymbolKind::Label {
                section,
                offset,
                space,
            }
            | SymbolKind::Entry {
                section,
                offset,
                space,
            } => (*section, *offset, space),
            SymbolKind::Const { .. } => continue,
        };
        let error = |reason| SymbolExportError {
            symbol: symbol.name.clone(),
            reason,
        };
        let (name, address, bank) = match format {
            SymbolFormat::NoCash => {
                let bank = result
                    .debug
                    .symbol_banks
                    .get(&symbol.name)
                    .copied()
                    .ok_or_else(|| error("no captured Game Boy bank"))?;
                let address = result
                    .symbols
                    .get(&symbol.name)
                    .and_then(|v| u16::try_from(*v).ok())
                    .ok_or_else(|| error("no resolved 16-bit CPU address"))?;
                if !valid_name(&symbol.name, false) {
                    return Err(error(
                        "name is outside the supported NO$-style ASCII alphabet",
                    ));
                }
                (symbol.name.clone(), address, bank)
            }
            SymbolFormat::Vice => {
                let section = info.sections.iter().find(|s| s.id == section);
                if space.is_some()
                    || section.is_some_and(|s| s.space.is_some())
                    || result.debug.symbol_pages.contains_key(&symbol.name)
                    || result.debug.symbol_banks.contains_key(&symbol.name)
                {
                    return Err(error("VICE C: labels cannot preserve this banked location"));
                }
                let address = section
                    .and_then(|s| s.base)
                    .and_then(|base| base.checked_add(offset))
                    .and_then(|a| u16::try_from(a).ok())
                    .ok_or_else(|| error("no resolved 16-bit CPU address"))?;
                let name = symbol.name.strip_prefix('.').unwrap_or(&symbol.name);
                // VICE's C64 monitor resolves these as registers, not labels.
                // Its importer refuses them even when their spelling is legal.
                if ["PC", "A", "X", "Y", "SP", "FL", "LIN", "CYC"]
                    .iter()
                    .any(|reserved| name.eq_ignore_ascii_case(reserved))
                {
                    return Err(error("name is reserved by the VICE C64 monitor"));
                }
                if !valid_name(name, true) {
                    return Err(error("name is outside the supported VICE alphabet"));
                }
                (format!(".{name}"), address, 0)
            }
            SymbolFormat::Native => unreachable!("handled above"),
        };
        if rows.insert(name, (address, bank)).is_some() {
            return Err(error("names collide in the selected format"));
        }
    }
    let mut output = String::new();
    for (name, (address, bank)) in rows {
        match format {
            SymbolFormat::Vice => writeln!(output, "al C:{address:04x} {name}"),
            SymbolFormat::NoCash => writeln!(output, "{bank:02x}:{address:04x} {name}"),
            SymbolFormat::Native => unreachable!("handled above"),
        }
        .expect("writing to a String cannot fail");
    }
    Ok(output)
}

fn valid_name(name: &str, vice: bool) -> bool {
    let mut chars = name.bytes();
    let initial = |c: u8| c.is_ascii_alphabetic() || c == b'_' || (vice && b"@?:".contains(&c));
    chars.next().is_some_and(initial)
        && chars.all(|c| {
            initial(c) || c.is_ascii_digit() || c == b'.' || (!vice && b"@#$".contains(&c))
        })
}
