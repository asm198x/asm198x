//! CLI boundary for the opt-in, verified 6502 recovery workflow.
use std::io::Write as _;
use std::ops::Range;
use std::path::PathBuf;

pub(super) fn run(args: &[String]) -> Result<String, String> {
    let mut options = asm198x::RecoveryOptions::new(0);
    let mut recover = false;
    let mut input = None;
    let mut output: Option<PathBuf> = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--recover" => recover = true,
            "--org" | "--code" | "--data" | "--label" | "--dialect" | "-d" | "--cpu"
            | "--target" | "-o" | "--output" => {
                let flag = args[i].as_str();
                i += 1;
                let value = args
                    .get(i)
                    .ok_or_else(|| format!("`{flag}` needs a value"))?;
                match flag {
                    "--org" => options.origin = super::parse_u16(value)?,
                    "--code" => options.code.push(range(value)?),
                    "--data" => options.data.push(range(value)?),
                    "--label" => {
                        let (address, name) = value
                            .split_once(':')
                            .ok_or("`--label` needs ADDRESS:NAME")?;
                        let address = super::parse_u16(address)?;
                        if options.labels.insert(address, name.to_string()).is_some() {
                            return Err(format!("more than one `--label` for ${address:04X}"));
                        }
                    }
                    "-o" | "--output" => output = Some(PathBuf::from(value)),
                    _ => {
                        if asm198x::dialect_table::canonical(&value.to_ascii_lowercase())
                            != Some("acme")
                        {
                            return Err("`disasm --recover` currently emits ACME source for the base 6502; use `--dialect acme`".into());
                        }
                    }
                }
            }
            flag if flag.starts_with('-') => {
                return Err(format!("unsupported recovery option `{flag}`"));
            }
            path if input.is_none() => input = Some(path),
            _ => return Err("only one input file is supported".into()),
        }
        i += 1;
    }
    if !recover {
        return Err("`--code`, `--data`, and `--label` require `disasm --recover`".into());
    }
    let input = input.ok_or("no binary input given")?;
    let bytes = std::fs::read(input).map_err(|e| format!("cannot read {input}: {e}"))?;
    let source = asm198x::recover_6502(&bytes, &options).map_err(|e| e.to_string())?;
    let destination = if let Some(path) = output {
        // Recovery is followed by hand editing. Exclusive creation protects
        // those edits, the binary input, and any symlink or hard-link alias.
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|e| {
                format!(
                    "cannot create {} (recovery output must be a new file): {e}",
                    path.display()
                )
            })?;
        file.write_all(source.as_bytes())
            .map_err(|e| format!("cannot write {}: {e}", path.display()))?;
        path.display().to_string()
    } else {
        print!("{source}");
        "stdout".into()
    };
    Ok(format!(
        "recovered {} byte(s) at ${:04X} as ACME source (verified byte-identical) -> {destination}",
        bytes.len(),
        options.origin
    ))
}

fn range(value: &str) -> Result<Range<u32>, String> {
    let (start, end) = value
        .split_once(':')
        .ok_or("a recovery range needs START:END (end exclusive)")?;
    let address = |text: &str| {
        let parsed = if let Some(hex) = text.strip_prefix('$').or_else(|| text.strip_prefix("0x")) {
            u32::from_str_radix(hex, 16)
        } else {
            text.parse::<u32>()
        };
        parsed
            .ok()
            .filter(|&n| n <= 0x10000)
            .ok_or_else(|| format!("invalid recovery address `{text}`"))
    };
    Ok(address(start)?..address(end)?)
}
