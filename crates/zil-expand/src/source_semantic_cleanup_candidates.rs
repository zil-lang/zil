//! Untrusted original cleanup and complete-consuming effect candidates.
//! Known Task/registration residuals can select an implicit body Suspend upper;
//! opaque FnOnce results also include the returned environment's retirement.
//! These candidates grant no Closed, SyncDrop, frame or NoExternal proof.
use super::*;

// These are candidate effects over original ownership, not SyncDrop or Closed
// theorems. An abstract type's missing cleanup premise is deliberately distinct
// from a known Task/registration whose cleanup can require a continuation.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
struct Cleanup {
    may: bool,
    asynchronous: bool,
}
impl Cleanup {
    const NONE: Self = Self {
        may: false,
        asynchronous: false,
    };
    const ABSTRACT: Self = Self {
        may: true,
        asynchronous: false,
    };
    const ASYNC: Self = Self {
        may: true,
        asynchronous: true,
    };
}
impl std::ops::BitOr for Cleanup {
    type Output = Self;
    fn bitor(self, other: Self) -> Self {
        Self {
            may: self.may || other.may,
            asynchronous: self.asynchronous || other.asynchronous,
        }
    }
}
impl std::ops::BitOrAssign for Cleanup {
    fn bitor_assign(&mut self, other: Self) {
        *self = *self | other;
    }
}
#[derive(Clone, Copy)]
enum ReadyCleanup {
    NoReturn,
    Unknown,
    Known(Cleanup),
}
impl ReadyCleanup {
    fn join(self, other: Self) -> Self {
        match (self, other) {
            (Self::NoReturn, value) | (value, Self::NoReturn) => value,
            (Self::Known(a), Self::Known(b)) => Self::Known(a | b),
            _ => Self::Unknown,
        }
    }
    fn known(self) -> Option<Cleanup> {
        if let Self::Known(value) = self {
            Some(value)
        } else {
            None
        }
    }
}
struct Place<'a> {
    owner: usize,
    index: usize,
    fields: Vec<&'a str>,
}
enum Value<'a> {
    Plain,
    Task,
    // A real created worker plus its complete input graph has a synchronous
    // shutdown candidate. This is distinct from a Closed observation.
    SyncTask,
    SettledTask,
    Unknown,
    UnknownAsync,
    View {
        place: Place<'a>,
        mutable: bool,
    },
    Poll(Place<'a>),
    Error {
        is_error: bool,
        value: Box<Value<'a>>,
    },
    Tuple(Vec<Value<'a>>),
    Alternatives(Vec<Value<'a>>),
    Record(Vec<(&'a str, Value<'a>)>),
    Callable {
        definition: Option<usize>,
        scope: usize,
        kind: CallableKind,
        signature: &'a SemanticSignatureRecipe,
        suspend: bool,
        known_suspend: bool,
        shutdown: bool,
        cleanup: Cleanup,
        initial: Cleanup,
        unknown_write: Cleanup,
        after: Cleanup,
        ready: Option<Cleanup>,
    },
    Function(usize),
}
// Release argument-evaluation temporaries before entering the callee's body.
enum CallOutcome<'a> {
    Value(Value<'a>),
    Defined {
        owner: usize,
        inputs: Vec<Value<'a>>,
    },
}
struct Flow<'a> {
    value: Option<Value<'a>>,
    live: bool,
    cleanup: Cleanup,
    body_cleanup: Cleanup,
    ready: ReadyCleanup,
    breaks: Vec<Env<'a>>,
    continues: Vec<Env<'a>>,
    incomplete: bool,
    shutdown: bool,
}
impl<'a> Flow<'a> {
    fn new() -> Self {
        Self {
            value: None,
            live: true,
            cleanup: Cleanup::NONE,
            body_cleanup: Cleanup::NONE,
            ready: ReadyCleanup::NoReturn,
            breaks: Vec::new(),
            continues: Vec::new(),
            incomplete: false,
            shutdown: true,
        }
    }
    fn drop_value(&mut self, cleanup: Cleanup) {
        self.cleanup |= cleanup;
        self.body_cleanup |= cleanup;
    }
}
struct TypeFrame<'a> {
    declaration: usize,
    caller: usize,
    arguments: &'a [SemanticActualRecipe],
    context: Option<usize>,
}
struct PlainCall {
    target: usize,
    inputs: usize,
    depth: usize,
    shutdown: bool,
}
struct Walk<'a, 'b, 'l> {
    definitions: &'a [&'a SemanticDefinition],
    active: Vec<usize>,
    last_plain_call: Option<PlainCall>,
    frames: Vec<TypeFrame<'a>>,
    context: Option<usize>,
    execution: bool,
    known_execution: bool,
    publish: bool,
    shutdown: bool,
    evidence: &'b mut super::Evidence,
    seed_changed: &'b mut bool,
    held_captures: usize,
    early_cleanup: Cleanup,
    early_body_cleanup: Cleanup,
    prepared: Cleanup,
    prepared_values: Vec<Value<'a>>,
    prepared_base: usize,
    bodies: &'b mut [bool],
    budget: &'b mut Budget<'l>,
}
type Env<'a> = Vec<(&'a str, Value<'a>)>;

fn copy_rows<T>(count: usize, budget: &mut Budget<'_>) -> R<Vec<T>> {
    let span = Span::default();
    budget.lineage_bytes(
        count
            .checked_mul(std::mem::size_of::<T>())
            .ok_or_else(|| error(span, "cleanup snapshot allocation overflow"))?,
        span,
    )?;
    let mut rows = Vec::new();
    rows.try_reserve_exact(count)
        .map_err(|_| error(span, "cleanup snapshot allocation failed"))?;
    Ok(rows)
}

