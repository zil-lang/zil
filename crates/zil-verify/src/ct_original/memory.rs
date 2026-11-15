//! Prepayment for the real private interpreter copies. Source trees stay
//! borrowed. Every recursive term is bounded on the same request meter.
use super::*;
use std::cmp::Ordering;
use std::mem::size_of;

fn sequence<T>(a: &[T], b: &[T], compare: fn(&T, &T) -> Ordering) -> Ordering {
    for (a, b) in a.iter().zip(b) {
        let order = compare(a, b);
        if order != Ordering::Equal {
            return order;
        }
    }
    a.len().cmp(&b.len())
}
fn binder(a: &Binder, b: &Binder) -> Ordering {
    (a.owner, a.ordinal).cmp(&(b.owner, b.ordinal))
}
fn meta_order(value: Meta) -> u8 {
    match value {
        Meta::Type => 0,
        Meta::Syntax(category) => 1 + category as u8,
        Meta::Ident => 5,
        Meta::TokenTree => 6,
        Meta::TokenCursor => 7,
        Meta::Text => 8,
        Meta::Shape => 9,
        Meta::Field => 10,
        Meta::Impl => 11,
        Meta::Layout => 12,
    }
}
pub(super) fn type_order(a: &Ty, b: &Ty) -> Ordering {
    fn rank(ty: &Ty) -> u8 {
        match ty {
            Ty::Scalar(_) => 0,
            Ty::Meta(_) => 1,
            Ty::Parameter(_) => 2,
            Ty::Tuple(_) => 3,
            Ty::Array(..) => 4,
            Ty::List(_) => 5,
            Ty::Sum(_) => 6,
            Ty::Nominal(..) => 7,
            Ty::Runtime(..) => 8,
        }
    }
    rank(a).cmp(&rank(b)).then_with(|| match (a, b) {
        (Ty::Scalar(a), Ty::Scalar(b)) => (*a as u8).cmp(&(*b as u8)),
        (Ty::Meta(a), Ty::Meta(b)) => meta_order(*a).cmp(&meta_order(*b)),
        (Ty::Parameter(a), Ty::Parameter(b)) => binder(a, b),
        (Ty::Tuple(a), Ty::Tuple(b)) | (Ty::Sum(a), Ty::Sum(b)) => sequence(a, b, type_order),
        (Ty::Array(a, n), Ty::Array(b, m)) => type_order(a, b).then_with(|| constant_order(n, m)),
        (Ty::List(a), Ty::List(b)) => type_order(a, b),
        (Ty::Nominal(a, x), Ty::Nominal(b, y)) => {
            a.cmp(b).then_with(|| sequence(x, y, actual_order))
        }
        (Ty::Runtime(a, x), Ty::Runtime(b, y)) => {
            a.cmp(b).then_with(|| sequence(x, y, actual_order))
        }
        _ => Ordering::Equal,
    })
}
fn actual_order(a: &Actual, b: &Actual) -> Ordering {
    match (a, b) {
        (Actual::Type(a), Actual::Type(b)) => type_order(a, b),
        (Actual::Const(a), Actual::Const(b)) => constant_order(a, b),
        (Actual::Life(a), Actual::Life(b)) => binder(a, b),
        (Actual::Type(_), _) => Ordering::Less,
        (_, Actual::Type(_)) => Ordering::Greater,
        (Actual::Const(_), _) => Ordering::Less,
        _ => Ordering::Greater,
    }
}
fn constant_order(a: &Constant, b: &Constant) -> Ordering {
    fn rank(term: &Term) -> u8 {
        match term {
            Term::Parameter(_) => 0,
            Term::Integer(_) => 1,
            Term::Bool(_) => 2,
            Term::Literal(_) => 3,
            Term::Unary(..) => 4,
            Term::Binary(..) => 5,
            Term::Convert(_) => 6,
        }
    }
    (a.carrier as u8)
        .cmp(&(b.carrier as u8))
        .then_with(|| rank(&a.term).cmp(&rank(&b.term)))
        .then_with(|| match (&a.term, &b.term) {
            (Term::Parameter(a), Term::Parameter(b)) => binder(a, b),
            (Term::Integer(a), Term::Integer(b)) => a.cmp(b),
            (Term::Bool(a), Term::Bool(b)) => a.cmp(b),
            (Term::Literal(a), Term::Literal(b)) => a.cmp(b),
            (Term::Unary(a, x), Term::Unary(b, y)) => (*a as u8)
                .cmp(&(*b as u8))
                .then_with(|| constant_order(x, y)),
            (Term::Binary(a, x, y), Term::Binary(b, z, w)) => (*a as u8)
                .cmp(&(*b as u8))
                .then_with(|| constant_order(x, z))
                .then_with(|| constant_order(y, w)),
            (Term::Convert(a), Term::Convert(b)) => constant_order(a, b),
            _ => Ordering::Equal,
        })
}

