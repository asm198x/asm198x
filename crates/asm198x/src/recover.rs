//! Verified, reader-guided recovery of flat 6502 binaries as ACME source.
//!
//! Isa198x decodes instructions. The reader supplies code/data boundaries;
//! everything outside a code region remains data. Recovery adds names, uses
//! the shared AST formatter, and reassembles before returning any source.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::ops::Range;

use crate::AsmError;

/// The reader's interpretation of one flat, non-wrapping 6502 image.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct RecoveryOptions {
    /// CPU address of the first input byte; file headers must be removed first.
    pub origin: u16,
    /// Half-open CPU address ranges to decode linearly as instructions.
    pub code: Vec<Range<u32>>,
    /// Half-open CPU address ranges kept as data, overriding any code range.
    pub data: Vec<Range<u32>>,
    /// Reader-supplied names. Addresses outside the image become constants.
    pub labels: BTreeMap<u16, String>,
}

impl RecoveryOptions {
    /// Start with an image whose bytes are all data, with no invented names.
    #[must_use]
    pub fn new(origin: u16) -> Self {
        Self {
            origin,
            code: Vec::new(),
            data: Vec::new(),
            labels: BTreeMap::new(),
        }
    }
}

struct Row {
    address: u32,
    bytes: Vec<u8>,
    instruction: Option<(&'static isa::Instruction, &'static isa::Form)>,
    text: String,
}

/// Recover ACME source that reassembles to exactly `bytes` at `options.origin`.
///
/// Only explicit code ranges are decoded. Direct branch, jump, and call targets
/// inside the image receive labels; indirect jumps do not invent a destination.
/// Unknown/truncated encodings, wrapping branches, and instructions split
/// by a target remain data.
/// Named memory operands are substituted only when their encoded width is safe.
///
/// # Errors
/// An empty or wrapping image, an invalid range or name, duplicate names,
/// output rejected by ACME, or any byte/origin mismatch during verification.
/// No source is returned on failure.
pub fn recover_6502(bytes: &[u8], options: &RecoveryOptions) -> Result<String, AsmError> {
    let start = u32::from(options.origin);
    let end = start
        .checked_add(u32::try_from(bytes.len()).map_err(|_| error("image is too large"))?)
        .filter(|&end| end <= 0x10000)
        .ok_or_else(|| error("the image crosses the 6502 address-space boundary"))?;
    if bytes.is_empty() {
        return Err(error("cannot recover an empty image"));
    }
    let mut code = vec![false; bytes.len()];
    for (ranges, is_code) in [(&options.code, true), (&options.data, false)] {
        for range in ranges {
            if range.start >= range.end || range.start < start || range.end > end {
                return Err(error(format!(
                    "range ${:X}:${:X} must be non-empty and inside ${start:04X}:${end:X}",
                    range.start, range.end
                )));
            }
            code[(range.start - start) as usize..(range.end - start) as usize].fill(is_code);
        }
    }
    let mut names = BTreeSet::new();
    for name in options.labels.values() {
        if !valid_name(name) {
            return Err(error(format!(
                "invalid ACME label `{name}`; use an ASCII letter or underscore, followed by letters, digits or underscores"
            )));
        }
        if !names.insert(name.clone()) {
            return Err(error(format!("label `{name}` names more than one address")));
        }
    }

    let mut rows = Vec::new();
    let mut pos = 0;
    while pos < bytes.len() {
        let is_code = code[pos];
        let limit = (pos + 1..bytes.len())
            .find(|&i| code[i] != is_code)
            .unwrap_or(bytes.len());
        if is_code {
            for line in
                isa_disasm::disassemble_6502(&bytes[pos..limit], (start + pos as u32) as u16)
            {
                let instruction = isa::mos6502::SET.instructions.iter().find_map(|insn| {
                    insn.forms
                        .iter()
                        .find(|form| {
                            form.opcode == &line.bytes[..1] && form.len() == line.bytes.len()
                        })
                        .map(|form| (insn, form))
                });
                rows.push(Row {
                    address: line.addr,
                    bytes: line.bytes,
                    instruction,
                    text: line.text,
                });
            }
        } else {
            for (i, &byte) in bytes[pos..limit].iter().enumerate() {
                rows.push(Row {
                    address: start + (pos + i) as u32,
                    bytes: vec![byte],
                    instruction: None,
                    text: String::new(),
                });
            }
        }
        pos = limit;
    }
    let mut labels = options.labels.clone();
    let mut targets = BTreeSet::new();
    // Name the first instruction in each explicitly marked code run as well
    // as direct control-flow destinations. A label is not proof of execution.
    for (index, row) in rows.iter().enumerate() {
        if row.instruction.is_some() && (index == 0 || rows[index - 1].instruction.is_none()) {
            targets.insert(row.address as u16);
        }
        if let Some(target) =
            control_target(row).filter(|&addr| u32::from(addr) >= start && u32::from(addr) < end)
        {
            targets.insert(target);
        }
    }
    for target in targets {
        if labels.contains_key(&target) {
            continue;
        }
        let base = format!("l_{target:04x}");
        let mut name = base.clone();
        let mut suffix = 2;
        while !names.insert(name.clone()) {
            name = format!("{base}_{suffix}");
            suffix += 1;
        }
        labels.insert(target, name);
    }

    let mut source = String::from("; Recovered 6502 source (ACME). Unmarked bytes are data.\n");
    for (&address, name) in &labels {
        if u32::from(address) < start || u32::from(address) >= end {
            writeln!(source, "{name} = ${address:04X}").expect("string write");
        }
    }
    writeln!(source, "\n* = ${start:04X}").expect("string write");
    let mut pending_data = Vec::new();
    for row in &rows {
        let split = labels
            .range((
                std::ops::Bound::Excluded(row.address as u16),
                std::ops::Bound::Unbounded,
            ))
            .next()
            .is_some_and(|(&addr, _)| u32::from(addr) < row.address + row.bytes.len() as u32);
        let wraps = control_target(row).is_some_and(|target| {
            row.instruction
                .is_some_and(|(_, form)| form.mode == "relative")
                && i64::from(target)
                    != i64::from(row.address)
                        + row.bytes.len() as i64
                        + i64::from(row.bytes[1] as i8)
        });
        if row.instruction.is_some() && !split && !wraps {
            flush_data(&mut source, &mut pending_data);
            emit_label(&mut source, &labels, row.address as u16);
            writeln!(source, "    {}", render_instruction(row, &labels)).expect("string write");
        } else {
            if wraps {
                flush_data(&mut source, &mut pending_data);
                writeln!(
                    source,
                    "; {} wraps the address space; retain its bytes.",
                    row.text
                )
                .expect("string write");
            }
            if split && row.instruction.is_some() {
                flush_data(&mut source, &mut pending_data);
                writeln!(
                    source,
                    "; A target splits the instruction at ${:04X}; retain its bytes.",
                    row.address
                )
                .expect("string write");
            }
            for (offset, &byte) in row.bytes.iter().enumerate() {
                let address = (row.address + offset as u32) as u16;
                if labels.contains_key(&address) {
                    flush_data(&mut source, &mut pending_data);
                    emit_label(&mut source, &labels, address);
                }
                pending_data.push(byte);
                if pending_data.len() == 8 {
                    flush_data(&mut source, &mut pending_data);
                }
            }
        }
    }
    flush_data(&mut source, &mut pending_data);
    let output = crate::format_acme(&source)?;
    let rebuilt = crate::assemble_acme(&output)
        .map_err(|e| error(format!("recovered source does not assemble: {e}")))?;
    if rebuilt.bytes != bytes || rebuilt.origin != Some(options.origin) {
        let at = bytes
            .iter()
            .zip(&rebuilt.bytes)
            .position(|(a, b)| a != b)
            .unwrap_or(bytes.len().min(rebuilt.bytes.len()));
        return Err(error(format!(
            "recovery does not verify at byte {at}; no source returned"
        )));
    }
    Ok(output)
}

