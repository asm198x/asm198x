//! Private immutable definitions retained by the linked ca65 projection.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use crate::span::Span;

use super::{AsmError, Expr, Kind, collect_syms, map_expr_syms, split_scope_ref};

pub(super) struct Definition {
    pub value: Expr,
    source_value: Expr,
    pub span: Span,
    pub position: usize,
}

#[derive(Default)]
pub(super) struct Deferred {
    pub definitions: BTreeMap<String, Definition>,
    waiting: BTreeMap<String, BTreeSet<String>>,
    remaining: BTreeMap<String, BTreeSet<String>>,
    size_cache: std::cell::RefCell<BTreeMap<String, SizeHint>>,
}

fn symbols(value: &Expr) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    collect_syms(&Kind::Bytes(vec![value.clone()]), &mut names);
    names
}

#[derive(Clone, Copy, Default)]
struct SizeHint {
    value: Option<i64>,
    byte: Option<bool>,
}

impl SizeHint {
    fn known(value: i64) -> Self {
        Self {
            value: Some(value),
            byte: Some((0..=255).contains(&value)),
        }
    }

    fn sized(byte: bool) -> Self {
        Self {
            value: None,
            byte: Some(byte),
        }
    }
}

// Width analysis never evaluates a symbolic value. When children are numeric,
// the shared evaluator supplies the result, including its overflow rules.
fn expression_hint(value: &Expr, names: &BTreeMap<String, SizeHint>, line: usize) -> SizeHint {
    use crate::engine::BinOp;
    match value {
        Expr::Num(value) => SizeHint::known(*value),
        Expr::Sym(name) => names.get(name).copied().unwrap_or_default(),
        Expr::Pc => SizeHint::default(),
        Expr::Bin(op, left, right) => {
            let left = expression_hint(left, names, line);
            let right = expression_hint(right, names, line);
            if let (Some(left), Some(right)) = (left.value, right.value) {
                let numeric = Expr::Bin(*op, Box::new(Expr::Num(left)), Box::new(Expr::Num(right)));
                return numeric
                    .eval_with(&|_| None, None, line)
                    .map(SizeHint::known)
                    .unwrap_or_default();
            }
            let byte = if matches!(
                op,
                BinOp::LogAnd
                    | BinOp::LogOr
                    | BinOp::LogXor
                    | BinOp::Eq
                    | BinOp::Ne
                    | BinOp::Lt
                    | BinOp::Gt
                    | BinOp::Le
                    | BinOp::Ge
            ) {
                Some(true)
            } else {
                left.byte.zip(right.byte).map(|(left, right)| left && right)
            };
            SizeHint { value: None, byte }
        }
        Expr::Lo(child)
        | Expr::Hi(child)
        | Expr::Bank(child)
        | Expr::Neg(child)
        | Expr::BitNot(child)
        | Expr::LogNot(child)
        | Expr::FixRound(child)
        | Expr::TrailingZeros(child) => {
            let child = expression_hint(child, names, line);
            if let Some(number) = child.value {
                let number = Box::new(Expr::Num(number));
                let numeric = match value {
                    Expr::Lo(_) => Expr::Lo(number),
                    Expr::Hi(_) => Expr::Hi(number),
                    Expr::Bank(_) => Expr::Bank(number),
                    Expr::Neg(_) => Expr::Neg(number),
                    Expr::BitNot(_) => Expr::BitNot(number),
                    Expr::LogNot(_) => Expr::LogNot(number),
                    Expr::FixRound(_) => Expr::FixRound(number),
                    Expr::TrailingZeros(_) => Expr::TrailingZeros(number),
                    _ => unreachable!("unary expression"),
                };
                return numeric
                    .eval_with(&|_| None, None, line)
                    .map(SizeHint::known)
                    .unwrap_or_default();
            }
            if matches!(
                value,
                Expr::Lo(_) | Expr::Hi(_) | Expr::Bank(_) | Expr::LogNot(_)
            ) {
                SizeHint::sized(true)
            } else {
                SizeHint {
                    value: None,
                    byte: child.byte,
                }
            }
        }
    }
}

