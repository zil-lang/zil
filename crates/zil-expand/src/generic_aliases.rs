//! Definition-time State windows. These facts describe source positions and
//! fresh borrow identities; neither a type slot nor a concrete instance grants
//! an alias proof. Concrete Analyze and Verify still rebuild their own facts.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Position {
    owner: usize,
    field: Option<usize>,
}
impl Position {
    fn overlaps(self, other: Self) -> bool {
        self.owner == other.owner
            && (self.field.is_none() || other.field.is_none() || self.field == other.field)
    }
}

#[derive(Clone)]
struct Loan {
    position: Position,
    mutable: bool,
    parents: Vec<usize>,
    invalid: bool,
    born: usize,
}

#[derive(Clone, Default)]
pub(super) struct AliasState {
    loans: BTreeMap<usize, Loan>,
    locals: BTreeMap<usize, Vec<usize>>,
}

#[derive(Clone, Copy)]
struct Use {
    born: usize,
    local: Option<usize>,
}

#[derive(Clone, Copy)]
struct Held {
    loan: usize,
    mutable: bool,
}

#[derive(Default)]
pub(super) struct Tracker {
    next: usize,
    pub(super) result: Vec<usize>,
    active: Vec<Held>,
    uses: Vec<Use>,
    joins: Vec<BTreeMap<usize, usize>>,
}

#[derive(Clone, Copy)]
pub(super) struct LoopMark {
    first_use: usize,
    first_new_loan: usize,
}

struct Origin {
    position: Position,
    parents: Vec<usize>,
}

fn tree_work(size: usize) -> usize {
    (usize::BITS - size.saturating_add(1).leading_zeros()) as usize + 1
}