impl Checker<'_, '_, '_> {
    fn bytes_add(&self, a: usize, b: usize, span: Span) -> R<usize> {
        a.checked_add(b).ok_or_else(|| {
            Diagnostic::limit(Phase::Ct, "CT original copy storage overflow", Some(span))
        })
    }
    fn term_bytes(&mut self, value: &Constant, span: Span, depth: usize) -> R<usize> {
        self.step(span, depth)?;
        match &value.term {
            Term::Literal(text) => {
                self.work(text.len(), span)?;
                Ok(text.len())
            }
            Term::Unary(_, value) | Term::Convert(value) => {
                let child = self.term_bytes(value, span, depth + 1)?;
                self.bytes_add(size_of::<Constant>(), child, span)
            }
            Term::Binary(_, a, b) => {
                let a = self.term_bytes(a, span, depth + 1)?;
                let b = self.term_bytes(b, span, depth + 1)?;
                let children = self.bytes_add(a, b, span)?;
                self.bytes_add(size_of::<Constant>().saturating_mul(2), children, span)
            }
            _ => Ok(0),
        }
    }
    fn actual_bytes(&mut self, actual: &Actual, span: Span, depth: usize) -> R<usize> {
        self.step(span, depth)?;
        match actual {
            Actual::Type(ty) => self.type_bytes(ty, span, depth + 1),
            Actual::Const(value) => self.term_bytes(value, span, depth + 1),
            Actual::Life(_) => Ok(0),
        }
    }
    fn type_bytes(&mut self, ty: &Ty, span: Span, depth: usize) -> R<usize> {
        self.step(span, depth)?;
        match ty {
            Ty::Tuple(fields) | Ty::Sum(fields) => {
                let mut bytes = fields
                    .len()
                    .checked_mul(size_of::<Ty>())
                    .ok_or_else(|| bad(span, "CT original type vector overflow"))?;
                for ty in fields {
                    let child = self.type_bytes(ty, span, depth + 1)?;
                    bytes = self.bytes_add(bytes, child, span)?;
                }
                Ok(bytes)
            }
            Ty::Array(inner, length) => {
                let inner = self.type_bytes(inner, span, depth + 1)?;
                let length = self.term_bytes(length, span, depth + 1)?;
                let children = self.bytes_add(inner, length, span)?;
                self.bytes_add(size_of::<Ty>(), children, span)
            }
            Ty::List(inner) => {
                let child = self.type_bytes(inner, span, depth + 1)?;
                self.bytes_add(size_of::<Ty>(), child, span)
            }
            Ty::Nominal(_, actuals) | Ty::Runtime(_, actuals) => {
                let mut bytes = actuals
                    .len()
                    .checked_mul(size_of::<Actual>())
                    .ok_or_else(|| bad(span, "CT original actual vector overflow"))?;
                for actual in actuals {
                    let child = self.actual_bytes(actual, span, depth + 1)?;
                    bytes = self.bytes_add(bytes, child, span)?;
                }
                Ok(bytes)
            }
            _ => Ok(0),
        }
    }
    fn value_bytes(&mut self, value: &Value, span: Span) -> R<usize> {
        let mut bytes = self.type_bytes(&value.ty, span, 0)?;
        if let Some(value) = &value.constant {
            let child = self.term_bytes(value, span, 0)?;
            bytes = self.bytes_add(bytes, child, span)?;
        }
        if let Some(ty) = &value.represented {
            let child = self.type_bytes(ty, span, 0)?;
            bytes = self.bytes_add(bytes, child, span)?;
        }
        Ok(bytes)
    }
    pub(super) fn duplicate_type(&mut self, ty: &Ty, span: Span) -> R<Ty> {
        let bytes = self.type_bytes(ty, span, 0)?;
        self.alloc(bytes, span)?;
        Ok(ty.clone())
    }
    pub(super) fn duplicate_constant(&mut self, value: &Constant, span: Span) -> R<Constant> {
        let bytes = self.term_bytes(value, span, 0)?;
        self.alloc(bytes, span)?;
        Ok(value.clone())
    }
    pub(super) fn duplicate_actual(&mut self, actual: &Actual, span: Span) -> R<Actual> {
        let bytes = self.actual_bytes(actual, span, 0)?;
        self.alloc(bytes, span)?;
        Ok(actual.clone())
    }
    pub(super) fn duplicate_actuals(&mut self, actuals: &[Actual], span: Span) -> R<Vec<Actual>> {
        let mut bytes = actuals
            .len()
            .checked_mul(size_of::<Actual>())
            .ok_or_else(|| bad(span, "CT actual copy storage overflow"))?;
        for actual in actuals {
            let child = self.actual_bytes(actual, span, 0)?;
            bytes = self.bytes_add(bytes, child, span)?;
        }
        self.alloc(bytes, span)?;
        Ok(actuals.to_vec())
    }
    pub(super) fn duplicate_value(&mut self, value: &Value, span: Span) -> R<Value> {
        let bytes = self.value_bytes(value, span)?;
        self.alloc(bytes, span)?;
        Ok(value.clone())
    }
    pub(super) fn copy_environment(&mut self, env: &Env, span: Span) -> R<Env> {
        let mut bytes = env
            .generics
            .len()
            .checked_mul(size_of::<(String, Actual)>())
            .and_then(|n| n.checked_add(env.values.len().checked_mul(size_of::<Binding>())?))
            .ok_or_else(|| bad(span, "CT environment copy storage overflow"))?;
        for (name, actual) in &env.generics {
            let child = self.actual_bytes(actual, span, 0)?;
            bytes = self.bytes_add(bytes, name.len(), span)?;
            bytes = self.bytes_add(bytes, child, span)?;
        }
        for binding in &env.values {
            let child = self.value_bytes(&binding.value, span)?;
            bytes = self.bytes_add(bytes, binding.name.len(), span)?;
            bytes = self.bytes_add(bytes, child, span)?;
        }
        let result = self.type_bytes(&env.result, span, 0)?;
        bytes = self.bytes_add(bytes, result, span)?;
        if let Some(Some(ty)) = &env.yielding {
            let child = self.type_bytes(ty, span, 0)?;
            bytes = self.bytes_add(bytes, child, span)?;
        }
        self.alloc(bytes, span)?;
        Ok(env.clone())
    }
    pub(super) fn vector<T>(&mut self, count: usize, span: Span) -> R<Vec<T>> {
        let bytes = count.checked_mul(size_of::<T>()).ok_or_else(|| {
            Diagnostic::limit(Phase::Ct, "CT original vector storage overflow", Some(span))
        })?;
        self.alloc(bytes, span)?;
        let mut values = Vec::new();
        values.try_reserve_exact(count).map_err(|_| {
            Diagnostic::limit(
                Phase::Ct,
                "CT original vector allocation failed",
                Some(span),
            )
        })?;
        Ok(values)
    }
    pub(super) fn append<T>(&mut self, values: &mut Vec<T>, value: T, span: Span) -> R<()> {
        if values.len() == values.capacity() {
            let growth = values.capacity().max(4);
            let bytes = growth.checked_mul(size_of::<T>()).ok_or_else(|| {
                Diagnostic::limit(Phase::Ct, "CT original vector growth overflow", Some(span))
            })?;
            self.alloc(bytes, span)?;
            values.try_reserve_exact(growth).map_err(|_| {
                Diagnostic::limit(Phase::Ct, "CT original vector growth failed", Some(span))
            })?;
        }
        values.push(value);
        Ok(())
    }
}