impl Deferred {
    pub fn insert(
        &mut self,
        name: String,
        value: Expr,
        span: Span,
        position: usize,
        values: &mut BTreeMap<String, i64>,
    ) -> Vec<(String, i64)> {
        self.invalidate_size(&name);
        // A source-time fold can use the current scope only. Outward fallback
        // belongs to final binding; retain the original expression for it.
        let source_value = map_expr_syms(&value, &|key| {
            let (path, name) = split_scope_ref(key)?;
            Some(Expr::Sym(if path.is_empty() {
                name.to_string()
            } else {
                format!("{path}::{name}")
            }))
        });
        let missing: BTreeSet<String> = symbols(&source_value)
            .into_iter()
            .filter(|key| !values.contains_key(key))
            .collect();
        for dependency in &missing {
            self.waiting
                .entry(dependency.clone())
                .or_default()
                .insert(name.clone());
        }
        self.remaining.insert(name.clone(), missing);
        self.definitions.insert(
            name.clone(),
            Definition {
                value,
                source_value,
                span,
                position,
            },
        );
        let mut updates = Vec::new();
        self.fold_ready(&name, values, &mut updates);
        updates
    }

    fn fold_ready(
        &self,
        name: &str,
        values: &mut BTreeMap<String, i64>,
        updates: &mut Vec<(String, i64)>,
    ) {
        if self.remaining.get(name).is_some_and(|v| v.is_empty()) {
            let definition = &self.definitions[name];
            if let Ok(value) = definition.source_value.eval_with(
                &|key| values.get(key).copied(),
                None,
                definition.span.line as usize,
            ) {
                values.insert(name.to_string(), value);
                updates.push((name.to_string(), value));
            }
        }
    }

    /// Visit only aliases waiting on a value that has just become numeric.
    pub fn advance(
        &mut self,
        changes: Vec<(String, i64)>,
        values: &mut BTreeMap<String, i64>,
    ) -> Vec<(String, i64)> {
        let mut queue: VecDeque<_> = changes.into();
        let mut updates = Vec::new();
        while let Some((name, value)) = queue.pop_front() {
            self.invalidate_size(&name);
            updates.push((name.clone(), value));
            if let Some(dependants) = self.waiting.remove(&name) {
                for dependant in dependants {
                    let remaining = self.remaining.get_mut(&dependant).expect("waiting alias");
                    remaining.remove(&name);
                    let mut ready = Vec::new();
                    self.fold_ready(&dependant, values, &mut ready);
                    queue.extend(ready);
                }
            }
        }
        updates
    }

    /// Invalidate only cached dependants of the source event. Uncached chains
    /// are skipped, so propagating a long numeric chain remains linear.
    pub fn invalidate_size(&mut self, name: &str) {
        if self.size_cache.borrow().is_empty() {
            return;
        }
        let mut queue = VecDeque::from([name.to_string()]);
        self.size_cache.borrow_mut().remove(name);
        while let Some(name) = queue.pop_front() {
            if let Some(dependants) = self.waiting.get(&name) {
                for dependant in dependants {
                    if self.size_cache.borrow_mut().remove(dependant).is_some() {
                        queue.push_back(dependant.clone());
                    }
                }
            }
        }
    }

