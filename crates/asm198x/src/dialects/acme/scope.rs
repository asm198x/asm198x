//! The anonymous-label scope (#486's scope split for acme): the `-`/`+`
//! definitions registered in evaluation order during the walk, the
//! self-describing placeholder a reference parses to, and the post-walk
//! rewrite of every placeholder to its definition's name. Moved verbatim from
//! the parent; the seam is the boundary, not a rewrite.

use super::{AsmError, Expr, FileId, Operation};

// ---------------------------------------------------------------------------
// Anonymous labels (`-`/`--`/`+`/`++` …)
// ---------------------------------------------------------------------------

/// One anonymous-label definition: its **evaluation-order position** (the
/// "virtual line" — one per live lowered line, so included files splice in
/// order and untaken branches never register), its sign and level (the run
/// length, so `--` is level 2), and the unique synthetic name it binds. The
/// name carries a leading control char so it can never collide with a real
/// identifier.
pub(super) struct AnonDef {
    vline: usize,
    sign: char,
    level: usize,
    pub(super) name: String,
}

/// The anonymous-label state of one evaluation walk (language-surface U4).
///
/// Definitions register as the walk reaches them live, in spliced order.
/// References cannot resolve during the walk — a forward `+` may point into a
/// file not yet loaded (an include reached later) — so [`parse_value`](super::parse_value) mints
/// a self-describing **placeholder symbol** ([`anon_ref_placeholder`])
/// encoding the sign, level, and referencing position; after the walk,
/// [`AcmeEval::resolve_anon_refs`](super::AcmeEval::resolve_anon_refs) rewrites every placeholder to its
/// definition's name ([`substitute_anon_refs`]).
#[derive(Default)]
pub(super) struct Anons {
    defs: Vec<AnonDef>,
    /// The current evaluation position; bumped once per live lowered line.
    pub(super) vline: usize,
}

impl Anons {
    /// Register a definition at the current evaluation position.
    pub(super) fn define(&mut self, sign: char, level: usize) {
        let name = format!("\u{1}{sign}{level}#{}", self.defs.len());
        self.defs.push(AnonDef {
            vline: self.vline,
            sign,
            level,
            name,
        });
    }

    /// The definition registered at the current evaluation position, if any —
    /// how the label side of a line finds its own synthetic name.
    pub(super) fn def_here(&self) -> Option<&AnonDef> {
        self.defs.last().filter(|d| d.vline == self.vline)
    }

    /// Resolve a reference at position `vline`: the nearest preceding `-`
    /// definition (backward — the same line is allowed: `- jmp -` self-loops)
    /// or the nearest *strictly following* `+` definition (forward — acme does
    /// **not** let `+ jmp +` see its own line; probe-pinned), at the same
    /// level.
    fn resolve(&self, sign: char, level: usize, vline: usize) -> Option<&AnonDef> {
        let matching = self
            .defs
            .iter()
            .filter(|d| d.sign == sign && d.level == level);
        if sign == '-' {
            matching
                .filter(|d| d.vline <= vline)
                .max_by_key(|d| d.vline)
        } else {
            matching.filter(|d| d.vline > vline).min_by_key(|d| d.vline)
        }
    }
}

/// A column-0 token made entirely of `-` or entirely of `+` is an anonymous
/// label. Returns its sign and level (run length).
pub(super) fn anon_marker(word: &str) -> Option<(char, usize)> {
    let mut chars = word.chars();
    let first = chars.next()?;
    if (first == '-' || first == '+') && word.chars().all(|c| c == first) {
        Some((first, word.len()))
    } else {
        None
    }
}

/// The self-describing placeholder a reference parses to during the walk:
/// `\u{2}{sign}{level}@{vline}`. The `\u{2}` prefix can never collide with a
/// real identifier (or with the `\u{1}` definition names), and the payload
/// carries everything post-walk resolution needs — no side table.
pub(super) fn anon_ref_placeholder(sign: char, level: usize, vline: usize) -> String {
    format!("\u{2}{sign}{level}@{vline}")
}

/// Decode an [`anon_ref_placeholder`]'s `(sign, level, vline)`, or `None` for
/// an ordinary symbol.
fn decode_anon_ref(name: &str) -> Option<(char, usize, usize)> {
    let body = name.strip_prefix('\u{2}')?;
    let mut chars = body.chars();
    let sign = chars.next()?;
    let rest = chars.as_str();
    let (level, vline) = rest.split_once('@')?;
    Some((sign, level.parse().ok()?, vline.parse().ok()?))
}

