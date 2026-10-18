//! The parameter table: definitions in insertion order, transactional edits, cycle detection.

use std::collections::BTreeMap;

use burnburn_types::{Angle, Length};

use super::{Dim, ExprError, ExprErrorKind, Expression, ParameterError, Quantity, Span, Value, valid_name};

/// Named parameter definitions. Every edit is checked against the whole table and either
/// applies completely or leaves the table unchanged.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Parameters {
    entries: Vec<(String, Expression)>,
}

/// Evaluated parameter values.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Values {
    values: BTreeMap<String, Value>,
}

impl Values {
    pub fn get(&self, name: &str) -> Option<Value> {
        self.values.get(name).copied()
    }

    pub(crate) fn quantity(&self, name: &str) -> Option<Quantity> {
        self.get(name).map(|v| v.quantity())
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, Value)> {
        self.values.iter().map(|(k, v)| (k.as_str(), *v))
    }
}

impl Parameters {
    pub fn new() -> Self {
        Parameters::default()
    }

    /// Builds a table from stored `(name, text)` definitions (any order of dependencies) and
    /// checks it as a whole. Spans in errors are relative to each definition's text.
    pub fn from_definitions<'a>(defs: impl IntoIterator<Item = (&'a str, &'a str)>) -> Result<Self, ParameterError> {
        let mut table = Parameters::new();
        for (name, text) in defs {
            if !valid_name(name) || table.entries.iter().any(|(n, _)| n == name) {
                return Err(ParameterError::InvalidName { name: name.to_owned(), span: Span::new(0, name.len()) });
            }
            let expr = Expression::parse(text)
                .map_err(|error| ParameterError::Expression { parameter: name.to_owned(), error })?;
            table.entries.push((name.to_owned(), expr));
        }
        table.evaluate()?;
        Ok(table)
    }

    /// Defines or redefines from a line `name = expression`. Error spans point into `line`.
    pub fn define(&mut self, line: &str) -> Result<(), ParameterError> {
        let Some(eq) = line.find('=') else {
            let span = Span::new(0, line.len());
            let error = ExprError { kind: ExprErrorKind::ExpectedAssignment, span };
            return Err(ParameterError::Expression { parameter: line.trim().to_owned(), error });
        };
        let lhs = &line[..eq];
        let name = lhs.trim();
        let lead = lhs.len() - lhs.trim_start().len();
        if !valid_name(name) {
            return Err(ParameterError::InvalidName {
                name: name.to_owned(),
                span: Span::new(lead, lead + name.len()),
            });
        }
        let rhs = &line[eq + 1..];
        let offset = eq + 1;
        // Parse once at the line offset for line-relative syntax errors; store rhs-relative.
        Expression::parse_at(rhs, offset)
            .map_err(|error| ParameterError::Expression { parameter: name.to_owned(), error })?;
        let stored =
            Expression::parse(rhs).map_err(|error| ParameterError::Expression { parameter: name.to_owned(), error })?;
        self.commit(name, stored, offset)
    }

    /// Replaces the expression of an existing parameter. Error spans point into `expression`.
    pub fn set(&mut self, name: &str, expression: &str) -> Result<(), ParameterError> {
        if !self.entries.iter().any(|(n, _)| n == name) {
            return Err(ParameterError::UnknownParameter(name.to_owned()));
        }
        let expr = Expression::parse(expression)
            .map_err(|error| ParameterError::Expression { parameter: name.to_owned(), error })?;
        self.commit(name, expr, 0)
    }

    /// Removes an unreferenced parameter.
    pub fn remove(&mut self, name: &str) -> Result<(), ParameterError> {
        let at = self
            .entries
            .iter()
            .position(|(n, _)| n == name)
            .ok_or_else(|| ParameterError::UnknownParameter(name.to_owned()))?;
        let by: Vec<String> = self
            .entries
            .iter()
            .filter(|(n, e)| n != name && e.references().contains(&name))
            .map(|(n, _)| n.clone())
            .collect();
        if !by.is_empty() {
            return Err(ParameterError::StillReferenced { parameter: name.to_owned(), by });
        }
        self.entries.remove(at);
        Ok(())
    }

    pub fn expression(&self, name: &str) -> Option<&Expression> {
        self.entries.iter().find(|(n, _)| n == name).map(|(_, e)| e)
    }

    /// Names in definition order.
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.entries.iter().map(|(n, _)| n.as_str())
    }

    /// Evaluates every parameter. Spans in errors are relative to each stored expression.
    pub fn evaluate(&self) -> Result<Values, ParameterError> {
        let order = self.order()?;
        let mut values = Values::default();
        for i in order {
            let (name, expr) = &self.entries[i];
            let wrap = |error| ParameterError::Expression { parameter: name.clone(), error };
            let q = expr.evaluate(&|n| values.quantity(n)).map_err(wrap)?;
            let v = to_value(q, expr.span()).map_err(wrap)?;
            values.values.insert(name.clone(), v);
        }
        Ok(values)
    }

    /// Applies `name := expr` on a copy, checks the whole table, and keeps it only if valid.
    fn commit(&mut self, name: &str, expr: Expression, offset: usize) -> Result<(), ParameterError> {
        let mut next = self.clone();
        match next.entries.iter_mut().find(|(n, _)| n == name) {
            Some(slot) => slot.1 = expr,
            None => next.entries.push((name.to_owned(), expr)),
        }
        next.evaluate().map_err(|e| match e {
            ParameterError::Expression { parameter, mut error } if parameter == name => {
                error.span = error.span.shifted(offset);
                ParameterError::Expression { parameter, error }
            }
            other => other,
        })?;
        *self = next;
        Ok(())
    }

    /// Evaluation order (dependencies first), or the first cycle found. Deterministic: entries
    /// are visited in definition order and references in order of appearance.
    fn order(&self) -> Result<Vec<usize>, ParameterError> {
        #[derive(Clone, Copy, PartialEq)]
        enum Mark {
            New,
            Active,
            Done,
        }
        let index = |name: &str| self.entries.iter().position(|(n, _)| n == name);
        let mut mark = vec![Mark::New; self.entries.len()];
        let mut order = Vec::with_capacity(self.entries.len());
        for root in 0..self.entries.len() {
            if mark[root] != Mark::New {
                continue;
            }
            // Iterative DFS: (entry, next reference to visit).
            let mut stack: Vec<(usize, usize)> = vec![(root, 0)];
            mark[root] = Mark::Active;
            while let Some(&(node, next)) = stack.last() {
                let refs = self.entries[node].1.references();
                if let Some(r) = refs.get(next) {
                    if let Some(top) = stack.last_mut() {
                        top.1 += 1;
                    }
                    let Some(dep) = index(r) else { continue };
                    match mark[dep] {
                        Mark::New => {
                            mark[dep] = Mark::Active;
                            stack.push((dep, 0));
                        }
                        Mark::Active => {
                            let from = stack.iter().position(|(n, _)| *n == dep).unwrap_or(0);
                            let mut cycle: Vec<String> =
                                stack[from..].iter().map(|(n, _)| self.entries[*n].0.clone()).collect();
                            cycle.push(self.entries[dep].0.clone());
                            return Err(ParameterError::Cycle { cycle });
                        }
                        Mark::Done => {}
                    }
                } else {
                    mark[node] = Mark::Done;
                    order.push(node);
                    stack.pop();
                }
            }
        }
        Ok(order)
    }
}

/// A quantity as a parameter value; only lengths, angles and numbers are values.
pub(crate) fn to_value(q: Quantity, span: Span) -> Result<Value, ExprError> {
    let bad = |kind| ExprError { kind, span };
    match q.dim {
        Dim::LENGTH => Length::from_mm(q.value).map(Value::Length).map_err(|_| bad(ExprErrorKind::NotFinite)),
        Dim::ANGLE => Angle::from_radians(q.value).map(Value::Angle).map_err(|_| bad(ExprErrorKind::NotFinite)),
        Dim::NONE => Ok(Value::Number(q.value)),
        d => Err(bad(ExprErrorKind::UnsupportedDimension(d))),
    }
}