fn error(message: impl Into<String>) -> AsmError {
    AsmError::new(0, message)
}

fn valid_name(name: &str) -> bool {
    let mut chars = name.bytes();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == b'_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == b'_')
}

fn control_target(row: &Row) -> Option<u16> {
    let (insn, form) = row.instruction?;
    if form.mode == "relative" {
        Some(
            (row.address as u16)
                .wrapping_add(row.bytes.len() as u16)
                .wrapping_add_signed(i16::from(row.bytes[1] as i8)),
        )
    } else if form.mode == "absolute" && matches!(insn.mnemonic, "JSR" | "JMP") {
        Some(u16::from_le_bytes([row.bytes[1], row.bytes[2]]))
    } else {
        None
    }
}

fn render_instruction(row: &Row, labels: &BTreeMap<u16, String>) -> String {
    let Some((insn, form)) = row.instruction else {
        return row.text.clone();
    };
    let address = control_target(row).or_else(|| match form.mode {
        "absolute" | "absolute,x" | "absolute,y" | "indirect" => {
            Some(u16::from_le_bytes([row.bytes[1], row.bytes[2]]))
        }
        _ => None,
    });
    if let Some(address) = address
        && let Some(name) = labels.get(&address)
        // A low absolute address must keep its written width, and a zero-page
        // forward label would select a different form. Leave those numeric.
        && (address >= 0x100 || form.mode == "relative" || matches!(insn.mnemonic,"JSR" | "JMP"))
    {
        row.text.replace(&format!("${address:04X}"), name)
    } else {
        row.text.clone()
    }
}

fn emit_label(source: &mut String, labels: &BTreeMap<u16, String>, address: u16) {
    if let Some(name) = labels.get(&address) {
        writeln!(source, "{name}:").expect("string write");
    }
}

fn flush_data(source: &mut String, data: &mut Vec<u8>) {
    if data.is_empty() {
        return;
    }
    source.push_str("    !byte ");
    for (index, byte) in data.iter().enumerate() {
        if index != 0 {
            source.push_str(", ");
        }
        write!(source, "${byte:02X}").expect("string write");
    }
    source.push('\n');
    data.clear();
}