    /// Read source-time widths through the existing scoped reference keys.
    /// An outward size hint is not a numeric value in the current scope.
    pub fn size_hint(
        &self,
        value: &Expr,
        values: &BTreeMap<String, i64>,
        labels: &BTreeMap<String, String>,
        zero_page: &BTreeSet<String>,
        line: usize,
    ) -> Option<bool> {
        let mut found: BTreeMap<String, SizeHint> = BTreeMap::new();
        let mut visiting = BTreeSet::new();
        let mut stack: Vec<_> = symbols(value).into_iter().map(|key| (key, false)).collect();
        while let Some((key, after)) = stack.pop() {
            if found.contains_key(&key) {
                continue;
            }
            let target = super::scope_targets(&BTreeSet::from([key.clone()]), &|name| {
                values.contains_key(name)
                    || labels.contains_key(name)
                    || zero_page.contains(name)
                    || self.definitions.contains_key(name)
            })
            .remove(&key)
            .unwrap_or_else(|| key.clone());
            if target != key {
                let current = split_scope_ref(&key).map(|(path, name)| {
                    if path.is_empty() {
                        name.to_string()
                    } else {
                        format!("{path}::{name}")
                    }
                });
                if after {
                    let mut hint = found.get(&target).copied().unwrap_or_default();
                    if current.as_ref() != Some(&target) {
                        hint.value = None;
                    }
                    found.insert(key, hint);
                } else {
                    stack.push((key, true));
                    stack.push((target, false));
                }
                continue;
            }
            if let Some(number) = values.get(&key) {
                found.insert(key, SizeHint::known(*number));
                continue;
            }
            if let Some(hint) = self.size_cache.borrow().get(&key).copied() {
                found.insert(key, hint);
                continue;
            }
            if zero_page.contains(&key) || labels.get(&key).is_some_and(|seg| seg == "ZEROPAGE") {
                found.insert(key, SizeHint::sized(true));
                continue;
            }
            if labels.contains_key(&key) {
                found.insert(key, SizeHint::sized(false));
                continue;
            }
            let Some(definition) = self.definitions.get(&key) else {
                found.insert(key, SizeHint::default());
                continue;
            };
            if after {
                let hint =
                    expression_hint(&definition.value, &found, definition.span.line as usize);
                if hint.byte.is_some() {
                    self.size_cache.borrow_mut().insert(key.clone(), hint);
                }
                visiting.remove(&key);
                found.insert(key, hint);
            } else if visiting.insert(key.clone()) {
                stack.push((key, true));
                for dependency in symbols(&definition.value) {
                    if !visiting.contains(&dependency) {
                        stack.push((dependency, false));
                    }
                }
            }
        }
        expression_hint(value, &found, line).byte
    }

    /// Resolve the remaining graph once labels and definition PCs are placed.
    /// An iterative queue bounds stack use for long chains and circular input.
    pub fn finish(
        &self,
        values: &mut BTreeMap<String, i64>,
        positions: &BTreeMap<usize, i64>,
    ) -> Result<(), AsmError> {
        let mut waiting: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        let mut remaining = BTreeMap::new();
        let mut ready = VecDeque::new();
        for (name, definition) in &self.definitions {
            let dependencies = symbols(&definition.value);
            for dependency in &dependencies {
                if !values.contains_key(dependency) && !self.definitions.contains_key(dependency) {
                    return Err(AsmError::at(
                        definition.span.clone(),
                        format!("undefined symbol `{dependency}`"),
                    ));
                }
            }
            if values.contains_key(name) {
                continue;
            }
            let missing: BTreeSet<_> = dependencies
                .into_iter()
                .filter(|key| !values.contains_key(key))
                .collect();
            for dependency in &missing {
                waiting
                    .entry(dependency.clone())
                    .or_default()
                    .insert(name.clone());
            }
            if missing.is_empty() {
                ready.push_back(name.clone());
            }
            remaining.insert(name.clone(), missing);
        }
        while let Some(name) = ready.pop_front() {
            let definition = &self.definitions[&name];
            let value = definition
                .value
                .eval_with(
                    &|key| values.get(key).copied(),
                    positions.get(&definition.position).copied(),
                    definition.span.line as usize,
                )
                .map_err(|error| AsmError::at(definition.span.clone(), error.message))?;
            values.insert(name.clone(), value);
            remaining.remove(&name);
            if let Some(dependants) = waiting.remove(&name) {
                for dependant in dependants {
                    let missing = remaining.get_mut(&dependant).expect("pending alias");
                    missing.remove(&name);
                    if missing.is_empty() {
                        ready.push_back(dependant);
                    }
                }
            }
        }
        if let Some((name, _)) = remaining.first_key_value() {
            return Err(AsmError::at(
                self.definitions[name].span.clone(),
                format!("circular definition of symbol `{name}`"),
            ));
        }
        Ok(())
    }
}