impl<'a> Walk<'a, '_, '_> {
    fn copy(&mut self, value: &Value<'a>, depth: usize) -> R<Value<'a>> {
        self.budget.node(Span::default(), depth)?;
        Ok(match value {
            Value::Plain => Value::Plain,
            Value::Task => Value::Task,
            Value::SyncTask => Value::SyncTask,
            Value::SettledTask => Value::SettledTask,
            Value::Unknown => Value::Unknown,
            Value::UnknownAsync => Value::UnknownAsync,
            Value::View { place, mutable } => Value::View {
                place: self.place_copy(place, depth + 1)?,
                mutable: *mutable,
            },
            Value::Poll(place) => Value::Poll(self.place_copy(place, depth + 1)?),
            Value::Error { is_error, value } => {
                let value = self.copy(value, depth + 1)?;
                self.budget
                    .lineage_bytes(std::mem::size_of::<Value<'a>>(), Span::default())?;
                Value::Error {
                    is_error: *is_error,
                    value: Box::new(value),
                }
            }
            Value::Function(owner) => Value::Function(*owner),
            Value::Callable {
                definition,
                scope,
                kind,
                signature,
                suspend,
                known_suspend,
                shutdown,
                cleanup,
                initial,
                unknown_write,
                after,
                ready,
            } => Value::Callable {
                definition: *definition,
                scope: *scope,
                kind: *kind,
                signature,
                suspend: *suspend,
                known_suspend: *known_suspend,
                shutdown: *shutdown,
                cleanup: *cleanup,
                initial: *initial,
                unknown_write: *unknown_write,
                after: *after,
                ready: *ready,
            },
            Value::Alternatives(fields) => {
                Value::Alternatives(self.copy_values(fields, depth + 1)?)
            }
            Value::Tuple(fields) => Value::Tuple(self.copy_values(fields, depth + 1)?),
            Value::Record(fields) => {
                let mut result = copy_rows(fields.len(), self.budget)?;
                for (name, field) in fields {
                    let value = self.copy(field, depth + 1)?;
                    self.budget.node(Span::default(), 0)?;
                    result.push((*name, value));
                }
                Value::Record(result)
            }
        })
    }
    fn copy_values(&mut self, values: &[Value<'a>], depth: usize) -> R<Vec<Value<'a>>> {
        let mut result = copy_rows(values.len(), self.budget)?;
        for value in values {
            let value = self.copy(value, depth)?;
            self.budget.node(Span::default(), 0)?;
            result.push(value);
        }
        Ok(result)
    }
    fn env_copy(&mut self, env: &Env<'a>, depth: usize) -> R<Env<'a>> {
        let mut result = copy_rows(env.len(), self.budget)?;
        for (name, value) in env {
            let value = self.copy(value, depth + 1)?;
            self.budget.node(Span::default(), 0)?;
            result.push((*name, value));
        }
        Ok(result)
    }
    fn place_copy(&mut self, place: &Place<'a>, depth: usize) -> R<Place<'a>> {
        self.budget.node(Span::default(), depth)?;
        let mut fields = copy_rows(place.fields.len(), self.budget)?;
        for field in &place.fields {
            self.budget.text(field, Span::default())?;
            self.budget.node(Span::default(), 0)?;
            fields.push(*field);
        }
        Ok(Place {
            owner: place.owner,
            index: place.index,
            fields,
        })
    }
    fn same_place(&mut self, a: &Place<'a>, b: &Place<'a>, depth: usize) -> R<bool> {
        self.budget.node(Span::default(), depth)?;
        if a.owner != b.owner || a.index != b.index || a.fields.len() != b.fields.len() {
            return Ok(false);
        }
        for (a, b) in a.fields.iter().zip(&b.fields) {
            self.budget.text(a, Span::default())?;
            self.budget.text(b, Span::default())?;
            if a != b {
                return Ok(false);
            }
        }
        Ok(true)
    }
    fn place(
        &mut self,
        owner: usize,
        env: &Env<'a>,
        expr: &'a SemanticOriginalExpr,
        depth: usize,
    ) -> R<Option<Place<'a>>> {
        self.budget.node(expr.span, depth)?;
        Ok(match &expr.kind {
            CoreExprKind::Group(inner)
            | CoreExprKind::Ascribed { value: inner, .. }
            | CoreExprKind::Unary {
                op: UnaryOp::Borrow | UnaryOp::BorrowMut | UnaryOp::Dereference,
                operand: inner,
            } => self.place(owner, env, inner, depth + 1)?,
            CoreExprKind::Name(name) => {
                let mut found = None;
                for (index, (binding, value)) in env.iter().enumerate().rev() {
                    self.budget.text(name, expr.span)?;
                    if *binding == name {
                        found = Some(if let Value::View { place, .. } = value {
                            self.place_copy(place, depth + 1)?
                        } else {
                            Place {
                                owner,
                                index,
                                fields: Vec::new(),
                            }
                        });
                        break;
                    }
                }
                found
            }
            CoreExprKind::Member { object, member, .. } => {
                let Some(mut place) = self.place(owner, env, object, depth + 1)? else {
                    return Ok(None);
                };
                self.budget
                    .lineage_push(&mut place.fields, member.as_str(), expr.span)?;
                Some(place)
            }
            _ => None,
        })
    }
    fn borrowed_place(
        &mut self,
        owner: usize,
        env: &Env<'a>,
        expression: &'a SemanticOriginalExpr,
        depth: usize,
    ) -> R<Option<Place<'a>>> {
        self.budget.node(expression.span, depth)?;
        match &expression.kind {
            CoreExprKind::Group(inner) | CoreExprKind::Ascribed { value: inner, .. } => {
                self.borrowed_place(owner, env, inner, depth + 1)
            }
            CoreExprKind::Name(name) => {
                for (index, (binding, _)) in env.iter().enumerate().rev() {
                    self.budget.text(name, expression.span)?;
                    if *binding == name {
                        return Ok(Some(Place {
                            owner,
                            index,
                            fields: Vec::new(),
                        }));
                    }
                }
                Ok(None)
            }
            _ => self.place(owner, env, expression, depth + 1),
        }
    }
    fn place_value_mut<'c>(
        &mut self,
        owner: usize,
        env: &'c mut Env<'a>,
        place: &Place<'a>,
        depth: usize,
    ) -> R<Option<&'c mut Value<'a>>> {
        self.budget.node(Span::default(), depth)?;
        if place.owner != owner {
            return Ok(None);
        }
        let Some(entry) = env.get_mut(place.index) else {
            return Ok(None);
        };
        let mut value = &mut entry.1;
        for field in &place.fields {
            self.budget.text(field, Span::default())?;
            value = match value {
                Value::Tuple(values) => {
                    let Some(value) = field
                        .parse::<usize>()
                        .ok()
                        .and_then(|index| values.get_mut(index))
                    else {
                        return Ok(None);
                    };
                    value
                }
                Value::Record(values) => {
                    self.budget.identity_work(values.len(), Span::default())?;
                    let mut selected = None;
                    for (name, value) in values {
                        self.budget.text(name, Span::default())?;
                        if name == field {
                            selected = Some(value);
                            break;
                        }
                    }
                    let Some(value) = selected else {
                        return Ok(None);
                    };
                    value
                }
                _ => return Ok(None),
            };
        }
        Ok(Some(value))
    }
    fn invalidate_value(
        budget: &mut Budget<'_>,
        value: &mut Value<'a>,
        place: &Place<'a>,
        depth: usize,
    ) -> R<()> {
        budget.node(Span::default(), depth)?;
        match value {
            Value::Poll(old) if old.owner == place.owner && old.index == place.index => {
                let mut overlaps = true;
                for (a, b) in old.fields.iter().zip(&place.fields) {
                    budget.text(a, Span::default())?;
                    budget.text(b, Span::default())?;
                    overlaps &= a == b;
                }
                if overlaps {
                    *value = Value::Plain;
                }
            }
            Value::Tuple(values) | Value::Alternatives(values) => {
                for value in values {
                    Self::invalidate_value(budget, value, place, depth + 1)?;
                }
            }
            Value::Record(values) => {
                for (_, value) in values {
                    Self::invalidate_value(budget, value, place, depth + 1)?;
                }
            }
            Value::Error { value, .. } => Self::invalidate_value(budget, value, place, depth + 1)?,
            _ => {}
        }
        Ok(())
    }
    fn invalidate(&mut self, env: &mut Env<'a>, place: &Place<'a>, depth: usize) -> R<()> {
        for (_, value) in env {
            Self::invalidate_value(self.budget, value, place, depth + 1)?;
        }
        // Earlier fields/arguments are real operands even before they have a
        // lexical binding. A later operand cannot revive their old Poll fact.
        for value in &mut self.prepared_values[self.prepared_base..] {
            Self::invalidate_value(self.budget, value, place, depth + 1)?;
        }
        Ok(())
    }
    fn forget_mutated(budget: &mut Budget<'_>, value: &mut Value<'a>, depth: usize) -> R<()> {
        budget.node(Span::default(), depth)?;
        match value {
            Value::SettledTask | Value::SyncTask => *value = Value::Task,
            Value::Callable {
                cleanup,
                initial,
                unknown_write,
                shutdown,
                after,
                ready,
                ..
            } => {
                // A mutable helper can replace this factory's occupant. Its
                // old actual SyncTask birth and conditional Ready receipt do
                // not belong to the replacement. Keep only the original
                // complete owning-source upper, not an invented SyncDrop.
                *cleanup |= *unknown_write;
                *initial |= *unknown_write;
                *after |= *unknown_write;
                *shutdown = false;
                *ready = None;
            }
            Value::Tuple(values) | Value::Alternatives(values) => {
                for value in values {
                    Self::forget_mutated(budget, value, depth + 1)?;
                }
            }
            Value::Record(values) => {
                for (_, value) in values {
                    Self::forget_mutated(budget, value, depth + 1)?;
                }
            }
            Value::Error { value, .. } => Self::forget_mutated(budget, value, depth + 1)?,
            // An arbitrary write may replace a pointer or a poll value; its
            // prior provenance is not a relation of the replacement.
            Value::Poll(_) | Value::View { .. } | Value::Function(_) => *value = Value::Plain,
            _ => {}
        }
        Ok(())
    }
    fn park(&mut self, value: Value<'a>, owned: bool, span: Span, depth: usize) -> R<()> {
        if owned {
            let cleanup = self.pending(&value, depth + 1)?;
            self.prepared |= cleanup;
        }
        self.budget
            .lineage_push(&mut self.prepared_values, value, span)
    }
    fn unpark(&mut self, base: usize, span: Span) -> R<Vec<Value<'a>>> {
        // split_off allocates the result while the caller's outer operand
        // frame remains in place. Its storage is charged before allocation.
        let bytes = self
            .prepared_values
            .len()
            .saturating_sub(base)
            .saturating_mul(std::mem::size_of::<Value<'a>>());
        self.budget.lineage_bytes(bytes, span)?;
        Ok(self.prepared_values.split_off(base))
    }
    fn enter_operand_frame(&mut self) -> (Cleanup, usize) {
        (
            std::mem::replace(&mut self.prepared, Cleanup::NONE),
            std::mem::replace(&mut self.prepared_base, self.prepared_values.len()),
        )
    }
    fn leave_operand_frame(&mut self, (prepared, base): (Cleanup, usize)) {
        self.prepared_values.truncate(self.prepared_base);
        self.prepared_base = base;
        self.prepared = prepared;
    }
    fn refine_ready(
        &mut self,
        owner: usize,
        env: &mut Env<'a>,
        value: &Value<'a>,
        depth: usize,
    ) -> R<bool> {
        let Value::Poll(place) = value else {
            return Ok(false);
        };
        let Some(value) = self.place_value_mut(owner, env, place, depth + 1)? else {
            return Ok(false);
        };
        match value {
            Value::Task | Value::SyncTask | Value::SettledTask => {
                *value = Value::SettledTask;
                Ok(true)
            }
            Value::Callable {
                cleanup,
                ready: Some(ready),
                ..
            } => {
                *cleanup = *ready;
                Ok(true)
            }
            _ => Ok(false),
        }
    }
    fn ready_cleanup(
        &mut self,
        owner: usize,
        env: &Env<'a>,
        value: &Value<'a>,
        depth: usize,
    ) -> R<ReadyCleanup> {
        if !matches!(value, Value::Poll(_)) {
            return Ok(ReadyCleanup::Unknown);
        }
        let mut ready = self.env_copy(env, depth + 1)?;
        if self.refine_ready(owner, &mut ready, value, depth + 1)? {
            Ok(ReadyCleanup::Known(self.residual(&ready, depth + 1)?))
        } else {
            Ok(ReadyCleanup::Unknown)
        }
    }
    fn same_value(&mut self, a: &Value<'a>, b: &Value<'a>, depth: usize) -> R<bool> {
        self.budget.node(Span::default(), depth)?;
        Ok(match (a, b) {
            (Value::Plain, Value::Plain)
            | (Value::Task, Value::Task)
            | (Value::SyncTask, Value::SyncTask)
            | (Value::SettledTask, Value::SettledTask)
            | (Value::Unknown, Value::Unknown)
            | (Value::UnknownAsync, Value::UnknownAsync) => true,
            (Value::Function(a), Value::Function(b)) => a == b,
            (
                Value::View {
                    place: a,
                    mutable: am,
                },
                Value::View {
                    place: b,
                    mutable: bm,
                },
            ) => am == bm && self.same_place(a, b, depth + 1)?,
            (Value::Poll(a), Value::Poll(b)) => self.same_place(a, b, depth + 1)?,
            (Value::Tuple(a), Value::Tuple(b))
            | (Value::Alternatives(a), Value::Alternatives(b))
                if a.len() == b.len() =>
            {
                for (a, b) in a.iter().zip(b) {
                    if !self.same_value(a, b, depth + 1)? {
                        return Ok(false);
                    }
                }
                true
            }
            (Value::Record(a), Value::Record(b)) if a.len() == b.len() => {
                for ((an, a), (bn, b)) in a.iter().zip(b) {
                    self.budget.text(an, Span::default())?;
                    self.budget.text(bn, Span::default())?;
                    if an != bn || !self.same_value(a, b, depth + 1)? {
                        return Ok(false);
                    }
                }
                true
            }
            (
                Value::Error {
                    is_error: a,
                    value: av,
                },
                Value::Error {
                    is_error: b,
                    value: bv,
                },
            ) => a == b && self.same_value(av, bv, depth + 1)?,
            (
                Value::Callable {
                    definition: a,
                    scope: ac,
                    kind: ak,
                    signature: asig,
                    suspend: ae,
                    known_suspend: ae_known,
                    shutdown: ash,
                    cleanup: ad,
                    initial: ai,
                    unknown_write: aw,
                    after: aa,
                    ready: ar,
                },
                Value::Callable {
                    definition: b,
                    scope: bc,
                    kind: bk,
                    signature: bsig,
                    suspend: be,
                    known_suspend: be_known,
                    shutdown: bsh,
                    cleanup: bd,
                    initial: bi,
                    unknown_write: bw,
                    after: ba,
                    ready: br,
                },
            ) => {
                a == b
                    && ac == bc
                    && ak == bk
                    && std::ptr::eq(*asig, *bsig)
                    && ae == be
                    && ae_known == be_known
                    && ash == bsh
                    && ad == bd
                    && ai == bi
                    && aw == bw
                    && aa == ba
                    && ar == br
            }
            _ => false,
        })
    }
    fn same_env(&mut self, a: &Env<'a>, b: &Env<'a>, depth: usize) -> R<bool> {
        if a.len() != b.len() {
            return Ok(false);
        }
        for ((an, a), (bn, b)) in a.iter().zip(b) {
            self.budget.text(an, Span::default())?;
            self.budget.text(bn, Span::default())?;
            if an != bn || !self.same_value(a, b, depth + 1)? {
                return Ok(false);
            }
        }
        Ok(true)
    }
    fn alternatives(&mut self, values: Vec<Value<'a>>, depth: usize) -> R<Value<'a>> {
        let mut pending = values;
        let mut result = Vec::new();
        while let Some(value) = pending.pop() {
            self.budget.node(Span::default(), depth)?;
            if let Value::Alternatives(values) = value {
                for value in values {
                    self.budget
                        .lineage_push(&mut pending, value, Span::default())?;
                }
            } else {
                let mut duplicate = false;
                for old in &result {
                    duplicate |= self.same_value(old, &value, depth + 1)?;
                }
                if !duplicate {
                    self.budget
                        .lineage_push(&mut result, value, Span::default())?;
                }
            }
        }
        result.reverse();
        Ok(if result.len() == 1 {
            result.pop().unwrap()
        } else {
            Value::Alternatives(result)
        })
    }
    fn ok_value(&mut self, value: Value<'a>, depth: usize) -> R<Value<'a>> {
        self.budget.node(Span::default(), depth)?;
        Ok(match value {
            Value::Error {
                is_error: false,
                value,
            } => *value,
            Value::Error { is_error: true, .. } => Value::Unknown,
            Value::Alternatives(values) => {
                let mut good = Vec::new();
                for value in values {
                    if let Value::Error {
                        is_error: false,
                        value,
                    } = value
                    {
                        self.budget
                            .lineage_push(&mut good, *value, Span::default())?;
                    }
                }
                if good.is_empty() {
                    Value::Unknown
                } else {
                    self.alternatives(good, depth + 1)?
                }
            }
            value => value,
        })
    }
    fn select(
        &mut self,
        pattern: &SemanticOriginalPattern,
        value: Value<'a>,
        depth: usize,
    ) -> R<Vec<Value<'a>>> {
        self.budget.node(pattern.span, depth)?;
        let mut selected = Vec::new();
        if let Value::Alternatives(values) = value {
            for value in values {
                for value in self.select(pattern, value, depth + 1)? {
                    self.budget
                        .lineage_push(&mut selected, value, pattern.span)?;
                }
            }
        } else if !matches!((&pattern.kind,&value),(CorePatternKind::ErrorArm{is_error:a,..},Value::Error{is_error:b,..}) if a!=b)
        {
            self.budget
                .lineage_push(&mut selected, value, pattern.span)?;
        }
        Ok(selected)
    }
    fn pending(&mut self, value: &Value<'a>, depth: usize) -> R<Cleanup> {
        self.budget.node(Span::default(), depth)?;
        Ok(match value {
            Value::Task | Value::UnknownAsync => Cleanup::ASYNC,
            Value::Unknown => Cleanup::ABSTRACT,
            Value::Error { value, .. } => self.pending(value, depth + 1)?,
            Value::Tuple(fields) | Value::Alternatives(fields) => {
                let mut pending = Cleanup::NONE;
                for field in fields {
                    pending |= self.pending(field, depth + 1)?;
                }
                pending
            }
            Value::Record(fields) => {
                let mut pending = Cleanup::NONE;
                for (_, field) in fields {
                    pending |= self.pending(field, depth + 1)?;
                }
                pending
            }
            // Calling FnOnce may settle its remaining environment; merely
            // owning/ignoring any callable never establishes SyncDrop.
            Value::Callable { cleanup, .. } => *cleanup,
            Value::Plain
            | Value::SyncTask
            | Value::SettledTask
            | Value::View { .. }
            | Value::Poll(_)
            | Value::Function(_) => Cleanup::NONE,
        })
    }
    fn unknown_write_cleanup(&mut self, value: &Value<'a>, depth: usize) -> R<Cleanup> {
        self.budget.node(Span::default(), depth)?;
        Ok(match value {
            Value::Task | Value::SyncTask | Value::SettledTask | Value::UnknownAsync => {
                Cleanup::ASYNC
            }
            Value::Unknown => Cleanup::ABSTRACT,
            Value::Callable { unknown_write, .. } => *unknown_write,
            Value::Tuple(values) | Value::Alternatives(values) => {
                let mut result = Cleanup::NONE;
                for value in values {
                    result |= self.unknown_write_cleanup(value, depth + 1)?;
                }
                result
            }
            Value::Record(values) => {
                let mut result = Cleanup::NONE;
                for (_, value) in values {
                    result |= self.unknown_write_cleanup(value, depth + 1)?;
                }
                result
            }
            Value::Error { value, .. } => self.unknown_write_cleanup(value, depth + 1)?,
            Value::Plain | Value::View { .. } | Value::Poll(_) | Value::Function(_) => {
                Cleanup::NONE
            }
        })
    }
    fn residual(&mut self, env: &[(&'a str, Value<'a>)], depth: usize) -> R<Cleanup> {
        let mut result = Cleanup::NONE;
        for (_, value) in env {
            result |= self.pending(value, depth + 1)?;
        }
        Ok(result)
    }
    fn join(&mut self, left: Value<'a>, right: Value<'a>, depth: usize) -> R<Value<'a>> {
        self.budget.node(Span::default(), depth)?;
        Ok(match (left, right) {
            (Value::Plain, Value::Plain) => Value::Plain,
            (Value::Task, Value::Task) => Value::Task,
            (Value::SyncTask, Value::SyncTask) => Value::SyncTask,
            (Value::Task, Value::SyncTask) | (Value::SyncTask, Value::Task) => Value::Task,
            (Value::SettledTask, Value::SyncTask) | (Value::SyncTask, Value::SettledTask) => {
                Value::SyncTask
            }
            (Value::SettledTask, Value::SettledTask) => Value::SettledTask,
            (Value::Task, Value::SettledTask) | (Value::SettledTask, Value::Task) => Value::Task,
            (
                Value::Error {
                    is_error: a,
                    value: av,
                },
                Value::Error {
                    is_error: b,
                    value: bv,
                },
            ) if a == b => {
                let value = self.join(*av, *bv, depth + 1)?;
                self.budget
                    .lineage_bytes(std::mem::size_of::<Value<'a>>(), Span::default())?;
                Value::Error {
                    is_error: a,
                    value: Box::new(value),
                }
            }
            (
                Value::View {
                    place: a,
                    mutable: am,
                },
                Value::View {
                    place: b,
                    mutable: bm,
                },
            ) if am == bm && self.same_place(&a, &b, depth + 1)? => Value::View {
                place: a,
                mutable: am,
            },
            (Value::Poll(a), Value::Poll(b)) if self.same_place(&a, &b, depth + 1)? => {
                Value::Poll(a)
            }
            (Value::Tuple(left), Value::Tuple(right)) if left.len() == right.len() => {
                let mut result = Vec::new();
                for (left, right) in left.into_iter().zip(right) {
                    let value = self.join(left, right, depth + 1)?;
                    self.budget
                        .lineage_push(&mut result, value, Span::default())?;
                }
                Value::Tuple(result)
            }
            (Value::Record(left), Value::Record(right)) if left.len() == right.len() => {
                let mut result = Vec::new();
                for ((name, left), (other, right)) in left.into_iter().zip(right) {
                    self.budget.text(name, Span::default())?;
                    self.budget.text(other, Span::default())?;
                    if name != other {
                        return Ok(Value::Unknown);
                    }
                    let value = self.join(left, right, depth + 1)?;
                    self.budget
                        .lineage_push(&mut result, (name, value), Span::default())?;
                }
                Value::Record(result)
            }
            (
                Value::Callable {
                    definition: a,
                    scope,
                    kind,
                    signature,
                    suspend: left,
                    known_suspend: left_known,
                    shutdown: left_shutdown,
                    cleanup: left_cleanup,
                    initial: left_initial,
                    unknown_write: left_write,
                    after: left_after,
                    ready: left_ready,
                },
                Value::Callable {
                    definition: b,
                    scope: other_scope,
                    kind: other,
                    signature: other_signature,
                    suspend: right,
                    known_suspend: right_known,
                    shutdown: right_shutdown,
                    cleanup: right_cleanup,
                    initial: right_initial,
                    unknown_write: right_write,
                    after: right_after,
                    ready: right_ready,
                },
            ) if a == b
                && scope == other_scope
                && kind == other
                && std::ptr::eq(signature, other_signature) =>
            {
                Value::Callable {
                    definition: a,
                    scope,
                    kind,
                    signature,
                    suspend: left || right,
                    known_suspend: left_known || right_known,
                    shutdown: left_shutdown && right_shutdown,
                    cleanup: left_cleanup | right_cleanup,
                    initial: left_initial | right_initial,
                    unknown_write: left_write | right_write,
                    after: left_after | right_after,
                    ready: left_ready.zip(right_ready).map(|(a, b)| a | b),
                }
            }
            (
                left @ (Value::Callable { .. } | Value::Alternatives(_) | Value::Error { .. }),
                right @ (Value::Callable { .. } | Value::Alternatives(_) | Value::Error { .. }),
            ) => {
                // Preserve each original creator, not a common actual image.
                // Only the operation upper is joined, never its types/classes.
                let mut alternatives = Vec::new();
                self.budget
                    .lineage_push(&mut alternatives, left, Span::default())?;
                self.budget
                    .lineage_push(&mut alternatives, right, Span::default())?;
                self.alternatives(alternatives, depth + 1)?
            }
            (left, right) => {
                if self.pending(&left, depth + 1)?.asynchronous
                    || self.pending(&right, depth + 1)?.asynchronous
                {
                    Value::UnknownAsync
                } else {
                    Value::Unknown
                }
            }
        })
    }
    fn merge_env(&mut self, env: &mut Env<'a>, other: Env<'a>, depth: usize) -> R<()> {
        if env.len() != other.len() {
            return Err(error(
                Span::default(),
                "cleanup candidate lexical branch differs",
            ));
        }
        for ((name, value), (other_name, other)) in env.iter_mut().zip(other) {
            self.budget.text(name, Span::default())?;
            self.budget.text(other_name, Span::default())?;
            if *name != other_name {
                return Err(error(
                    Span::default(),
                    "cleanup candidate shadow scope differs",
                ));
            }
            *value = self.join(std::mem::replace(value, Value::Plain), other, depth + 1)?;
        }
        Ok(())
    }
    fn lookup(&mut self, env: &Env<'a>, name: &str, depth: usize) -> R<Option<Value<'a>>> {
        for (binding, value) in env.iter().rev() {
            self.budget.node(Span::default(), depth)?;
            self.budget.text(name, Span::default())?;
            if *binding == name {
                return self.copy(value, depth + 1).map(Some);
            }
        }
        Ok(None)
    }
    fn take(
        &mut self,
        owner: usize,
        env: &mut Env<'a>,
        expression: &'a SemanticOriginalExpr,
        depth: usize,
    ) -> R<Value<'a>> {
        self.budget.node(expression.span, depth)?;
        if let Some(place) = self.place(owner, env, expression, depth + 1)? {
            self.invalidate(env, &place, depth + 1)?;
        }
        match &expression.kind {
            CoreExprKind::Group(inner) | CoreExprKind::Ascribed { value: inner, .. } => {
                self.take(owner, env, inner, depth + 1)
            }
            CoreExprKind::Name(name) => {
                for (binding, value) in env.iter_mut().rev() {
                    self.budget.text(name, expression.span)?;
                    if *binding == name {
                        return Ok(std::mem::replace(value, Value::Plain));
                    }
                }
                Ok(Value::Unknown)
            }
            CoreExprKind::Member { .. } => {
                let mut path = Vec::new();
                let mut root = expression;
                loop {
                    self.budget.node(root.span, depth + path.len())?;
                    match &root.kind {
                        CoreExprKind::Group(inner)
                        | CoreExprKind::Ascribed { value: inner, .. } => root = inner,
                        CoreExprKind::Member { object, member, .. } => {
                            self.budget
                                .lineage_push(&mut path, member.as_str(), root.span)?;
                            root = object;
                        }
                        _ => break,
                    }
                }
                let result = if let CoreExprKind::Name(name) = &root.kind {
                    path.reverse();
                    let mut result = Value::Unknown;
                    for (binding, value) in env.iter_mut().rev() {
                        self.budget.text(name, root.span)?;
                        if *binding == name {
                            result = self.take_path(value, &path, depth + 1)?;
                            break;
                        }
                    }
                    result
                } else {
                    Value::Unknown
                };
                self.budget.drop_lineage_vec(path, expression.span)?;
                Ok(result)
            }
            // An unresolved projected owner is never erased wholesale.
            _ => Ok(Value::Unknown),
        }
    }
    fn take_path(&mut self, value: &mut Value<'a>, path: &[&str], depth: usize) -> R<Value<'a>> {
        self.budget.node(Span::default(), depth)?;
        let Some((first, rest)) = path.split_first() else {
            return Ok(std::mem::replace(value, Value::Plain));
        };
        match value {
            Value::Tuple(fields) => {
                let Some(field) = first
                    .parse::<usize>()
                    .ok()
                    .and_then(|index| fields.get_mut(index))
                else {
                    return Ok(Value::Unknown);
                };
                self.take_path(field, rest, depth + 1)
            }
            Value::Record(fields) => {
                for (name, field) in fields {
                    self.budget.text(name, Span::default())?;
                    if name == first {
                        return self.take_path(field, rest, depth + 1);
                    }
                }
                Ok(Value::Unknown)
            }
            _ => Ok(Value::Unknown),
        }
    }
    fn transfer(
        &mut self,
        owner: usize,
        env: &mut Env<'a>,
        expression: &'a SemanticOriginalExpr,
        value: &Value<'a>,
        depth: usize,
    ) -> R<()> {
        self.budget.node(expression.span, depth)?;
        if matches!(
            value,
            Value::Callable { .. }
                | Value::Tuple(_)
                | Value::Record(_)
                | Value::Alternatives(_)
                | Value::Unknown
                | Value::UnknownAsync
        ) {
            self.take(owner, env, expression, depth + 1)?;
        }
        Ok(())
    }
    fn bound(
        &mut self,
        owner: usize,
        requirements: &'a [SemanticRequirementRecipe],
        ty: &SemanticTypeRecipe,
        depth: usize,
    ) -> R<Option<Value<'a>>> {
        let mut result = None;
        for requirement in requirements {
            self.budget.node(Span::default(), depth)?;
            prepay_recipe_work(requirement, self.budget, Span::default(), depth)?;
            let candidate = match requirement {
                SemanticRequirementRecipe::Callable {
                    ty: subject,
                    kind,
                    signature,
                } if subject == ty => Some(Value::Callable {
                    definition: None,
                    scope: owner,
                    kind: *kind,
                    signature,
                    suspend: signature.call_kind == CallKind::Suspend,
                    known_suspend: self.evidence.fixed(signature, self.budget)?,
                    shutdown: false,
                    cleanup: Cleanup::ABSTRACT,
                    initial: Cleanup::ABSTRACT,
                    unknown_write: Cleanup::ABSTRACT,
                    after: Cleanup::ABSTRACT,
                    ready: None,
                }),
                SemanticRequirementRecipe::Structural {
                    ty: subject,
                    bound: DynStructuralBound::Copy | DynStructuralBound::SyncDrop,
                } if subject == ty => Some(Value::Plain),
                SemanticRequirementRecipe::Builtin {
                    ty: subject,
                    trait_kind:
                        SemanticBuiltinTrait::Copy
                        | SemanticBuiltinTrait::SyncDrop
                        | SemanticBuiltinTrait::StoreElement,
                } if subject == ty => Some(Value::Plain),
                SemanticRequirementRecipe::ForAll { requirements, .. } => {
                    self.bound(owner, requirements, ty, depth + 1)?
                }
                _ => None,
            };
            if let Some(candidate) = candidate {
                result = Some(match result {
                    None => candidate,
                    Some(old) => self.premise_join(old, candidate, depth + 1)?,
                });
            }
        }
        Ok(result)
    }
    fn premise_join(&mut self, left: Value<'a>, right: Value<'a>, depth: usize) -> R<Value<'a>> {
        // Plain here can only be the exact subject's original Copy/SyncDrop
        // premise, never the absence of an occupant on one control-flow arm.
        Ok(match (left, right) {
            (
                Value::Plain,
                Value::Callable {
                    definition,
                    scope,
                    kind,
                    signature,
                    suspend,
                    known_suspend,
                    shutdown,
                    ..
                },
            )
            | (
                Value::Callable {
                    definition,
                    scope,
                    kind,
                    signature,
                    suspend,
                    known_suspend,
                    shutdown,
                    ..
                },
                Value::Plain,
            ) => Value::Callable {
                definition,
                scope,
                kind,
                signature,
                suspend,
                known_suspend,
                shutdown,
                cleanup: Cleanup::NONE,
                initial: Cleanup::NONE,
                unknown_write: Cleanup::NONE,
                after: Cleanup::NONE,
                ready: None,
            },
            (left, right) => self.join(left, right, depth + 1)?,
        })
    }
    fn ty(&mut self, owner: usize, ty: &'a SemanticTypeRecipe, depth: usize) -> R<Value<'a>> {
        self.budget.node(Span::default(), depth)?;
        Ok(match ty {
            SemanticTypeRecipe::Task { .. } | SemanticTypeRecipe::AsyncRegistration(_) => {
                Value::Task
            }
            SemanticTypeRecipe::Managed(inner)
            | SemanticTypeRecipe::Qualified { storage: inner, .. } => {
                self.ty(owner, inner, depth + 1)?
            }
            SemanticTypeRecipe::Address { .. }
            | SemanticTypeRecipe::Desc { .. }
            | SemanticTypeRecipe::Atom(_) => Value::Plain,
            SemanticTypeRecipe::Code { signature, .. } => Value::Callable {
                definition: None,
                scope: owner,
                kind: CallableKind::Fn,
                signature,
                suspend: signature.call_kind == CallKind::Suspend,
                known_suspend: signature.call_kind == CallKind::Suspend,
                shutdown: false,
                cleanup: Cleanup::NONE,
                initial: Cleanup::NONE,
                unknown_write: Cleanup::NONE,
                after: Cleanup::NONE,
                ready: None,
            },
            SemanticTypeRecipe::Tuple(fields) => {
                let mut result = Vec::new();
                for field in fields {
                    let value = self.ty(owner, field, depth + 1)?;
                    self.budget
                        .lineage_push(&mut result, value, Span::default())?;
                }
                Value::Tuple(result)
            }
            SemanticTypeRecipe::ErrorUnion { error, result } => {
                let mut values = Vec::new();
                for (is_error, ty) in [(true, error.as_ref()), (false, result.as_ref())] {
                    let value = self.ty(owner, ty, depth + 1)?;
                    self.budget
                        .lineage_bytes(std::mem::size_of::<Value<'a>>(), Span::default())?;
                    self.budget.lineage_push(
                        &mut values,
                        Value::Error {
                            is_error,
                            value: Box::new(value),
                        },
                        Span::default(),
                    )?;
                }
                Value::Alternatives(values)
            }
            SemanticTypeRecipe::Slice(_) => Value::Plain,
            SemanticTypeRecipe::Array { element, .. } => {
                let element = self.ty(owner, element, depth + 1)?;
                if self.pending(&element, depth + 1)?.asynchronous {
                    Value::UnknownAsync
                } else if self.pending(&element, depth + 1)?.may {
                    Value::Unknown
                } else {
                    Value::Plain
                }
            }
            SemanticTypeRecipe::Formal(reference) => {
                let SemanticScopeRef::Ancestor(distance) = &reference.scope;
                let mut declaration = owner;
                for _ in 0..*distance {
                    self.budget.node(Span::default(), depth + 1)?;
                    let Some(parent) = self.definitions[declaration].parent else {
                        return Ok(Value::Unknown);
                    };
                    let Some(parent) =
                        declaration_index(self.definitions, parent, self.budget, Span::default())?
                    else {
                        return Ok(Value::Unknown);
                    };
                    declaration = parent;
                }
                let mut context = self.context;
                while let Some(index) = context {
                    self.budget.node(Span::default(), depth + 1)?;
                    let frame = &self.frames[index];
                    context = frame.context;
                    if frame.declaration == declaration {
                        let caller = frame.caller;
                        let arguments = frame.arguments;
                        let saved = std::mem::replace(&mut self.context, context);
                        let result = match arguments.get(reference.ordinal as usize) {
                            Some(SemanticActualRecipe::Type(ty)) => self.ty(caller, ty, depth + 1),
                            _ => Ok(Value::Unknown),
                        };
                        self.context = saved;
                        return result;
                    }
                }
                let definition = self.definitions[declaration];
                let local = SemanticTypeRecipe::Formal(SemanticFormalRef {
                    scope: SemanticScopeRef::Ancestor(0),
                    ordinal: reference.ordinal,
                });
                let own = match definition.parameters.get(reference.ordinal as usize) {
                    Some(SemanticFormal::Type { requirements }) => {
                        self.bound(declaration, requirements, &local, depth + 1)?
                    }
                    _ => None,
                };
                match (
                    own,
                    self.bound(declaration, &definition.requirements, &local, depth + 1)?,
                ) {
                    (Some(value), None) | (None, Some(value)) => value,
                    (Some(left), Some(right)) => self.premise_join(left, right, depth + 1)?,
                    _ => Value::Unknown,
                }
            }
            SemanticTypeRecipe::Nominal {
                definition,
                arguments,
            } => {
                let Some(index) =
                    declaration_index(self.definitions, *definition, self.budget, Span::default())?
                else {
                    return Ok(Value::Unknown);
                };
                if self.definitions[index].parameters.len() != arguments.len() {
                    return Ok(Value::Unknown);
                }
                let context = self.context;
                let frame = self.frames.len();
                self.budget.lineage_push(
                    &mut self.frames,
                    TypeFrame {
                        declaration: index,
                        caller: owner,
                        arguments,
                        context,
                    },
                    Span::default(),
                )?;
                self.context = Some(frame);
                let value = match &self.definitions[index].body {
                    SemanticDefinitionBody::Nominal {
                        body: SemanticNominalBody::Product { fields },
                        ..
                    } => {
                        let mut result = Vec::new();
                        for field in fields {
                            let value = self.ty(index, &field.ty, depth + 1)?;
                            self.budget.lineage_push(
                                &mut result,
                                (field.name.as_str(), value),
                                Span::default(),
                            )?;
                        }
                        Value::Record(result)
                    }
                    SemanticDefinitionBody::Nominal {
                        body: SemanticNominalBody::Enum { variants },
                        ..
                    } => {
                        let mut pending = Cleanup::NONE;
                        for variant in variants {
                            for field in &variant.fields {
                                let value = self.ty(index, field, depth + 1)?;
                                pending |= self.pending(&value, depth + 1)?;
                            }
                        }
                        if pending.asynchronous {
                            Value::UnknownAsync
                        } else if pending.may {
                            Value::Unknown
                        } else {
                            Value::Plain
                        }
                    }
                    _ => Value::Unknown,
                };
                self.frames.pop();
                self.context = context;
                value
            }
            SemanticTypeRecipe::Opaque { definition, .. } => {
                if let Some(index) =
                    declaration_index(self.definitions, *definition, self.budget, Span::default())?
                {
                    self.function_result(index, depth + 1)?
                } else {
                    Value::Unknown
                }
            }
            _ => Value::Unknown,
        })
    }
    fn target(
        &mut self,
        owner: usize,
        env: &Env<'a>,
        expression: &SemanticOriginalExpr,
        depth: usize,
    ) -> R<Option<usize>> {
        self.budget.node(expression.span, depth)?;
        let mut path = Vec::new();
        let mut current = expression;
        loop {
            match &current.kind {
                CoreExprKind::Group(inner)
                | CoreExprKind::TypeApply { callee: inner, .. }
                | CoreExprKind::GenericApply { callee: inner, .. } => current = inner,
                CoreExprKind::Member { object, member, .. } => {
                    self.budget
                        .lineage_push(&mut path, member.as_str(), expression.span)?;
                    current = object;
                }
                CoreExprKind::Name(name) => {
                    if self.lookup(env, name, depth + 1)?.is_some() {
                        return Ok(None);
                    }
                    self.budget
                        .lineage_push(&mut path, name.as_str(), expression.span)?;
                    break;
                }
                _ => return Ok(None),
            }
        }
        path.reverse();
        let SemanticDefinitionBody::FunctionWithBody { names, .. } = &self.definitions[owner].body
        else {
            return Ok(None);
        };
        let mut found = None;
        for binding in names {
            self.budget
                .identity_work(path.len().saturating_add(1), expression.span)?;
            if binding.namespace == SemanticOriginalNamespace::Value
                && binding
                    .path
                    .iter()
                    .map(String::as_str)
                    .eq(path.iter().copied())
            {
                let SemanticOriginalNameValue::Definition(id) = &binding.value else {
                    continue;
                };
                let index = declaration_index(self.definitions, *id, self.budget, expression.span)?;
                if found.is_some() && found != index {
                    return Ok(None);
                }
                found = index;
            }
        }
        self.budget.drop_lineage_vec(path, expression.span)?;
        Ok(found)
    }
    fn operation(
        &mut self,
        value: &Value<'a>,
        depth: usize,
    ) -> R<Option<(bool, bool, Cleanup, Cleanup, Option<Cleanup>)>> {
        self.budget.node(Span::default(), depth)?;
        Ok(match value {
            Value::Callable {
                suspend,
                known_suspend,
                cleanup,
                after,
                ready,
                ..
            } => Some((*suspend, *known_suspend, *cleanup, *after, *ready)),
            Value::Function(index) => signature(self.definitions[*index]).map(|signature| {
                (
                    signature.call_kind == CallKind::Suspend,
                    self.evidence.rows[*index].function,
                    Cleanup::NONE,
                    Cleanup::NONE,
                    None,
                )
            }),
            Value::Alternatives(values) => {
                let mut suspend = false;
                let mut known = false;
                let mut cleanup = Cleanup::NONE;
                let mut after = Cleanup::NONE;
                let mut ready = Some(Cleanup::NONE);
                if values.is_empty() {
                    return Ok(None);
                }
                for value in values {
                    let Some((next, next_known, remaining, next_after, next_ready)) =
                        self.operation(value, depth + 1)?
                    else {
                        return Ok(None);
                    };
                    suspend |= next;
                    known |= next_known;
                    cleanup |= remaining;
                    after |= next_after;
                    ready = ready.zip(next_ready).map(|(a, b)| a | b);
                }
                Some((suspend, known, cleanup, after, ready))
            }
            _ => None,
        })
    }
    fn function_flow(&mut self, owner: usize, depth: usize) -> R<Flow<'a>> {
        self.function_flow_inputs(owner, None, depth)
    }
    fn function_flow_inputs(
        &mut self,
        owner: usize,
        inputs: Option<&[Value<'a>]>,
        depth: usize,
    ) -> R<Flow<'a>> {
        self.budget.node(Span::default(), depth)?;
        self.budget
            .identity_work(self.active.len(), Span::default())?;
        if self.active.contains(&owner) {
            let mut flow = Flow::new();
            flow.incomplete = true;
            return Ok(flow);
        }
        let definition = self.definitions[owner];
        let SemanticDefinitionBody::FunctionWithBody {
            signature,
            parameters,
            body: SemanticOriginalFunctionBody::Defined(body),
            ..
        } = &definition.body
        else {
            let mut flow = Flow::new();
            flow.incomplete = true;
            return Ok(flow);
        };
        if inputs.is_some_and(|values| values.len() != parameters.len()) {
            let mut flow = Flow::new();
            flow.incomplete = true;
            return Ok(flow);
        }
        self.budget
            .lineage_push(&mut self.active, owner, Span::default())?;
        let caller_cache = self.last_plain_call.take();
        let publish = self.publish;
        if inputs.is_some() {
            self.publish = false;
        }
        let shutdown = std::mem::replace(&mut self.shutdown, true);
        // Publication always uses the declaration Gamma. A separate replay
        // of the same original return/capture operations may transport actual
        // birth candidates, but its publish flag is sealed off above.
        let frames = self.frames.len();
        let context = self.context.take();
        let execution = std::mem::replace(&mut self.execution, false);
        let known_execution = std::mem::replace(&mut self.known_execution, false);
        let held = self.held_captures;
        let early = std::mem::replace(&mut self.early_cleanup, Cleanup::NONE);
        let early_body = std::mem::replace(&mut self.early_body_cleanup, Cleanup::NONE);
        let operands = self.enter_operand_frame();
        let mut env = Vec::new();
        for slot in &definition.environment {
            if let SemanticActualRecipe::Type(
                ty @ SemanticTypeRecipe::OriginalType {
                    source: SemanticOriginalTypeSource::NameAt { name, .. },
                    ..
                },
            ) = &slot.source
            {
                let value = self.ty(owner, ty, depth + 1)?;
                self.budget
                    .lineage_push(&mut env, (name.as_str(), value), Span::default())?;
            }
        }
        self.held_captures = env.len();
        for (ordinal, (parameter, ty)) in parameters.iter().zip(&signature.parameters).enumerate() {
            let value = if let Some(inputs) = inputs {
                self.copy(&inputs[ordinal], depth + 1)?
            } else {
                self.ty(owner, &ty.ty, depth + 1)?
            };
            let cleanup = self.pending(&value, depth + 1)?;
            self.shutdown &= !cleanup.may;
            self.budget
                .lineage_push(&mut env, (parameter.name.as_str(), value), parameter.span)?;
        }
        let mut flow = self.block(owner, &mut env, body, depth + 1)?;
        flow.cleanup |= self.early_cleanup;
        flow.body_cleanup |= self.early_body_cleanup;
        if flow.live {
            flow.cleanup |= self.residual(&env, depth + 1)?;
            flow.body_cleanup |= self.residual(&env[self.held_captures..], depth + 1)?;
        }
        flow.incomplete |= !flow.breaks.is_empty() || !flow.continues.is_empty();
        flow.shutdown = self.shutdown;
        if self.publish && flow.body_cleanup.asynchronous {
            self.budget.node(Span::default(), depth)?;
            self.bodies[owner] = true;
        }
        self.active.pop();
        self.last_plain_call = caller_cache;
        self.publish = publish;
        self.shutdown = shutdown;
        self.frames.truncate(frames);
        self.context = context;
        self.execution = execution;
        self.known_execution = known_execution;
        self.held_captures = held;
        self.early_cleanup = early;
        self.early_body_cleanup = early_body;
        self.leave_operand_frame(operands);
        Ok(flow)
    }
    fn function_result(&mut self, owner: usize, depth: usize) -> R<Value<'a>> {
        self.budget.node(Span::default(), depth)?;
        let definition = self.definitions[owner];
        self.budget
            .identity_work(self.active.len(), Span::default())?;
        if self.active.contains(&owner) {
            return match signature(definition) {
                Some(signature)
                    if !matches!(signature.result, SemanticTypeRecipe::Opaque { .. }) =>
                {
                    self.ty(owner, &signature.result, depth + 1)
                }
                _ => Ok(Value::Unknown),
            };
        }
        let mut bounds = Vec::new();
        opaque_bounds(
            &definition.requirements,
            &mut bounds,
            self.budget,
            depth + 1,
        )?;
        let selected = match bounds.as_slice() {
            [(kind, bound)] => Some((*kind, *bound)),
            _ => None,
        };
        self.budget.drop_lineage_vec(bounds, Span::default())?;
        let Some((kind, bound)) = selected else {
            return match signature(definition) {
                Some(signature)
                    if !matches!(signature.result, SemanticTypeRecipe::Opaque { .. }) =>
                {
                    self.ty(owner, &signature.result, depth + 1)
                }
                _ => Ok(Value::Unknown),
            };
        };
        if !matches!(
            definition.body,
            SemanticDefinitionBody::FunctionWithBody {
                body: SemanticOriginalFunctionBody::Defined(_),
                ..
            }
        ) {
            return Ok(Value::Callable {
                definition: Some(owner),
                scope: owner,
                kind,
                signature: bound,
                suspend: bound.call_kind == CallKind::Suspend,
                known_suspend: self.evidence.fixed(bound, self.budget)?,
                shutdown: false,
                cleanup: Cleanup::ABSTRACT,
                initial: Cleanup::ABSTRACT,
                unknown_write: Cleanup::ABSTRACT,
                after: Cleanup::ABSTRACT,
                ready: None,
            });
        }
        let flow = self.function_flow(owner, depth + 1)?;
        let unknown_write = match &flow.value {
            Some(value) => self.unknown_write_cleanup(value, depth + 1)?,
            None => Cleanup::ABSTRACT,
        };
        let (suspend, known, cleanup, after, ready) = if !flow.live && !flow.incomplete {
            match flow.value {
                Some(value) => self.operation(&value, depth + 1)?.unwrap_or((
                    true,
                    false,
                    Cleanup::ABSTRACT,
                    Cleanup::ABSTRACT,
                    None,
                )),
                None => (true, false, Cleanup::ABSTRACT, Cleanup::ABSTRACT, None),
            }
        } else {
            (true, false, Cleanup::ABSTRACT, Cleanup::ABSTRACT, None)
        };
        // Repeated invocation uses its declared entry contract. Remaining
        // cleanup is separate, and a real Ready relation refines only this
        // particular stored callback's later destruction candidate.
        let suspend = if kind == CallableKind::FnOnce {
            suspend || bound.call_kind == CallKind::Suspend
        } else {
            bound.call_kind == CallKind::Suspend
        };
        let known = if kind == CallableKind::FnOnce {
            known || self.evidence.fixed(bound, self.budget)?
        } else {
            self.evidence.fixed(bound, self.budget)?
        };
        if self.publish
            && kind == CallableKind::FnOnce
            && known
            && !self.evidence.rows[owner].operation
        {
            self.evidence.rows[owner].operation = true;
            *self.seed_changed = true;
        }
        Ok(Value::Callable {
            definition: Some(owner),
            scope: owner,
            kind,
            signature: bound,
            suspend,
            known_suspend: known,
            shutdown: false,
            cleanup,
            initial: cleanup,
            unknown_write,
            after,
            ready,
        })
    }
    fn call_result(&mut self, owner: usize, inputs: &[Value<'a>], depth: usize) -> R<Value<'a>> {
        self.budget.node(Span::default(), depth)?;
        let declared = self.function_result(owner, depth + 1)?;
        let definition = self.definitions[owner];
        if !matches!(
            definition.body,
            SemanticDefinitionBody::FunctionWithBody {
                body: SemanticOriginalFunctionBody::Defined(_),
                ..
            }
        ) {
            let leaf = self.scalar_foreign(definition)?;
            self.shutdown &= leaf;
            return Ok(declared);
        }
        // The replay runs with fresh operand and execution state. Within one
        // nonpublishing caller, identical plain inputs carry no place or owner
        // identity. Only a complete plain return and its shutdown bit qualify.
        let plain = !self.publish && matches!(declared, Value::Plain)
            && definition.environment.is_empty();
        if plain {
            self.budget.identity_work(inputs.len(), Span::default())?;
        }
        let plain = plain && inputs.iter().all(|value| matches!(value, Value::Plain));
        if plain && let Some(previous) = &self.last_plain_call
            && (previous.target, previous.inputs, previous.depth) == (owner, inputs.len(), depth)
        {
            self.shutdown &= previous.shutdown;
            return Ok(Value::Plain);
        }
        let flow = self.function_flow_inputs(owner, Some(inputs), depth + 1)?;
        self.shutdown &= flow.shutdown && !flow.incomplete;
        let Some(value) = flow.value.filter(|_| !flow.live && !flow.incomplete) else {
            return Ok(declared);
        };
        if plain && matches!(value, Value::Plain) {
            self.last_plain_call = Some(PlainCall {
                target: owner,
                inputs: inputs.len(),
                depth,
                shutdown: flow.shutdown,
            });
        }
        if let Value::Callable {
            definition,
            scope,
            kind,
            signature,
            suspend,
            known_suspend,
            unknown_write,
            ..
        } = declared
        {
            let Some((_, _, cleanup, after, ready)) = self.operation(&value, depth + 1)? else {
                return Ok(Value::Unknown);
            };
            let shutdown = self.callable_shutdown(&value, depth + 1)?;
            let unknown_write = unknown_write | self.unknown_write_cleanup(&value, depth + 1)?;
            Ok(Value::Callable {
                definition,
                scope,
                kind,
                signature,
                suspend,
                known_suspend,
                shutdown,
                cleanup,
                initial: cleanup,
                unknown_write,
                after,
                ready,
            })
        } else {
            Ok(value)
        }
    }
    fn callable_shutdown(&mut self, value: &Value<'a>, depth: usize) -> R<bool> {
        self.budget.node(Span::default(), depth)?;
        match value {
            Value::Callable { shutdown, .. } => Ok(*shutdown),
            Value::Alternatives(values) if !values.is_empty() => {
                let mut complete = true;
                for value in values {
                    complete &= self.callable_shutdown(value, depth + 1)?;
                }
                Ok(complete)
            }
            _ => Ok(false),
        }
    }
    fn scalar_foreign(&mut self, definition: &SemanticDefinition) -> R<bool> {
        self.budget.node(Span::default(), 0)?;
        let SemanticDefinitionBody::FunctionWithBody {
            signature,
            foreign: Some(foreign),
            body: SemanticOriginalFunctionBody::External,
            ..
        } = &definition.body
        else {
            return Ok(false);
        };
        self.budget
            .identity_work(signature.parameters.len(), Span::default())?;
        self.budget.text(&foreign.abi, Span::default())?;
        let scalar = |ty: &SemanticTypeRecipe| {
            matches!(
                ty,
                SemanticTypeRecipe::Atom(
                    Type::Unit
                        | Type::Bool
                        | Type::I8
                        | Type::I16
                        | Type::I32
                        | Type::I64
                        | Type::U8
                        | Type::U16
                        | Type::U32
                        | Type::U64
                        | Type::Isize
                        | Type::Usize
                        | Type::F32
                        | Type::F64
                )
            )
        };
        Ok(signature.call_kind == CallKind::Direct
            && signature.bound_lifetimes == 0
            && signature.invocation_lifetimes == 0
            && foreign.abi == "C"
            && !foreign.symbol.is_empty()
            && scalar(&signature.result)
            && signature
                .parameters
                .iter()
                .all(|parameter| scalar(&parameter.ty)))
    }
    fn shutdown_input(&mut self, value: &Value<'a>, depth: usize) -> R<bool> {
        self.budget.node(Span::default(), depth)?;
        match value {
            Value::Plain | Value::SyncTask | Value::SettledTask => Ok(true),
            Value::Tuple(values) | Value::Alternatives(values) => {
                for value in values {
                    if !self.shutdown_input(value, depth + 1)? {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
            Value::Record(values) => {
                for (_, value) in values {
                    if !self.shutdown_input(value, depth + 1)? {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
            Value::Error { value, .. } => self.shutdown_input(value, depth + 1),
            // Copying a borrowed pointer, code or an abstract callable is not
            // an independently known complete shutdown graph for a new Task.
            _ => Ok(false),
        }
    }
    fn worker_shutdown(&mut self, owner: usize, inputs: &[Value<'a>], depth: usize) -> R<bool> {
        self.budget.node(Span::default(), depth)?;
        let definition = self.definitions[owner];
        let Some(signature) = signature(definition) else {
            return Ok(false);
        };
        if signature.parameters.len() != inputs.len() {
            return Ok(false);
        }
        for input in inputs {
            if !self.shutdown_input(input, depth + 1)? {
                return Ok(false);
            }
        }
        if self.scalar_foreign(definition)? {
            for input in inputs {
                if self.pending(input, depth + 1)?.may {
                    return Ok(false);
                }
            }
            return Ok(true);
        }
        let flow = self.function_flow_inputs(owner, Some(inputs), depth + 1)?;
        let returned = match &flow.value {
            Some(value) => self.pending(value, depth + 1)?,
            None => Cleanup::NONE,
        };
        Ok(flow.shutdown && !flow.incomplete && !flow.cleanup.may && !returned.may)
    }
    fn expr(
        &mut self,
        owner: usize,
        env: &mut Env<'a>,
        expression: &'a SemanticOriginalExpr,
        depth: usize,
    ) -> R<Value<'a>> {
        self.budget.node(expression.span, depth)?;
        use CoreExprKind as E;
        // A call within a binary operand may evaluate another original body.
        // Neither operation needs the storage used for aggregate construction.
        let value = match &expression.kind {
            E::Call { callee, arguments } => {
                match self.call_expr(owner, env, callee, arguments, expression.span, depth)? {
                    CallOutcome::Value(value) => value,
                    CallOutcome::Defined { owner, inputs } => {
                        self.call_result(owner, &inputs, depth + 1)?
                    }
                }
            }
            E::Closure { move_capture, .. } => {
                return self.closure_expr(owner, env, expression, *move_capture, depth);
            }
            E::Binary { left, right, .. }
            | E::Range {
                start: left,
                end: right,
            } => {
                self.expr(owner, env, left, depth + 1)?;
                self.expr(owner, env, right, depth + 1)?;
                Value::Plain
            }
            _ => self.operand_value(owner, env, expression, depth)?,
        };
        self.finish_expr(value, depth)
    }
    fn operand_value(
        &mut self,
        owner: usize,
        env: &mut Env<'a>,
        expression: &'a SemanticOriginalExpr,
        depth: usize,
    ) -> R<Value<'a>> {
        use CoreExprKind as E;
        let value = match &expression.kind {
            E::Name(name) => {
                if let Some(value) = self.lookup(env, name, depth + 1)? {
                    value
                } else {
                    self.target(owner, env, expression, depth + 1)?
                        .map_or(Value::Unknown, Value::Function)
                }
            }
            E::Group(value) | E::Ascribed { value, .. } | E::Spread(value) => {
                self.expr(owner, env, value, depth + 1)?
            }
            E::Try(value) => {
                let value = self.expr(owner, env, value, depth + 1)?;
                let all = self.residual(env, depth + 1)? | self.prepared;
                let body = self.residual(&env[self.held_captures..], depth + 1)? | self.prepared;
                self.early_cleanup |= all;
                self.early_body_cleanup |= body;
                self.ok_value(value, depth + 1)?
            }
            E::Unary {
                op: UnaryOp::Give,
                operand,
            } => self.take(owner, env, operand, depth + 1)?,
            E::Unary {
                op: op @ (UnaryOp::Borrow | UnaryOp::BorrowMut),
                operand,
            } => match self.borrowed_place(owner, env, operand, depth + 1)? {
                Some(place) => Value::View {
                    place,
                    mutable: *op == UnaryOp::BorrowMut,
                },
                None => Value::Plain,
            },
            E::Unary { operand, .. }
            | E::NumericConvert { value: operand, .. }
            | E::IntegerConvert { value: operand, .. } => {
                self.expr(owner, env, operand, depth + 1)?;
                Value::Plain
            }
            E::Tuple(fields) | E::Array(fields) => {
                let base = self.prepared_values.len();
                let prepared = self.prepared;
                for field in fields {
                    let value = self.expr(owner, env, field, depth + 1)?;
                    self.transfer(owner, env, field, &value, depth + 1)?;
                    self.park(value, true, field.span, depth + 1)?;
                }
                let values = self.unpark(base, expression.span)?;
                self.prepared = prepared;
                Value::Tuple(values)
            }
            E::Member { object, member, .. } => {
                let value = self.expr(owner, env, object, depth + 1)?;
                match value {
                    Value::Tuple(fields) => match member
                        .parse::<usize>()
                        .ok()
                        .and_then(|index| fields.get(index))
                    {
                        Some(field) => self.copy(field, depth + 1)?,
                        None => Value::Unknown,
                    },
                    Value::Record(fields) => {
                        self.budget.identity_work(fields.len(), expression.span)?;
                        match fields.iter().find(|(name, _)| *name == member) {
                            Some((_, value)) => self.copy(value, depth + 1)?,
                            None => Value::Unknown,
                        }
                    }
                    _ => Value::Unknown,
                }
            }
            E::Record { fields, .. } | E::RecordConstructor { fields, .. } => {
                let base = self.prepared_values.len();
                let prepared = self.prepared;
                for (_, field) in fields {
                    let value = self.expr(owner, env, field, depth + 1)?;
                    self.transfer(owner, env, field, &value, depth + 1)?;
                    self.park(value, true, field.span, depth + 1)?;
                }
                let mut values = copy_rows(fields.len(), self.budget)?;
                let operands = self.prepared_values.drain(base..);
                for ((name, field), value) in fields.iter().zip(operands) {
                    self.budget.node(field.span, 0)?;
                    values.push((name.as_str(), value));
                }
                self.prepared = prepared;
                Value::Record(values)
            }
            E::If {
                condition,
                then_value,
                else_value,
            } => {
                self.expr(owner, env, condition, depth + 1)?;
                let mut other = self.env_copy(env, depth + 1)?;
                let left = self.expr(owner, env, then_value, depth + 1)?;
                let right = self.expr(owner, &mut other, else_value, depth + 1)?;
                self.merge_env(env, other, depth + 1)?;
                self.join(left, right, depth + 1)?
            }
            E::Sequence {
                bindings,
                value: result,
            } => {
                let base = env.len();
                for (name, expression) in bindings {
                    let value = self.expr(owner, env, expression, depth + 1)?;
                    self.transfer(owner, env, expression, &value, depth + 1)?;
                    self.budget
                        .lineage_push(env, (name.as_str(), value), expression.span)?;
                }
                let value = self.expr(owner, env, result, depth + 1)?;
                self.transfer(owner, env, result, &value, depth + 1)?;
                // Hygienic temporaries have ordinary lexical cleanup. Keep
                // the returned creator and its complete original signature.
                let cleanup = self.residual(&env[base..], depth + 1)?;
                self.early_cleanup |= cleanup;
                self.early_body_cleanup |= cleanup;
                env.truncate(base);
                value
            }
            E::TypeApply { callee, .. } | E::GenericApply { callee, .. } => {
                self.expr(owner, env, callee, depth + 1)?
            }
            E::Literal(_)
            | E::Integer(_)
            | E::Float(_)
            | E::Bool(_)
            | E::String(_)
            | E::Character(_)
            | E::Unit => Value::Plain,
            // No unsupported projection/control is treated as evidence that
            // a previously owned Task was transferred or synchronously closed.
            _ => Value::Unknown,
        };
        Ok(value)
    }
    fn finish_expr(&mut self, value: Value<'a>, depth: usize) -> R<Value<'a>> {
        let cleanup = self.pending(&value, depth + 1)?;
        self.shutdown &= !cleanup.may;
        Ok(value)
    }
    fn call_expr(
        &mut self,
        owner: usize,
        env: &mut Env<'a>,
        callee: &'a SemanticOriginalExpr,
        arguments: &'a [SemanticOriginalExpr],
        span: Span,
        depth: usize,
    ) -> R<CallOutcome<'a>> {
        use CoreExprKind as E;
        let target = self.target(owner, env, callee, depth + 1)?;
        let mut head = callee;
        while let E::Group(inner)
        | E::TypeApply { callee: inner, .. }
        | E::GenericApply { callee: inner, .. } = &head.kind
        {
            head = inner;
        }
        let primitive = match &head.kind {
            E::Name(name)
                if target.is_none()
                    && self.lookup(env, name, depth + 1)?.is_none()
                    && crate::primitive_name(name) =>
            {
                Some(name.as_str())
            }
            _ => None,
        };
        let callable = if primitive.is_some() {
            Value::Plain
        } else {
            self.expr(owner, env, callee, depth + 1)?
        };
        let position = self.place(owner, env, callee, depth + 1)?;
        let primitive_place = if matches!(
            primitive,
            Some(
                "task_poll_close"
                    | "task_poll"
                    | "task_request_cancel"
                    | "async_registration_poll_close"
                    | "async_registration_request_close"
            )
        ) {
            if let Some(argument) = arguments.first() {
                self.place(owner, env, argument, depth + 1)?
            } else {
                None
            }
        } else {
            None
        };
        let saved_prepared = self.prepared;
        let base = self.prepared_values.len();
        let consuming = matches!(
            &callable,
            Value::Callable {
                kind: CallableKind::FnOnce,
                ..
            }
        );
        if consuming {
            self.take(owner, env, callee, depth + 1)?;
        }
        self.park(
            callable,
            consuming || position.is_none(),
            callee.span,
            depth + 1,
        )?;
        let mut mutated = Vec::new();
        for (ordinal, argument) in arguments.iter().enumerate() {
            let value = self.expr(owner, env, argument, depth + 1)?;
            if let Value::View {
                place,
                mutable: true,
            } = &value
            {
                let place = self.place_copy(place, depth + 1)?;
                self.budget
                    .lineage_push(&mut mutated, place, argument.span)?;
            }
            let parameters = match &self.prepared_values[base] {
                Value::Callable { signature, .. } => Some(signature.parameters.as_slice()),
                Value::Function(index) => signature(self.definitions[*index])
                    .map(|signature| signature.parameters.as_slice()),
                _ => target
                    .and_then(|index| signature(self.definitions[index]))
                    .map(|signature| signature.parameters.as_slice()),
            };
            let parameter =
                if matches!(primitive, Some("task_create" | "task_create_owned")) && ordinal != 0 {
                    match self.prepared_values.get(base + 1) {
                        Some(Value::Function(index)) => signature(self.definitions[*index])
                            .and_then(|signature| signature.parameters.get(ordinal - 1)),
                        _ => None,
                    }
                } else {
                    parameters.and_then(|parameters| parameters.get(ordinal))
                };
            let owned = parameter.is_some_and(|parameter| {
                !matches!(
                    parameter.ty,
                    SemanticTypeRecipe::Address { .. } | SemanticTypeRecipe::Desc { .. }
                )
            });
            if owned {
                self.transfer(owner, env, argument, &value, depth + 1)?;
            }
            self.park(value, owned, argument.span, depth + 1)?;
        }
        for place in mutated {
            self.invalidate(env, &place, depth + 1)?;
            if primitive.is_none() {
                if let Some(value) = self.place_value_mut(owner, env, &place, depth + 1)? {
                    Self::forget_mutated(self.budget, value, depth + 1)?;
                }
            }
        }
        if let Some(place) = &primitive_place {
            self.invalidate(env, place, depth + 1)?;
        }
        self.budget.identity_work(arguments.len(), span)?;
        let mut callable = self.prepared_values.remove(base);
        let inputs = self.unpark(base, span)?;
        self.prepared = saved_prepared;
        // A stored non-consuming receiver is read after all argument
        // effects. Its pre-argument clone cannot supply later state.
        if !consuming {
            if let Some(place) = &position {
                if let Some(current) = self.place_value_mut(owner, env, place, depth + 1)? {
                    callable = self.copy(current, depth + 1)?;
                }
            }
        }
        if primitive.is_some_and(|name| {
            name.starts_with("async_registration_") || name == "c_register_async"
        }) {
            self.shutdown = false;
        }
        if matches!(primitive, Some("task_yield" | "task_await")) {
            self.execution = true;
            self.known_execution = true;
        }
        let value = match callable {
            Value::Callable {
                scope,
                kind,
                signature,
                suspend,
                known_suspend,
                shutdown,
                after,
                ready,
                ..
            } => {
                self.shutdown &= shutdown;
                self.execution |= suspend;
                self.known_execution |= known_suspend;
                if kind != CallableKind::FnOnce {
                    if let Some(place) = &position {
                        self.invalidate(env, place, depth + 1)?;
                        if let Some(Value::Callable { cleanup, .. }) =
                            self.place_value_mut(owner, env, place, depth + 1)?
                        {
                            *cleanup = after;
                        }
                    } else {
                        self.early_cleanup |= after;
                        self.early_body_cleanup |= after;
                    }
                }
                if ready.is_some() && kind != CallableKind::FnOnce {
                    if let Some(place) = position {
                        Value::Poll(place)
                    } else {
                        self.ty(scope, &signature.result, depth + 1)?
                    }
                } else {
                    self.ty(scope, &signature.result, depth + 1)?
                }
            }
            Value::Function(index) => {
                self.execution |= signature(self.definitions[index])
                    .is_none_or(|signature| signature.call_kind == CallKind::Suspend);
                self.known_execution |= self.evidence.rows[index].function;
                return Ok(CallOutcome::Defined {
                    owner: index,
                    inputs,
                });
            }
            _ => {
                if let Some(index) = target {
                    self.known_execution |= self.evidence.rows[index].function;
                    return Ok(CallOutcome::Defined {
                        owner: index,
                        inputs,
                    });
                } else if matches!(
                    primitive,
                    Some("task_poll_close" | "async_registration_poll_close")
                ) {
                    if let Some(place) = primitive_place {
                        if matches!(
                            self.place_value_mut(owner, env, &place, depth + 1)?,
                            Some(Value::Task | Value::SyncTask | Value::SettledTask)
                        ) {
                            Value::Poll(place)
                        } else {
                            Value::Plain
                        }
                    } else {
                        Value::Plain
                    }
                } else if matches!(primitive, Some("task_create" | "task_create_owned")) {
                    let task = if let Some(Value::Function(worker)) = inputs.first() {
                        if self.worker_shutdown(*worker, &inputs[1..], depth + 1)? {
                            Value::SyncTask
                        } else {
                            Value::Task
                        }
                    } else {
                        Value::Task
                    };
                    let mut values = Vec::new();
                    let mut error = Vec::new();
                    if primitive == Some("task_create_owned") {
                        let mut inputs = inputs.into_iter();
                        inputs.next();
                        let mut owned = Vec::new();
                        for input in inputs {
                            self.budget.lineage_push(&mut owned, input, span)?;
                        }
                        self.budget.lineage_push(
                            &mut error,
                            ("inputs", Value::Tuple(owned)),
                            span,
                        )?;
                        self.budget
                            .lineage_push(&mut error, ("reason", Value::Plain), span)?;
                    }
                    for (is_error, value) in [(true, Value::Record(error)), (false, task)] {
                        self.budget
                            .lineage_bytes(std::mem::size_of::<Value<'a>>(), span)?;
                        self.budget.lineage_push(
                            &mut values,
                            Value::Error {
                                is_error,
                                value: Box::new(value),
                            },
                            span,
                        )?;
                    }
                    Value::Alternatives(values)
                } else if primitive == Some("c_register_async") {
                    Value::Task
                } else if matches!(
                    primitive,
                    Some(
                        "task_valid"
                            | "task_yield"
                            | "task_request_cancel"
                            | "async_registration_request_close"
                            | "async_registration_finish_destroy_checked"
                            | "task_finish_destroy"
                            | "task_finish_destroy_checked"
                            | "task_poll"
                    )
                ) {
                    Value::Plain
                } else {
                    self.shutdown = false;
                    Value::Unknown
                }
            }
        };
        Ok(CallOutcome::Value(value))
    }
    fn closure_expr(
        &mut self,
        owner: usize,
        env: &mut Env<'a>,
        expression: &'a SemanticOriginalExpr,
        move_capture: bool,
        depth: usize,
    ) -> R<Value<'a>> {
        let definition = self.definitions[owner];
        let SemanticDefinitionBody::FunctionWithBody {
            sites, site_nodes, ..
        } = &definition.body
        else {
            return Ok(Value::Unknown);
        };
        self.budget.identity_work(
            sites.len().saturating_add(site_nodes.len()),
            expression.span,
        )?;
        let mut child = None;
        for node in site_nodes {
            if node.node != SemanticOriginalNode::Expression(expression.id) {
                continue;
            }
            if let Some(SemanticSiteRecipe {
                kind: SemanticSiteRecipeKind::Closure { definition, .. },
                ..
            }) = sites.iter().find(|site| site.id == node.site)
            {
                let found =
                    declaration_index(self.definitions, *definition, self.budget, expression.span)?;
                if child.is_some() && child != found {
                    return Err(error(
                        expression.span,
                        "opaque cleanup has ambiguous original creator",
                    ));
                }
                child = found;
            }
        }
        let Some(child) = child else {
            return Ok(Value::Unknown);
        };
        if self.definitions[child].parent != Some(definition.id) {
            return Err(error(
                expression.span,
                "opaque cleanup creator changes original parent",
            ));
        }
        let SemanticDefinitionBody::FunctionWithBody {
            signature: child_signature,
            parameters,
            body: SemanticOriginalFunctionBody::Defined(body),
            ..
        } = &self.definitions[child].body
        else {
            return Ok(Value::Unknown);
        };
        let mut captured = Vec::new();
        for slot in &self.definitions[child].environment {
            self.budget.node(expression.span, depth + 1)?;
            let SemanticActualRecipe::Type(SemanticTypeRecipe::OriginalType {
                source: SemanticOriginalTypeSource::NameAt { at, name },
                ..
            }) = &slot.source
            else {
                return Ok(Value::Unknown);
            };
            if *at != SemanticOriginalNode::Expression(expression.id) {
                return Err(error(
                    expression.span,
                    "opaque cleanup capture selects another creator",
                ));
            }
            let value = if move_capture {
                self.lookup(env, name, depth + 1)?.unwrap_or(Value::Unknown)
            } else {
                Value::Plain
            };
            self.budget
                .lineage_push(&mut captured, (name.as_str(), value), expression.span)?;
            if move_capture {
                self.budget.identity_work(
                    env.len().saturating_mul(name.len().saturating_add(1)),
                    expression.span,
                )?;
                if let Some(index) = env.iter().rposition(|(binding, _)| *binding == name) {
                    self.invalidate(
                        env,
                        &Place {
                            owner,
                            index,
                            fields: Vec::new(),
                        },
                        depth + 1,
                    )?;
                }
                for (binding, value) in env.iter_mut().rev() {
                    self.budget.text(name, expression.span)?;
                    if *binding == name {
                        *value = Value::Plain;
                        break;
                    }
                }
            }
        }
        // Uncalled cleanup owns only captures, never hypothetical
        // invocation inputs which have not been supplied.
        let cleanup = self.residual(&captured, depth + 1)?;
        let mut unknown_write = Cleanup::NONE;
        for (_, value) in &captured {
            unknown_write |= self.unknown_write_cleanup(value, depth + 1)?;
        }
        let held = std::mem::replace(&mut self.held_captures, captured.len());
        for (parameter, ty) in parameters.iter().zip(&child_signature.parameters) {
            let value = self.ty(child, &ty.ty, depth + 1)?;
            self.budget.lineage_push(
                &mut captured,
                (parameter.name.as_str(), value),
                parameter.span,
            )?;
        }
        let shutdown = std::mem::replace(&mut self.shutdown, true);
        let execution = std::mem::replace(&mut self.execution, false);
        let known_execution = std::mem::replace(&mut self.known_execution, false);
        let early = std::mem::replace(&mut self.early_cleanup, Cleanup::NONE);
        let early_body = std::mem::replace(&mut self.early_body_cleanup, Cleanup::NONE);
        let operands = self.enter_operand_frame();
        let caller_cache = self.last_plain_call.take();
        let mut flow = self.block(child, &mut captured, body, depth + 1)?;
        flow.cleanup |= self.early_cleanup;
        flow.body_cleanup |= self.early_body_cleanup;
        if flow.live {
            flow.cleanup |= self.residual(&captured, depth + 1)?;
            flow.body_cleanup |= self.residual(&captured[self.held_captures..], depth + 1)?;
        }
        let pending = self.execution || flow.cleanup.may || flow.incomplete;
        flow.shutdown = self.shutdown;
        if self.publish && flow.body_cleanup.asynchronous {
            self.budget.node(expression.span, depth)?;
            self.bodies[child] = true;
        }
        let known =
            self.known_execution || flow.cleanup.asynchronous || self.evidence.rows[child].function;
        self.shutdown = shutdown;
        self.execution = execution;
        self.known_execution = known_execution;
        self.held_captures = held;
        self.early_cleanup = early;
        self.early_body_cleanup = early_body;
        self.last_plain_call = caller_cache;
        self.leave_operand_frame(operands);
        let value = Value::Callable {
            definition: Some(child),
            scope: child,
            kind: CallableKind::FnMut,
            signature: child_signature,
            suspend: pending || child_signature.call_kind == CallKind::Suspend,
            known_suspend: known,
            shutdown: flow.shutdown,
            cleanup,
            initial: cleanup,
            unknown_write,
            after: flow.cleanup,
            ready: flow.ready.known(),
        };
        self.finish_expr(value, depth)
    }
    fn pattern_owns(&mut self, pattern: &SemanticOriginalPattern, depth: usize) -> R<bool> {
        self.budget.node(pattern.span, depth)?;
        Ok(match &pattern.kind {
            CorePatternKind::Reference { .. } => false,
            CorePatternKind::Tuple(fields)
            | CorePatternKind::Variant { fields, .. }
            | CorePatternKind::Constructor { fields, .. } => {
                let mut owns = true;
                for field in fields {
                    owns &= self.pattern_owns(field, depth + 1)?;
                }
                owns
            }
            CorePatternKind::Record { fields, .. }
            | CorePatternKind::RecordConstructor { fields, .. } => {
                let mut owns = true;
                for (_, field) in fields {
                    owns &= self.pattern_owns(field, depth + 1)?;
                }
                owns
            }
            CorePatternKind::ErrorArm { pattern, .. } => self.pattern_owns(pattern, depth + 1)?,
            _ => true,
        })
    }
    fn pattern(
        &mut self,
        env: &mut Env<'a>,
        pattern: &'a SemanticOriginalPattern,
        value: Value<'a>,
        depth: usize,
    ) -> R<Cleanup> {
        self.budget.node(pattern.span, depth)?;
        Ok(match (&pattern.kind, value) {
            (CorePatternKind::Name(name), value) => {
                self.budget
                    .lineage_push(env, (name.as_str(), value), pattern.span)?;
                Cleanup::NONE
            }
            (CorePatternKind::Tuple(patterns), Value::Tuple(values))
                if patterns.len() == values.len() =>
            {
                let mut dropped = Cleanup::NONE;
                for (pattern, value) in patterns.iter().zip(values) {
                    dropped |= self.pattern(env, pattern, value, depth + 1)?;
                }
                dropped
            }
            (
                CorePatternKind::Tuple(patterns)
                | CorePatternKind::Variant {
                    fields: patterns, ..
                }
                | CorePatternKind::Constructor {
                    fields: patterns, ..
                },
                Value::Plain | Value::Poll(_),
            ) => {
                for pattern in patterns {
                    self.pattern(env, pattern, Value::Plain, depth + 1)?;
                }
                Cleanup::NONE
            }
            (
                CorePatternKind::Record {
                    fields: patterns, ..
                }
                | CorePatternKind::RecordConstructor {
                    fields: patterns, ..
                },
                Value::Record(fields),
            ) => {
                let mut dropped = Cleanup::NONE;
                for (name, value) in fields {
                    self.budget.identity_work(patterns.len(), pattern.span)?;
                    if let Some((_, field)) =
                        patterns.iter().find(|(field, _)| field.as_str() == name)
                    {
                        dropped |= self.pattern(env, field, value, depth + 1)?;
                    } else {
                        dropped |= self.pending(&value, depth + 1)?;
                    }
                }
                dropped
            }
            (CorePatternKind::Reference { pattern, .. }, value) => {
                let pending = self.pending(&value, depth + 1)?;
                self.pattern(env, pattern, Value::Plain, depth + 1)?;
                pending
            }
            (CorePatternKind::ErrorArm { pattern, .. }, Value::Error { value, .. }) => {
                self.pattern(env, pattern, *value, depth + 1)?
            }
            (CorePatternKind::ErrorArm { pattern, .. }, value) => {
                self.pattern(env, pattern, value, depth + 1)?
            }
            (_, value) => self.pending(&value, depth + 1)?,
        })
    }
    fn result(
        &mut self,
        output: &mut Option<Value<'a>>,
        value: Option<Value<'a>>,
        depth: usize,
    ) -> R<()> {
        if let Some(value) = value {
            *output = Some(match output.take() {
                Some(old) => self.join(old, value, depth + 1)?,
                None => value,
            });
        }
        Ok(())
    }
    fn absorb(&mut self, output: &mut Flow<'a>, mut flow: Flow<'a>, depth: usize) -> R<()> {
        output.cleanup |= flow.cleanup;
        output.body_cleanup |= flow.body_cleanup;
        output.incomplete |= flow.incomplete;
        output.ready = output.ready.join(flow.ready);
        self.result(&mut output.value, flow.value.take(), depth + 1)?;
        for value in flow.breaks {
            self.budget
                .lineage_push(&mut output.breaks, value, Span::default())?;
        }
        for value in flow.continues {
            self.budget
                .lineage_push(&mut output.continues, value, Span::default())?;
        }
        Ok(())
    }
    fn pattern_ready(pattern: &SemanticOriginalPattern) -> Option<bool> {
        match &pattern.kind {
            CorePatternKind::Variant { variant, .. }
            | CorePatternKind::Constructor { variant, .. } => match variant.as_str() {
                "Ready" => Some(true),
                "Pending" => Some(false),
                _ => None,
            },
            _ => None,
        }
    }
    fn join_exit_env(&mut self, output: &mut Option<Env<'a>>, env: Env<'a>, depth: usize) -> R<()> {
        if let Some(previous) = output {
            self.merge_env(previous, env, depth + 1)?;
        } else {
            *output = Some(env);
        }
        Ok(())
    }
    fn loop_flow(
        &mut self,
        owner: usize,
        env: &mut Env<'a>,
        body: &'a SemanticOriginalBlock,
        condition: Option<&'a SemanticOriginalExpr>,
        pattern: Option<&'a SemanticOriginalPattern>,
        may_skip: bool,
        depth: usize,
    ) -> R<Flow<'a>> {
        let initial = self.env_copy(env, depth + 1)?;
        let mut entry = self.env_copy(env, depth + 1)?;
        let mut exits = None;
        let mut output = Flow::new();
        loop {
            self.budget.node(body.span, depth)?;
            let mut iteration = self.env_copy(&entry, depth + 1)?;
            let mut subject = None;
            if let Some(condition) = condition {
                let value = self.expr(owner, &mut iteration, condition, depth + 1)?;
                if may_skip {
                    let mut exit = self.env_copy(&iteration, depth + 1)?;
                    if pattern.and_then(Self::pattern_ready) == Some(false) {
                        self.refine_ready(owner, &mut exit, &value, depth + 1)?;
                    }
                    self.join_exit_env(&mut exits, exit, depth + 1)?;
                }
                if pattern.and_then(Self::pattern_ready) == Some(true) {
                    self.refine_ready(owner, &mut iteration, &value, depth + 1)?;
                }
                subject = Some(value);
            } else if may_skip {
                let exit = self.env_copy(&iteration, depth + 1)?;
                self.join_exit_env(&mut exits, exit, depth + 1)?;
            }
            let before = iteration.len();
            if let (Some(pattern), Some(value)) = (pattern, subject) {
                output.drop_value(self.pattern(&mut iteration, pattern, value, depth + 1)?);
            }
            let mut flow = self.block(owner, &mut iteration, body, depth + 1)?;
            if flow.live {
                flow.drop_value(self.residual(&iteration[before..], depth + 1)?);
                iteration.truncate(before);
            }
            for mut exit in std::mem::take(&mut flow.breaks) {
                exit.truncate(before);
                self.join_exit_env(&mut exits, exit, depth + 1)?;
            }
            let mut back = None;
            if flow.live {
                self.join_exit_env(&mut back, iteration, depth + 1)?;
            }
            for mut next in std::mem::take(&mut flow.continues) {
                next.truncate(before);
                self.join_exit_env(&mut back, next, depth + 1)?;
            }
            self.absorb(&mut output, flow, depth + 1)?;
            let Some(back) = back else {
                break;
            };
            let mut next = self.env_copy(&initial, depth + 1)?;
            self.merge_env(&mut next, back, depth + 1)?;
            if self.same_env(&entry, &next, depth + 1)? {
                break;
            }
            entry = next;
        }
        output.live = exits.is_some();
        if let Some(exit) = exits {
            *env = exit;
        }
        Ok(output)
    }
    // Return expressions may enter another complete function body. Evaluate
    // them after releasing the current statement's branch and loop storage.
    fn statement(
        &mut self,
        owner: usize,
        env: &mut Env<'a>,
        output: &mut Flow<'a>,
        statement: &'a SemanticOriginalStatement,
        depth: usize,
    ) -> R<std::ops::ControlFlow<Option<&'a SemanticOriginalExpr>>> {
        use CoreStatementKind as S;
        match &statement.kind {
            S::Binding {
                name,
                initializer,
                ty,
                ..
            } => {
                let mut value = if let Some(expr) = initializer {
                    let value = self.expr(owner, env, expr, depth + 1)?;
                    self.transfer(owner, env, expr, &value, depth + 1)?;
                    if matches!(expr.kind, CoreExprKind::Name(_))
                        && matches!(value, Value::Task | Value::SyncTask | Value::SettledTask)
                    {
                        Value::Plain
                    } else {
                        value
                    }
                } else if let Some(ty) = ty {
                    self.ty(owner, ty, depth + 1)?
                } else {
                    Value::Unknown
                };
                if matches!(value, Value::Unknown) {
                    if let Some(ty) = ty {
                        let declared = self.ty(owner, ty, depth + 1)?;
                        if self.pending(&declared, depth + 1)?.asynchronous {
                            value = declared;
                        }
                    }
                }
                self.invalidate(
                    env,
                    &Place {
                        owner,
                        index: env.len(),
                        fields: Vec::new(),
                    },
                    depth + 1,
                )?;
                self.budget
                    .lineage_push(env, (name.as_str(), value), statement.span)?;
            }
            S::TupleBinding {
                pattern,
                initializer,
                ..
            } => {
                let value = self.expr(owner, env, initializer, depth + 1)?;
                if self.pattern_owns(pattern, depth + 1)? {
                    self.transfer(owner, env, initializer, &value, depth + 1)?;
                }
                output.drop_value(self.pattern(env, pattern, value, depth + 1)?);
            }
            S::Return(expression) => return Ok(std::ops::ControlFlow::Break(expression.as_ref())),
            S::Expression(expr) | S::Yield(expr) => {
                let value = self.expr(owner, env, expr, depth + 1)?;
                output.drop_value(self.pending(&value, depth + 1)?);
            }
            S::CompoundAssign { value, .. } => {
                self.expr(owner, env, value, depth + 1)?;
            }
            S::Assign {
                target,
                value: expression,
            } => {
                let mut value = self.expr(owner, env, expression, depth + 1)?;
                self.transfer(owner, env, expression, &value, depth + 1)?;
                if let Some(place) = self.place(owner, env, target, depth + 1)? {
                    self.invalidate(env, &place, depth + 1)?;
                    Self::invalidate_value(self.budget, &mut value, &place, depth + 1)?;
                    if let Some(old) = self.place_value_mut(owner, env, &place, depth + 1)? {
                        output.drop_value(self.pending(old, depth + 1)?);
                        *old = value;
                    } else {
                        output.drop_value(self.pending(&value, depth + 1)?);
                    }
                } else {
                    output.drop_value(self.pending(&value, depth + 1)?);
                }
            }
            S::If {
                condition,
                then_body,
                else_body,
            } => {
                self.expr(owner, env, condition, depth + 1)?;
                let mut other = self.env_copy(env, depth + 1)?;
                let left = self.block(owner, env, then_body, depth + 1)?;
                let right = if let Some(body) = else_body {
                    self.block(owner, &mut other, body, depth + 1)?
                } else {
                    Flow::new()
                };
                let live = left.live || right.live;
                if left.live && right.live {
                    self.merge_env(env, other, depth + 1)?;
                } else if right.live {
                    *env = other;
                }
                self.absorb(output, left, depth + 1)?;
                self.absorb(output, right, depth + 1)?;
                output.live = live;
            }
            S::Match { value, arms } => {
                let subject = self.expr(owner, env, value, depth + 1)?;
                let mut continuing = None;
                for arm in arms {
                    let copied = self.copy(&subject, depth + 1)?;
                    for selected in self.select(&arm.pattern, copied, depth + 1)? {
                        let mut branch = self.env_copy(env, depth + 1)?;
                        let before = branch.len();
                        if Self::pattern_ready(&arm.pattern) == Some(true) {
                            self.refine_ready(owner, &mut branch, &selected, depth + 1)?;
                        }
                        if self.pattern_owns(&arm.pattern, depth + 1)? {
                            self.transfer(owner, &mut branch, value, &selected, depth + 1)?;
                        }
                        output.drop_value(self.pattern(
                            &mut branch,
                            &arm.pattern,
                            selected,
                            depth + 1,
                        )?);
                        let mut flow = self.block(owner, &mut branch, &arm.body, depth + 1)?;
                        if flow.live {
                            flow.drop_value(self.residual(&branch[before..], depth + 1)?);
                        }
                        branch.truncate(before);
                        for exit in flow.breaks.iter_mut().chain(&mut flow.continues) {
                            exit.truncate(before);
                        }
                        if flow.live {
                            self.join_exit_env(&mut continuing, branch, depth + 1)?;
                        }
                        self.absorb(output, flow, depth + 1)?;
                    }
                }
                output.live = continuing.is_some();
                if let Some(branch) = continuing {
                    *env = branch;
                }
                if arms.is_empty() {
                    output.drop_value(Cleanup::ABSTRACT);
                }
            }
            S::Block(body) | S::Unsafe(body) => {
                let flow = self.block(owner, env, body, depth + 1)?;
                output.live = flow.live;
                self.absorb(output, flow, depth + 1)?;
            }
            S::Loop(body) => {
                let flow = self.loop_flow(owner, env, body, None, None, false, depth + 1)?;
                output.live = flow.live;
                self.absorb(output, flow, depth + 1)?;
            }
            S::While { condition, body } => {
                let skip = !matches!(condition.kind, CoreExprKind::Bool(true));
                let flow =
                    self.loop_flow(owner, env, body, Some(condition), None, skip, depth + 1)?;
                output.live = flow.live;
                self.absorb(output, flow, depth + 1)?;
            }
            S::WhileLet {
                pattern,
                value,
                body,
            } => {
                let flow = self.loop_flow(
                    owner,
                    env,
                    body,
                    Some(value),
                    Some(pattern),
                    true,
                    depth + 1,
                )?;
                output.live = flow.live;
                self.absorb(output, flow, depth + 1)?;
            }
            S::ForRange {
                start, end, body, ..
            } => {
                self.expr(owner, env, start, depth + 1)?;
                self.expr(owner, env, end, depth + 1)?;
                let flow = self.loop_flow(owner, env, body, None, None, true, depth + 1)?;
                output.live = flow.live;
                self.absorb(output, flow, depth + 1)?;
            }
            S::ForEach { iterator, body, .. } => {
                self.expr(owner, env, iterator, depth + 1)?;
                let flow = self.loop_flow(owner, env, body, None, None, true, depth + 1)?;
                output.live = flow.live;
                self.absorb(output, flow, depth + 1)?;
            }
            S::Defer(body) => {
                let mut deferred = self.env_copy(env, depth + 1)?;
                let flow = self.block(owner, &mut deferred, body, depth + 1)?;
                output.cleanup |= flow.cleanup;
                output.body_cleanup |= flow.body_cleanup;
                output.incomplete |= flow.incomplete
                    || flow.value.is_some()
                    || !flow.breaks.is_empty()
                    || !flow.continues.is_empty();
            }
            S::IfLet {
                pattern,
                value,
                then_body,
                else_body,
            } => {
                let subject = self.expr(owner, env, value, depth + 1)?;
                let mut other = self.env_copy(env, depth + 1)?;
                let before = env.len();
                if Self::pattern_ready(pattern) == Some(true) {
                    self.refine_ready(owner, env, &subject, depth + 1)?;
                }
                if Self::pattern_ready(pattern) == Some(false) {
                    self.refine_ready(owner, &mut other, &subject, depth + 1)?;
                }
                if self.pattern_owns(pattern, depth + 1)? {
                    self.transfer(owner, env, value, &subject, depth + 1)?;
                }
                output.drop_value(self.pattern(env, pattern, subject, depth + 1)?);
                let mut left = self.block(owner, env, then_body, depth + 1)?;
                if left.live {
                    left.drop_value(self.residual(&env[before..], depth + 1)?);
                }
                env.truncate(before);
                for exit in left.breaks.iter_mut().chain(&mut left.continues) {
                    exit.truncate(before);
                }
                let right = if let Some(body) = else_body {
                    self.block(owner, &mut other, body, depth + 1)?
                } else {
                    Flow::new()
                };
                let live = left.live || right.live;
                if left.live && right.live {
                    self.merge_env(env, other, depth + 1)?;
                } else if right.live {
                    *env = other;
                }
                self.absorb(output, left, depth + 1)?;
                self.absorb(output, right, depth + 1)?;
                output.live = live;
            }
            S::With { name, value, body } => {
                let value = self.expr(owner, env, value, depth + 1)?;
                let before = env.len();
                self.budget
                    .lineage_push(env, (name.as_str(), value), statement.span)?;
                let mut flow = self.block(owner, env, body, depth + 1)?;
                if flow.live {
                    flow.drop_value(self.residual(&env[before..], depth + 1)?);
                }
                env.truncate(before);
                for exit in flow.breaks.iter_mut().chain(&mut flow.continues) {
                    exit.truncate(before);
                }
                output.live = flow.live;
                self.absorb(output, flow, depth + 1)?;
            }
            S::Break => {
                let exit = self.env_copy(env, depth + 1)?;
                self.budget
                    .lineage_push(&mut output.breaks, exit, statement.span)?;
                output.live = false;
            }
            S::Continue => {
                let exit = self.env_copy(env, depth + 1)?;
                self.budget
                    .lineage_push(&mut output.continues, exit, statement.span)?;
                output.live = false;
            }
            S::Unsupported(_) => {
                output.drop_value(Cleanup::ABSTRACT);
                output.incomplete = true;
            }
            S::CompileTime(_) => {}
        }
        Ok(std::ops::ControlFlow::Continue(()))
    }
    fn block(
        &mut self,
        owner: usize,
        env: &mut Env<'a>,
        body: &'a SemanticOriginalBlock,
        depth: usize,
    ) -> R<Flow<'a>> {
        self.budget.node(body.span, depth)?;
        let base = env.len();
        let mut output = Flow::new();
        for statement in &body.statements {
            if !output.live {
                break;
            }
            self.budget.node(statement.span, depth + 1)?;
            if let std::ops::ControlFlow::Break(expression) =
                self.statement(owner, env, &mut output, statement, depth)?
            {
                let value = if let Some(expr) = expression {
                    let value = self.expr(owner, env, expr, depth + 1)?;
                    self.transfer(owner, env, expr, &value, depth + 1)?;
                    value
                } else {
                    Value::Plain
                };
                output.cleanup |= self.residual(env, depth + 1)?;
                output.body_cleanup |= self.residual(&env[self.held_captures..], depth + 1)?;
                output.ready =
                    output
                        .ready
                        .join(self.ready_cleanup(owner, env, &value, depth + 1)?);
                self.result(&mut output.value, Some(value), depth + 1)?;
                output.live = false;
            }
        }
        if output.live {
            output.drop_value(self.residual(&env[base..], depth + 1)?);
        }
        for exit in output.breaks.iter_mut().chain(&mut output.continues) {
            let cleanup = self.residual(&exit[base..], depth + 1)?;
            output.cleanup |= cleanup;
            output.body_cleanup |= cleanup;
            exit.truncate(base);
        }
        env.truncate(base);
        Ok(output)
    }
}
fn opaque_bounds<'a>(
    rows: &'a [SemanticRequirementRecipe],
    output: &mut Vec<(CallableKind, &'a SemanticSignatureRecipe)>,
    budget: &mut Budget<'_>,
    depth: usize,
) -> R<()> {
    for row in rows {
        budget.node(Span::default(), depth)?;
        match row {
            SemanticRequirementRecipe::Callable {
                ty:
                    SemanticTypeRecipe::OriginalType {
                        scope: SemanticScopeRef::Ancestor(0),
                        source: SemanticOriginalTypeSource::Result,
                    },
                kind,
                signature,
            } => budget.lineage_push(output, (*kind, signature.as_ref()), Span::default())?,
            SemanticRequirementRecipe::ForAll { requirements, .. } => {
                opaque_bounds(requirements, output, budget, depth + 1)?
            }
            _ => {}
        }
    }
    Ok(())
}
fn propose_inner(
    modules: &mut [(ModuleId, SemanticModule)],
    originals: &SourceDeclarationInventory<'_>,
    evidence: &mut super::Evidence,
    budget: &mut Budget<'_>,
) -> R<bool> {
    let mut definitions = Vec::new();
    for (_, module) in modules.iter() {
        for definition in &module.definitions {
            budget.lineage_push(&mut definitions, definition, Span::default())?;
        }
    }
    let mut seed_changed = false;
    let mut selected = Vec::new();
    let mut bodies = Vec::new();
    for _ in &definitions {
        budget.lineage_push(&mut bodies, false, Span::default())?;
    }
    for (index, definition) in definitions.iter().enumerate() {
        budget.node(Span::default(), 0)?;
        if !matches!(
            definition.body,
            SemanticDefinitionBody::FunctionWithBody {
                body: SemanticOriginalFunctionBody::Defined(_),
                ..
            }
        ) {
            continue;
        }
        // All candidate trees are temporary. Nothing but this bool escapes;
        // actual original definitions/signatures and their allocations remain
        // owned by the surrounding Source program.
        let mark = budget.lineage_checkpoint();
        let suspend = {
            let mut walk = Walk {
                definitions: &definitions,
                active: Vec::new(),
                last_plain_call: None,
                frames: Vec::new(),
                context: None,
                execution: false,
                known_execution: false,
                publish: true,
                shutdown: true,
                evidence: &mut *evidence,
                seed_changed: &mut seed_changed,
                held_captures: 0,
                early_cleanup: Cleanup::NONE,
                early_body_cleanup: Cleanup::NONE,
                prepared: Cleanup::NONE,
                prepared_values: Vec::new(),
                prepared_base: 0,
                bodies: &mut bodies,
                budget: &mut *budget,
            };
            let mut bounds = Vec::new();
            opaque_bounds(&definition.requirements, &mut bounds, walk.budget, 0)?;
            walk.budget.identity_work(bounds.len(), Span::default())?;
            let once = bounds.iter().any(|(kind, _)| *kind == CallableKind::FnOnce);
            if bounds.len() != 1 {
                // Body cleanup belongs to every original Defined function,
                // even when no unique opaque callable prototype can be read.
                // Scan here once in its Gamma, not once per actual call site.
                walk.function_flow(index, 0)?;
                once
            } else {
                let value = walk.function_result(index, 0)?;
                once && matches!(
                    value,
                    Value::Callable { suspend: true, .. } | Value::Unknown | Value::UnknownAsync
                )
            }
        };
        budget.discard_lineage(mark, (), Span::default())?;
        if suspend {
            budget.lineage_push(&mut selected, definition.id, Span::default())?;
        }
    }
    budget.drop_lineage_vec(definitions, Span::default())?;
    fn publish(
        rows: &mut [SemanticRequirementRecipe],
        budget: &mut Budget<'_>,
        depth: usize,
    ) -> R<bool> {
        let mut changed = false;
        for row in rows {
            budget.node(Span::default(), depth)?;
            match row {
                SemanticRequirementRecipe::Callable {
                    ty:
                        SemanticTypeRecipe::OriginalType {
                            scope: SemanticScopeRef::Ancestor(0),
                            source: SemanticOriginalTypeSource::Result,
                        },
                    kind: CallableKind::FnOnce,
                    signature,
                } => {
                    changed |= signature.call_kind != CallKind::Suspend;
                    signature.call_kind = CallKind::Suspend;
                }
                SemanticRequirementRecipe::ForAll { requirements, .. } => {
                    changed |= publish(requirements, budget, depth + 1)?
                }
                _ => {}
            }
        }
        Ok(changed)
    }
    let mut changed = seed_changed;
    let mut index = 0;
    for (_, module) in modules {
        for definition in &mut module.definitions {
            budget.identity_work(selected.len(), Span::default())?;
            if selected.contains(&definition.id) {
                changed |= publish(&mut definition.requirements, budget, 0)?;
            }
            if bodies[index] && !evidence.rows[index].cleanup {
                evidence.rows[index].cleanup = true;
                changed = true;
            }
            if bodies[index] && super::explicit_kind(definition, originals, budget)?.is_none() {
                if let SemanticDefinitionBody::FunctionWithBody { signature, .. } =
                    &mut definition.body
                {
                    changed |= signature.call_kind != CallKind::Suspend;
                    signature.call_kind = CallKind::Suspend;
                }
            }
            index += 1;
        }
    }
    budget.drop_lineage_vec(selected, Span::default())?;
    budget.drop_lineage_vec(bodies, Span::default())?;
    Ok(changed)
}

pub(super) fn propose(
    modules: &mut [(ModuleId, SemanticModule)],
    originals: &SourceDeclarationInventory<'_>,
    evidence: &mut super::Evidence,
    budget: &mut Budget<'_>,
) -> R<bool> {
    let mark = budget.lineage_checkpoint();
    let result = propose_inner(modules, originals, evidence, budget);
    // Only CallKind and preallocated private provenance bits survive; all
    // candidate trees are dropped on both success and error before this reset.
    budget.discard_lineage(mark, (), Span::default())?;
    result
}