impl Check<'_, '_, '_> {
    fn alias_scope(&self) -> bool {
        self.callable
            .as_ref()
            .is_some_and(|scope| scope.owned_state)
    }

    fn alias_fee(&mut self, count: usize, span: Span) -> R<()> {
        for _ in 0..count {
            self.budget.node(span, 0)?;
        }
        Ok(())
    }

    pub(super) fn alias_clone_fee(&mut self, state: &State, span: Span) -> R<()> {
        let Some(aliases) = &state.aliases else {
            return Ok(());
        };
        self.alias_fee(1, span)?;
        for loan in aliases.loans.values() {
            self.alias_fee(loan.parents.len().saturating_add(2), span)?;
        }
        for loans in aliases.locals.values() {
            self.alias_fee(loans.len().saturating_add(1), span)?;
        }
        Ok(())
    }

    pub(super) fn alias_join(
        &mut self,
        state: &mut State,
        branches: &[&State],
        span: Span,
    ) -> R<()> {
        if self.aliases.next == 0 {
            return Ok(());
        }
        self.aliases.joins.clear();
        if let [branch] = branches {
            self.alias_clone_fee(branch, span)?;
            state.aliases = branch.aliases.clone();
            return Ok(());
        }
        self.alias_fee(branches.len().saturating_add(1), span)?;
        if branches.iter().all(|branch| branch.aliases.is_none()) {
            state.aliases = None;
            return Ok(());
        }
        self.alias_fee(1, span)?;
        let mut joined = AliasState::default();
        let mut pinned = BTreeSet::new();
        let first = branches
            .iter()
            .find_map(|branch| branch.aliases.as_ref())
            .expect("nonempty alias join");
        for index in 0..self.aliases.active.len() {
            let id = self.aliases.active[index].loan;
            self.alias_fee(
                tree_work(first.loans.len()).saturating_add(tree_work(pinned.len())),
                span,
            )?;
            let loan = &first.loans[&id];
            pinned.insert(id);
            for parent in &loan.parents {
                self.alias_fee(tree_work(pinned.len()), span)?;
                pinned.insert(*parent);
            }
        }
        self.alias_fee(branches.len(), span)?;
        self.aliases.joins.reserve(branches.len());
        for branch in branches {
            let Some(aliases) = &branch.aliases else {
                self.aliases.joins.push(BTreeMap::new());
                continue;
            };
            // A loan can be dead on one path because that path installed a
            // fresh address, yet still be the valid zero-iteration value on
            // another path. Keep those versions separate instead of OR-ing
            // an obsolete invalid bit into every surviving reference.
            let mut mapping = BTreeMap::new();
            for id in aliases.loans.keys() {
                self.alias_fee(
                    tree_work(pinned.len())
                        .saturating_add(tree_work(mapping.len()))
                        .saturating_add(1),
                    span,
                )?;
                let mapped = if pinned.contains(id) {
                    *id
                } else {
                    self.alias_id(span)?
                };
                mapping.insert(*id, mapped);
            }
            for (id, loan) in &aliases.loans {
                self.alias_fee(
                    loan.parents
                        .len()
                        .saturating_mul(tree_work(mapping.len()).saturating_add(1))
                        .saturating_add(tree_work(mapping.len()))
                        .saturating_add(tree_work(joined.loans.len()))
                        .saturating_add(2),
                    span,
                )?;
                let mut parents = Vec::with_capacity(loan.parents.len());
                for parent in &loan.parents {
                    parents.push(mapping[parent]);
                }
                let value = Loan {
                    position: loan.position,
                    mutable: loan.mutable,
                    parents,
                    invalid: loan.invalid,
                    born: loan.born,
                };
                joined
                    .loans
                    .entry(mapping[id])
                    .and_modify(|old| old.invalid |= loan.invalid)
                    .or_insert(value);
            }
            for (id, loans) in &aliases.locals {
                self.alias_fee(tree_work(joined.locals.len()).saturating_add(1), span)?;
                let target = joined.locals.entry(*id).or_default();
                for loan in loans {
                    self.alias_fee(
                        target
                            .len()
                            .saturating_mul(2)
                            .saturating_add(tree_work(mapping.len()))
                            .saturating_add(1),
                        span,
                    )?;
                    let mapped = mapping[loan];
                    if !target.contains(&mapped) {
                        target.push(mapped);
                    }
                }
            }
            self.aliases.joins.push(mapping);
        }
        state.aliases = Some(joined);
        Ok(())
    }

    pub(super) fn alias_mark_loop(&self) -> LoopMark {
        LoopMark {
            first_use: self.aliases.uses.len(),
            first_new_loan: self.aliases.next,
        }
    }

    pub(super) fn alias_loop_invariant(
        &mut self,
        before: &State,
        after: &State,
        mark: LoopMark,
        span: Span,
    ) -> R<()> {
        let Some(aliases) = &after.aliases else {
            return Ok(());
        };
        for index in mark.first_use..self.aliases.uses.len() {
            self.alias_fee(2, span)?;
            let used = self.aliases.uses[index];
            // A borrow freshly established by this iteration is not an old
            // address reused by the backedge. Outer alias locals still carry
            // the particular loan(s) that the next condition/body can use.
            if used.born >= mark.first_new_loan {
                continue;
            }
            let Some(local) = used.local else { continue };
            self.alias_fee(
                before
                    .locals
                    .len()
                    .saturating_add(tree_work(aliases.locals.len())),
                span,
            )?;
            if !before.locals.values().any(|value| value.id == local) {
                continue;
            }
            if let Some(loans) = aliases.locals.get(&local) {
                for id in loans {
                    self.alias_fee(tree_work(aliases.loans.len()), span)?;
                    if aliases.loans[id].invalid {
                        return Err(error(
                            span,
                            "State alias used by a loop condition or backedge outlives its original field",
                        ));
                    }
                }
            }
        }
        Ok(())
    }

    fn alias_origins(&mut self, expression: &Expr, state: &State, depth: usize) -> R<Vec<Origin>> {
        self.budget.node(expression.span, depth)?;
        match &expression.kind {
            ExprKind::Group(inner) => self.alias_origins(inner, state, depth + 1),
            ExprKind::Name(name) => {
                self.budget.text(name, expression.span)?;
                self.alias_fee(
                    tree_work(state.locals.len()).saturating_mul(name.len().saturating_add(1)),
                    expression.span,
                )?;
                let Some(local) = state.locals.get(name) else {
                    return Ok(Vec::new());
                };
                if let Some(aliases) = &state.aliases {
                    self.alias_fee(tree_work(aliases.locals.len()), expression.span)?;
                    if let Some(loans) = aliases.locals.get(&local.id) {
                        self.alias_fee(loans.len().saturating_add(1), expression.span)?;
                        let mut origins = Vec::with_capacity(loans.len());
                        for id in loans {
                            self.alias_fee(tree_work(aliases.loans.len()), expression.span)?;
                            let loan = &aliases.loans[id];
                            if loan.invalid {
                                return Err(error(
                                    expression.span,
                                    "State alias outlives a moved or replaced field",
                                ));
                            }
                            self.alias_fee(loan.parents.len().saturating_add(2), expression.span)?;
                            let mut parents = Vec::with_capacity(loan.parents.len() + 1);
                            parents.extend(&loan.parents);
                            parents.push(*id);
                            origins.push(Origin {
                                position: loan.position,
                                parents,
                            });
                        }
                        return Ok(origins);
                    }
                }
                self.budget.ty(&local.ty)?;
                if let Some(scope) = &self.callable {
                    for binding in scope.bindings() {
                        self.budget.text(&binding.generic_name, expression.span)?;
                    }
                }
                let callback = self.callable.as_ref().is_some_and(|scope| {
                    scope.bindings().any(|binding| {
                        callable::parameter_mode(&local.ty, &binding.generic_name) == Some(None)
                    })
                });
                if callback || self.package_local(expression, state).is_some() {
                    self.alias_fee(1, expression.span)?;
                    Ok(vec![Origin {
                        position: Position {
                            owner: local.id,
                            field: None,
                        },
                        parents: Vec::new(),
                    }])
                } else {
                    Ok(Vec::new())
                }
            }
            ExprKind::Member { object, member } => {
                let mut origins = self.alias_origins(object, state, depth + 1)?;
                for origin in &mut origins {
                    if origin.position.field.is_none()
                        && let MemberName::Name(name) = member
                    {
                        origin.position.field =
                            Some(self.field_slot(origin.position.owner, name, expression.span)?);
                    }
                }
                Ok(origins)
            }
            ExprKind::Unary {
                op: UnaryOp::Dereference,
                operand,
            }
            | ExprKind::Index {
                object: operand, ..
            }
            | ExprKind::Slice {
                object: operand, ..
            } => self.alias_origins(operand, state, depth + 1),
            _ => Ok(Vec::new()),
        }
    }

    fn alias_access(
        &mut self,
        state: &mut State,
        position: Position,
        mutable: bool,
        through: &[usize],
        moving: bool,
        span: Span,
    ) -> R<()> {
        let Some(aliases) = &mut state.aliases else {
            return Ok(());
        };
        // A prepared callee or earlier argument remains live until this call
        // completes, even when the address local has no later source use.
        for index in 0..self.aliases.active.len() {
            self.alias_fee(tree_work(aliases.loans.len()).saturating_add(1), span)?;
            let held = self.aliases.active[index];
            let loan = &aliases.loans[&held.loan];
            if loan.position.overlaps(position) && (moving || mutable || held.mutable) {
                return Err(error(
                    span,
                    "State field conflicts with a prepared callee or argument window",
                ));
            }
        }
        for (id, loan) in &mut aliases.loans {
            self.alias_fee(through.len().saturating_add(2), span)?;
            if !through.contains(id)
                && loan.position.overlaps(position)
                && (moving || mutable || loan.mutable)
            {
                loan.invalid = true;
            }
        }
        Ok(())
    }

    fn alias_new(
        &mut self,
        state: &mut State,
        origin: Origin,
        mutable: bool,
        span: Span,
    ) -> R<usize> {
        let size = state.aliases.as_ref().map_or(0, |s| s.loans.len());
        self.alias_fee(tree_work(size).saturating_add(3), span)?;
        let id = self.alias_id(span)?;
        state
            .aliases
            .get_or_insert_with(AliasState::default)
            .loans
            .insert(
                id,
                Loan {
                    position: origin.position,
                    mutable,
                    parents: origin.parents,
                    invalid: false,
                    born: id,
                },
            );
        Ok(id)
    }

    fn alias_id(&mut self, span: Span) -> R<usize> {
        let id = self.aliases.next;
        self.aliases.next = id.checked_add(1).ok_or_else(|| {
            Diagnostic::limit(Phase::Expand, "State alias identity overflow", Some(span))
        })?;
        Ok(id)
    }

    pub(super) fn alias_borrow(
        &mut self,
        expression: &Expr,
        state: &mut State,
        mutable: bool,
    ) -> R<()> {
        self.aliases.result.clear();
        if !self.alias_scope() {
            return Ok(());
        }
        let origins = self.alias_origins(expression, state, 0)?;
        self.alias_fee(origins.len(), expression.span)?;
        self.aliases.result.reserve(origins.len());
        let through = self.alias_origin_parents(&origins, expression.span)?;
        for origin in &origins {
            self.alias_access(
                state,
                origin.position,
                mutable,
                &through,
                false,
                expression.span,
            )?;
        }
        for origin in origins {
            let id = self.alias_new(state, origin, mutable, expression.span)?;
            self.alias_fee(1, expression.span)?;
            self.aliases.result.push(id);
        }
        Ok(())
    }

    fn alias_origin_parents(&mut self, origins: &[Origin], span: Span) -> R<Vec<usize>> {
        let work = origins
            .iter()
            .fold(1usize, |n, origin| n.saturating_add(origin.parents.len()));
        self.alias_fee(work, span)?;
        let mut parents = Vec::with_capacity(work - 1);
        for origin in origins {
            parents.extend(&origin.parents);
        }
        Ok(parents)
    }

    pub(super) fn alias_use_local(&mut self, state: &mut State, local: usize, span: Span) -> R<()> {
        self.aliases.result.clear();
        let Some(aliases) = &state.aliases else {
            return Ok(());
        };
        self.alias_fee(tree_work(aliases.locals.len()), span)?;
        let Some(loans) = aliases.locals.get(&local) else {
            return Ok(());
        };
        self.alias_fee(loans.len(), span)?;
        let loans = loans.clone();
        let growth = if loans.len()
            > self
                .aliases
                .uses
                .capacity()
                .saturating_sub(self.aliases.uses.len())
        {
            self.aliases.uses.len()
        } else {
            0
        };
        self.alias_fee(growth.saturating_add(loans.len()).saturating_mul(2), span)?;
        self.aliases.uses.reserve(loans.len());
        for id in &loans {
            self.alias_fee(tree_work(aliases.loans.len()).saturating_add(2), span)?;
            if aliases.loans[id].invalid {
                return Err(error(
                    span,
                    "State alias outlives a moved or replaced field",
                ));
            }
            self.aliases.uses.push(Use {
                born: aliases.loans[id].born,
                local: Some(local),
            });
        }
        self.aliases.result = loans;
        Ok(())
    }

    pub(super) fn alias_bind(&mut self, state: &mut State, local: usize, span: Span) -> R<()> {
        let Some(aliases) = &mut state.aliases else {
            return Ok(());
        };
        self.alias_fee(
            tree_work(aliases.locals.len())
                .saturating_add(self.aliases.result.len())
                .saturating_add(1),
            span,
        )?;
        if self.aliases.result.is_empty() {
            aliases.locals.remove(&local);
        } else {
            aliases.locals.insert(local, self.aliases.result.clone());
        }
        Ok(())
    }

    pub(super) fn alias_invalidate(
        &mut self,
        state: &mut State,
        owner: usize,
        field: Option<usize>,
        span: Span,
    ) -> R<()> {
        self.alias_access(state, Position { owner, field }, true, &[], true, span)
    }

    pub(super) fn alias_replace_place(&mut self, target: &Expr, state: &mut State) -> R<()> {
        if state.aliases.is_none() {
            return Ok(());
        }
        let mut direct = target;
        while let ExprKind::Group(inner) = &direct.kind {
            self.budget.node(direct.span, 0)?;
            direct = inner;
        }
        // Reassigning an address local changes its pointer value, not the old
        // referent. Dereference/field assignments still resolve that referent.
        if let ExprKind::Name(name) = &direct.kind
            && state
                .locals
                .get(name)
                .is_some_and(|local| matches!(group(&local.ty).kind, TypeKind::Reference { .. }))
        {
            return Ok(());
        }
        let origins = self.alias_origins(target, state, 0)?;
        let through = self.alias_origin_parents(&origins, target.span)?;
        for origin in origins {
            self.alias_access(state, origin.position, true, &through, true, target.span)?;
        }
        Ok(())
    }

    pub(super) fn alias_read_place(&mut self, expression: &Expr, state: &mut State) -> R<()> {
        if state.aliases.is_none() {
            return Ok(());
        }
        let origins = self.alias_origins(expression, state, 0)?;
        let through = self.alias_origin_parents(&origins, expression.span)?;
        for origin in origins {
            self.alias_access(
                state,
                origin.position,
                false,
                &through,
                false,
                expression.span,
            )?;
        }
        Ok(())
    }

    pub(super) fn alias_hold_result(
        &mut self,
        state: &mut State,
        mutable: bool,
        span: Span,
    ) -> R<()> {
        if self.aliases.result.is_empty() {
            return Ok(());
        }
        self.alias_fee(self.aliases.result.len(), span)?;
        let ids = self.aliases.result.clone();
        let mut through_len = 0usize;
        for id in &ids {
            let aliases = state.aliases.as_ref().expect("alias result has records");
            self.alias_fee(tree_work(aliases.loans.len()), span)?;
            let loan = &aliases.loans[id];
            if loan.invalid {
                return Err(error(
                    span,
                    "State alias outlives a moved or replaced field",
                ));
            }
            through_len = through_len
                .saturating_add(loan.parents.len())
                .saturating_add(1);
        }
        self.alias_fee(through_len.saturating_add(1), span)?;
        let mut through = Vec::with_capacity(through_len);
        for id in &ids {
            let aliases = state.aliases.as_ref().expect("alias result has records");
            self.alias_fee(tree_work(aliases.loans.len()), span)?;
            through.extend(&aliases.loans[id].parents);
            through.push(*id);
        }
        // Alternatives of one conditional address value are not simultaneous
        // arguments. Check against already prepared values, then retain this
        // whole origin set together.
        for id in &ids {
            let aliases = state.aliases.as_ref().expect("alias result has records");
            self.alias_fee(tree_work(aliases.loans.len()), span)?;
            let position = aliases.loans[id].position;
            self.alias_access(state, position, mutable, &through, false, span)?;
        }
        let growth = if ids.len()
            > self
                .aliases
                .active
                .capacity()
                .saturating_sub(self.aliases.active.len())
        {
            self.aliases.active.len()
        } else {
            0
        };
        self.alias_fee(ids.len().saturating_add(growth).saturating_mul(2), span)?;
        self.aliases.active.reserve(ids.len());
        self.aliases
            .active
            .extend(ids.into_iter().map(|loan| Held { loan, mutable }));
        Ok(())
    }

    pub(super) fn alias_call_start(&self) -> usize {
        self.aliases.active.len()
    }

    pub(super) fn alias_call_end(
        &mut self,
        state: &mut State,
        base: usize,
        result: &TypeRef,
        span: Span,
    ) -> R<()> {
        self.aliases.result.clear();
        if base == self.aliases.active.len() {
            return Ok(());
        }
        if let TypeKind::Reference { mutable, .. } = &group(result).kind {
            self.alias_fee(
                self.aliases
                    .active
                    .len()
                    .saturating_sub(base)
                    .saturating_mul(3),
                span,
            )?;
            let parents = self.aliases.active[base..].to_vec();
            self.aliases.active.truncate(base);
            self.aliases.result.reserve(parents.len());
            for parent in parents {
                let aliases = state.aliases.as_ref().expect("active alias has records");
                self.alias_fee(tree_work(aliases.loans.len()), span)?;
                let loan = &aliases.loans[&parent.loan];
                self.alias_fee(loan.parents.len().saturating_add(3), span)?;
                let mut chain = Vec::with_capacity(loan.parents.len() + 1);
                chain.extend(&loan.parents);
                chain.push(parent.loan);
                let origin = Origin {
                    position: loan.position,
                    parents: chain,
                };
                let id = self.alias_new(state, origin, *mutable, span)?;
                self.aliases.result.push(id);
            }
        } else {
            self.aliases.active.truncate(base);
        }
        Ok(())
    }

    pub(super) fn alias_result_copy(&mut self, span: Span) -> R<Vec<usize>> {
        self.alias_fee(self.aliases.result.len(), span)?;
        Ok(self.aliases.result.clone())
    }

    pub(super) fn alias_result_join(
        &mut self,
        left: Vec<usize>,
        right: Vec<usize>,
        span: Span,
    ) -> R<()> {
        self.aliases.result.clear();
        if left.is_empty() && right.is_empty() {
            return Ok(());
        }
        let count = left.len().saturating_add(right.len());
        self.alias_fee(count, span)?;
        self.aliases.result.reserve(count);
        for (branch, values) in [left, right].into_iter().enumerate() {
            for id in values {
                self.alias_fee(
                    tree_work(self.aliases.joins[branch].len()).saturating_add(1),
                    span,
                )?;
                self.aliases.result.push(self.aliases.joins[branch][&id]);
            }
        }
        Ok(())
    }
}