/// Rewrite every anonymous-reference placeholder in `op` to its resolved
/// definition name — the post-walk half of the spliced-order model. An
/// unresolvable reference errors at the statement that made it.
pub(super) fn substitute_anon_refs(
    op: Operation,
    anons: &Anons,
    file: FileId,
    line: usize,
) -> Result<Operation, AsmError> {
    let subst = |e: Expr| subst_anon_expr(e, anons, file, line);
    Ok(match op {
        Operation::Org(e) => Operation::Org(subst(e)?),
        Operation::Equ(e) => Operation::Equ(subst(e)?),
        Operation::Set(e) => Operation::Set(subst(e)?),
        Operation::SaveRaw {
            name,
            start,
            length,
        } => Operation::SaveRaw {
            name,
            start: subst(start)?,
            length: length.map(subst).transpose()?,
        },
        Operation::SaveTape {
            file,
            kind,
            name,
            start,
            length,
        } => Operation::SaveTape {
            file,
            kind,
            name,
            start: subst(start)?,
            length: subst(length)?,
        },
        Operation::Device(spec) => Operation::Device(spec),
        Operation::DeviceSlot(slot) => Operation::DeviceSlot(subst(slot)?),
        Operation::DevicePage(page) => Operation::DevicePage(subst(page)?),
        Operation::SaveCpr { name, pages } => Operation::SaveCpr {
            name,
            pages: subst(pages)?,
        },
        Operation::Entry(e) => Operation::Entry(subst(e)?),
        Operation::Bytes(v) => {
            Operation::Bytes(v.into_iter().map(subst).collect::<Result<_, _>>()?)
        }
        Operation::Words(v) => {
            Operation::Words(v.into_iter().map(subst).collect::<Result<_, _>>()?)
        }
        Operation::Os9Module { fields } => Operation::Os9Module {
            fields: fields.into_iter().map(subst).collect::<Result<_, _>>()?,
        },
        Operation::Os9EndModule => Operation::Os9EndModule,
        Operation::Sized {
            width,
            big_endian,
            values,
        } => Operation::Sized {
            width,
            big_endian,
            values: values.into_iter().map(subst).collect::<Result<_, _>>()?,
        },
        Operation::InitMem(v) => Operation::InitMem(v),
        Operation::PseudoPc(e) => Operation::PseudoPc(e.map(subst).transpose()?),
        Operation::RequestOutput {
            path,
            format,
            defaulted_format,
        } => Operation::RequestOutput {
            path,
            format,
            defaulted_format,
        },
        Operation::RequestSymbols { path } => Operation::RequestSymbols { path },
        Operation::Instruction {
            mnemonic,
            mode,
            operands,
        } => Operation::Instruction {
            mnemonic,
            mode,
            operands: operands.into_iter().map(subst).collect::<Result<_, _>>()?,
        },
        Operation::DirectPage {
            direct,
            extended,
            expr,
            dp,
        } => Operation::DirectPage {
            direct,
            extended,
            expr: subst(expr)?,
            dp,
        },
        // No expressions to rewrite: pre-encoded pieces, binary payloads, and
        // the constant-argument align.
        other @ (Operation::Encoded(_)
        | Operation::Binary(_)
        | Operation::DefineSymbols(_)
        | Operation::Align { .. }
        | Operation::AlignTo { .. }
        | Operation::Diagnose { .. }
        | Operation::Section { .. }
        | Operation::Reserve(_)) => other,
        Operation::Fill { count, value } => Operation::Fill {
            count: subst(count)?,
            value,
        },
        Operation::Assert {
            cond,
            fatal,
            message,
        } => Operation::Assert {
            cond: subst(cond)?,
            fatal,
            message,
        },
    })
}

fn subst_anon_expr(e: Expr, anons: &Anons, file: FileId, line: usize) -> Result<Expr, AsmError> {
    Ok(match e {
        Expr::Sym(s) => match decode_anon_ref(&s) {
            Some((sign, level, vline)) => {
                let def = anons.resolve(sign, level, vline).ok_or_else(|| {
                    let run: String = std::iter::repeat_n(sign, level).collect();
                    AsmError::at(
                        crate::ast::Span::in_file(file, line as u32, 0),
                        format!("no anonymous label `{run}` in that direction"),
                    )
                })?;
                Expr::Sym(def.name.clone())
            }
            None => Expr::Sym(s),
        },
        Expr::Lo(b) => Expr::Lo(Box::new(subst_anon_expr(*b, anons, file, line)?)),
        Expr::Hi(b) => Expr::Hi(Box::new(subst_anon_expr(*b, anons, file, line)?)),
        Expr::Bank(b) => Expr::Bank(Box::new(subst_anon_expr(*b, anons, file, line)?)),
        Expr::Neg(b) => Expr::Neg(Box::new(subst_anon_expr(*b, anons, file, line)?)),
        Expr::BitNot(b) => Expr::BitNot(Box::new(subst_anon_expr(*b, anons, file, line)?)),
        Expr::FixRound(b) => Expr::FixRound(Box::new(subst_anon_expr(*b, anons, file, line)?)),
        Expr::TrailingZeros(b) => {
            Expr::TrailingZeros(Box::new(subst_anon_expr(*b, anons, file, line)?))
        }
        Expr::LogNot(b) => Expr::LogNot(Box::new(subst_anon_expr(*b, anons, file, line)?)),
        Expr::Bin(op, l, r) => Expr::Bin(
            op,
            Box::new(subst_anon_expr(*l, anons, file, line)?),
            Box::new(subst_anon_expr(*r, anons, file, line)?),
        ),
        other @ (Expr::Num(_) | Expr::Pc) => other,
    })
}
