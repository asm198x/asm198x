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
    size_cache: std::cell::RefCell<BTreeMap<String, i64>>,
}

fn symbols(value: &Expr) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    collect_syms(&Kind::Bytes(vec![value.clone()]), &mut names);
    names
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

    /// A sparse, cached operand-size view. These placeholder values select
    /// forms only; the retained expressions still emit their final values.
    pub fn size_value(
        &self,
        name: &str,
        values: &BTreeMap<String, i64>,
        zero_page: &impl Fn(&str) -> bool,
    ) -> Option<i64> {
        let mut found: BTreeMap<String, Option<i64>> = BTreeMap::new();
        let mut visiting = BTreeSet::new();
        let mut stack = vec![(name.to_string(), false)];
        while let Some((key, after)) = stack.pop() {
            if found.contains_key(&key) {
                continue;
            }
            if let Some(value) = values
                .get(&key)
                .copied()
                .or_else(|| self.size_cache.borrow().get(&key).copied())
                .or_else(|| zero_page(&key).then_some(0))
            {
                found.insert(key, Some(value));
                continue;
            }
            let Some(definition) = self.definitions.get(&key) else {
                found.insert(key, None);
                continue;
            };
            if matches!(
                definition.source_value,
                Expr::Lo(_) | Expr::Hi(_) | Expr::Bank(_)
            ) {
                found.insert(key.clone(), Some(0));
                self.size_cache.borrow_mut().insert(key, 0);
                continue;
            }
            if after {
                let value = definition
                    .source_value
                    .eval_with(
                        &|dependency| found.get(dependency).copied().flatten(),
                        None,
                        definition.span.line as usize,
                    )
                    .ok();
                if let Some(value) = value {
                    self.size_cache.borrow_mut().insert(key.clone(), value);
                }
                visiting.remove(&key);
                found.insert(key, value);
            } else if visiting.insert(key.clone()) {
                stack.push((key, true));
                for dependency in symbols(&definition.source_value) {
                    if !visiting.contains(&dependency) {
                        stack.push((dependency, false));
                    }
                }
            }
        }
        found.get(name).copied().flatten()
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
