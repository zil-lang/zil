//! Forward must facts for actual owning callable occupants. These facts are
//! private cleanup obligations, separate from affine debt and retained loans.
use super::*;

type Point = (usize, usize);
const CLOSED_TASK: u32 = u32::MAX;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Token {
    Parameter(u32),
    ParameterElement(u32, u32),
    Value(ValueId),
    Capture(u32),
}

// Whole owners and aggregate children share one private selector directory.
// Vacant is an actual removal; an absent/unmodelled source remains Unknown.
#[derive(Clone, PartialEq, Eq)]
enum ElementAlias {
    Unknown,
    Vacant,
    Owner(Token),
    // This selector is vacant or contains this exact owner with these Closed
    // leaves. This is not a global Closed fact or an initialized receiver.
    ConditionalOwner(Token, BTreeSet<u32>),
}
#[derive(Clone, Default, PartialEq, Eq)]
struct OwnerAliases { paths: BTreeMap<Vec<PlaceProjection>, ElementAlias> }
impl OwnerAliases {
    fn owner(root: Token) -> Self {
        Self { paths: BTreeMap::from([(Vec::new(), ElementAlias::Owner(root))]) }
    }
    fn root(&self) -> Option<Token> {
        if self.paths.len() != 1 { return None; }
        match self.paths.get(&[][..]) { Some(ElementAlias::Owner(token)) => Some(*token), _ => None }
    }
    fn element(&self, index: u32, b: &mut Budget) -> CompileResult<&ElementAlias> {
        b.lookup(self.paths.len(), "closed exact element alias lookup")?;
        Ok(self.paths.get(&[PlaceProjection::Field(index)][..]).unwrap_or(&ElementAlias::Unknown))
    }
    fn work(&self) -> usize {
        self.paths.iter().map(|(path, value)| path.len().saturating_add(1)
            .saturating_add(match value { ElementAlias::ConditionalOwner(_, leaves) => leaves.len(), _ => 0 }))
            .sum::<usize>().saturating_add(1)
    }
    fn copy(&self, b: &mut Budget) -> CompileResult<Self> {
        b.spend(self.work(), "closed whole/child aliases snapshot")?;
        Ok(self.clone())
    }
    fn set_element(&mut self, index: u32, value: ElementAlias, b: &mut Budget) -> CompileResult<()> {
        b.spend(1, "closed element selector allocation")?;
        b.lookup(self.paths.len(), "closed element alias update")?;
        self.paths.insert(vec![PlaceProjection::Field(index)], value);
        Ok(())
    }
    fn append_under(&mut self, prefix: &[PlaceProjection], source: &Self, b: &mut Budget) -> CompileResult<()> {
        for (path, alias) in &source.paths {
            b.spend(prefix.len().saturating_add(path.len()).saturating_add(
                match alias { ElementAlias::ConditionalOwner(_, leaves) => leaves.len(), _ => 0 }),
                "closed aggregate source selector and conditional proof")?;
            let mut full = prefix.to_vec(); full.extend_from_slice(path);
            b.lookup(self.paths.len(), "closed aggregate source insertion")?;
            self.paths.insert(full, alias.clone());
        }
        Ok(())
    }
    fn project(&self, prefix: &[PlaceProjection], b: &mut Budget) -> CompileResult<Self> {
        let mut result = Self::default();
        for (path, alias) in &self.paths {
            b.spend(path.len().saturating_add(prefix.len()).saturating_add(
                match alias { ElementAlias::ConditionalOwner(_, leaves) => leaves.len(), _ => 0 }),
                "closed actual child selection and conditional proof")?;
            if path.starts_with(prefix) {
                b.lookup(result.paths.len(), "closed child source insertion")?;
                result.paths.insert(path[prefix.len()..].to_vec(), alias.clone());
            }
        }
        Ok(result)
    }
    fn remove_under(&mut self, prefix: &[PlaceProjection], b: &mut Budget) -> CompileResult<()> {
        for (path, alias) in &mut self.paths {
            b.spend(path.len().saturating_add(prefix.len()), "closed actual child removal")?;
            if path.starts_with(prefix) { *alias = ElementAlias::Vacant; }
        }
        Ok(())
    }
    fn retire(&mut self, token: Token) {
        for value in self.paths.values_mut() {
            if matches!(value, ElementAlias::Owner(owner) | ElementAlias::ConditionalOwner(owner, _) if *owner == token) {
                *value = ElementAlias::Unknown;
            }
        }
    }
    fn forget_effects(&mut self, token: Option<Token>) {
        for value in self.paths.values_mut() {
            if let ElementAlias::ConditionalOwner(owner, leaves) = value
                && token.is_none_or(|token| token == *owner) { leaves.clear(); }
        }
    }
    fn contains(&self, token: Token, b: &mut Budget) -> CompileResult<bool> {
        b.spend(self.work(), "closed complete source identity membership")?;
        Ok(self.paths.values().any(|value| matches!(value,
            ElementAlias::Owner(owner) | ElementAlias::ConditionalOwner(owner, _) if *owner == token)))
    }
    fn tokens(&self, b: &mut Budget) -> CompileResult<Vec<Token>> {
        b.spend(self.work(), "closed complete alias retirement inventory")?;
        Ok(self.paths.values().filter_map(|value|
            match value { ElementAlias::Owner(token) | ElementAlias::ConditionalOwner(token, _) => Some(*token),
                _ => None }).collect())
    }
    fn intersect(&mut self, other: &Self,
        closed: &BTreeMap<Token, BTreeSet<u32>>, incoming: &BTreeMap<Token, BTreeSet<u32>>,
        b: &mut Budget) -> CompileResult<()> {
        for (path, value) in &mut self.paths {
            b.spend(path.len().saturating_add(1), "closed conditional selector join")?;
            b.lookup(other.paths.len(), "closed incoming selector lookup")?;
            let right = other.paths.get(path).unwrap_or(&ElementAlias::Unknown);
            let owner = |value: &ElementAlias| match value {
                ElementAlias::Owner(token) | ElementAlias::ConditionalOwner(token, _) => Some(*token), _ => None,
            };
            if matches!((&*value, right), (ElementAlias::Vacant, ElementAlias::Vacant)) { continue; }
            if let (ElementAlias::Owner(left), ElementAlias::Owner(right)) = (&*value, right)
                && left == right { continue; }
            let token = match (owner(value), owner(right)) {
                (Some(left), Some(right)) if left == right => Some(left),
                (Some(token), None) if matches!(right, ElementAlias::Vacant) => Some(token),
                (None, Some(token)) if matches!(value, ElementAlias::Vacant) => Some(token),
                _ => None,
            };
            let Some(token) = token else { *value = ElementAlias::Unknown; continue; };
            let leaves = |alias: &ElementAlias, facts: &BTreeMap<Token, BTreeSet<u32>>, b: &mut Budget|
                -> CompileResult<Option<BTreeSet<u32>>> {
                let selected = match alias {
                    ElementAlias::Owner(owner) => {
                        b.lookup(facts.len(), "closed occupied branch proof")?;
                        facts.get(owner)
                    }
                    ElementAlias::ConditionalOwner(_, leaves) => Some(leaves),
                    ElementAlias::Vacant => return Ok(None),
                    ElementAlias::Unknown => return Err(fail(Default::default(), "unknown conditional owner join")),
                };
                b.spend(selected.map_or(1, |leaves| leaves.len().saturating_add(1)), "closed occupied branch proof copy")?;
                Ok(Some(selected.cloned().unwrap_or_default()))
            };
            let left = leaves(value, closed, b)?;
            let right = leaves(right, incoming, b)?;
            let proof = match (left, right) {
                (Some(mut left), Some(right)) => {
                    for _ in &left { b.lookup(right.len(), "closed conditional live-branch intersection")?; }
                    left.retain(|leaf| right.contains(leaf)); left
                }
                (Some(proof), None) | (None, Some(proof)) => proof,
                (None, None) => unreachable!("at least one occupied branch"),
            };
            *value = ElementAlias::ConditionalOwner(token, proof);
        }
        Ok(())
    }
}

#[derive(Clone, PartialEq, Eq)]
struct Query {
    occupant: Token,
    leaves: BTreeSet<u32>,
}

#[derive(Clone, PartialEq, Eq)]
struct Condition {
    query: Query,
    ready_if: bool,
}

#[derive(Clone, PartialEq, Eq)]
struct Flow {
    aliases: Vec<OwnerAliases>,
    local_aliases: Vec<OwnerAliases>,
    queries: Vec<Option<Query>>,
    local_queries: Vec<Option<Query>>,
    tags: Vec<Option<Query>>,
    local_tags: Vec<Option<Query>>,
    integers: Vec<Option<i32>>,
    local_integers: Vec<Option<i32>>,
    conditions: Vec<Option<Condition>>,
    local_conditions: Vec<Option<Condition>>,
    closed: BTreeMap<Token, BTreeSet<u32>>,
}

impl Flow {
    fn work(&self) -> usize {
        self.aliases
            .len()
            .saturating_add(self.local_aliases.len())
            .saturating_mul(12)
            .saturating_add(self.aliases.iter().chain(&self.local_aliases)
                .map(OwnerAliases::work).sum::<usize>())
            .saturating_add(
                self.queries
                    .iter()
                    .chain(&self.local_queries)
                    .chain(&self.tags)
                    .chain(&self.local_tags)
                    .flatten()
                    .map(|q| q.leaves.len().saturating_add(1))
                    .sum::<usize>(),
            )
            .saturating_add(
                self.conditions
                    .iter()
                    .chain(&self.local_conditions)
                    .flatten()
                    .map(|c| c.query.leaves.len().saturating_add(2))
                    .sum::<usize>(),
            )
            .saturating_add(
                self.closed
                    .values()
                    .map(|s| s.len().saturating_add(1))
                    .sum::<usize>(),
            )
    }
    fn snapshot(&self, b: &mut Budget) -> CompileResult<Self> {
        b.spend(self.work(), "closed occupant snapshot")?;
        Ok(self.clone())
    }
    fn new(f: &TypedFunction, b: &mut Budget) -> CompileResult<Self> {
        b.spend(
            f.values
                .len()
                .saturating_add(f.locals.len())
                .saturating_mul(12),
            "closed occupant tables",
        )?;
        let mut result = Self {
            aliases: vec![OwnerAliases::default(); f.values.len()],
            local_aliases: vec![OwnerAliases::default(); f.locals.len()],
            queries: vec![None; f.values.len()],
            local_queries: vec![None; f.locals.len()],
            tags: vec![None; f.values.len()],
            local_tags: vec![None; f.locals.len()],
            integers: vec![None; f.values.len()],
            local_integers: vec![None; f.locals.len()],
            conditions: vec![None; f.values.len()],
            local_conditions: vec![None; f.locals.len()],
            closed: BTreeMap::new(),
        };
        b.spend(f.parameters.len(), "closed actual parameter occupants")?;
        for (position, local) in f.parameters.iter().enumerate() {
            let ty = f
                .locals
                .get(local.0 as usize)
                .ok_or_else(|| fail(f.span, "closed parameter local absent"))?;
            if matches!(ty.ty, Type::Closure(_)) || task_owner(&ty.ty) {
                result.local_aliases[local.0 as usize] = OwnerAliases::owner(Token::Parameter(position as u32));
            } else if let Type::Array { element, length } = &ty.ty
                && matches!(element.as_ref(), Type::Closure(_)) {
                for index in 0..*length {
                    result.local_aliases[local.0 as usize].set_element(index,
                        ElementAlias::Owner(Token::ParameterElement(position as u32, index)), b)?;
                }
            }
        }
        Ok(result)
    }
    fn add_closed(
        &mut self,
        token: Token,
        leaves: &BTreeSet<u32>,
        b: &mut Budget,
    ) -> CompileResult<()> {
        b.spend(
            self.closed
                .len()
                .saturating_add(leaves.len())
                .saturating_add(1),
            "closed actual leaf update",
        )?;
        self.closed
            .entry(token)
            .or_default()
            .extend(leaves.iter().copied());
        Ok(())
    }
    // A static constructor instruction may execute again in a loop. Forget all
    // old aliases and queries before reusing its finite site token. An older
    // live occupant then becomes unknown rather than aliasing the new frame.
    fn retire(&mut self, token: Token, b: &mut Budget) -> CompileResult<()> {
        b.spend(
            self.work(),
            "closed replacement invalidates old occupant queries",
        )?;
        for alias in self.aliases.iter_mut().chain(&mut self.local_aliases) {
            alias.retire(token);
        }
        for query in self
            .queries
            .iter_mut()
            .chain(&mut self.local_queries)
            .chain(&mut self.tags)
            .chain(&mut self.local_tags)
        {
            if query.as_ref().is_some_and(|q| q.occupant == token) {
                *query = None;
            }
        }
        for condition in self.conditions.iter_mut().chain(&mut self.local_conditions) {
            if condition
                .as_ref()
                .is_some_and(|c| c.query.occupant == token)
            {
                *condition = None;
            }
        }
        self.closed.remove(&token);
        Ok(())
    }
    fn forget_effects(&mut self, b: &mut Budget) -> CompileResult<()> {
        b.spend(
            self.work(),
            "unproved mutable execution invalidates closed facts",
        )?;
        self.closed.clear();
        for alias in self.aliases.iter_mut().chain(&mut self.local_aliases) { alias.forget_effects(None); }
        self.queries.fill(None);
        self.local_queries.fill(None);
        self.tags.fill(None);
        self.local_tags.fill(None);
        self.conditions.fill(None);
        self.local_conditions.fill(None);
        Ok(())
    }
    fn expose_mutable_head(&mut self, token: Token, b: &mut Budget) -> CompileResult<()> {
        // A retained mutable head may later be driven by another Task or
        // callable. Do not carry an older close observation across that lease.
        // The owning handle itself still exists and may be moved normally.
        b.spend(
            self.work().saturating_mul(2),
            "closed mutable head exposure",
        )?;
        self.closed.remove(&token);
        for alias in self.aliases.iter_mut().chain(&mut self.local_aliases) { alias.forget_effects(Some(token)); }
        for query in self.queries.iter_mut().chain(&mut self.local_queries).chain(&mut self.tags).chain(&mut self.local_tags) {
            if query.as_ref().is_some_and(|query| query.occupant == token) { *query = None; }
        }
        for condition in self.conditions.iter_mut().chain(&mut self.local_conditions) {
            if condition.as_ref().is_some_and(|condition| condition.query.occupant == token) { *condition = None; }
        }
        Ok(())
    }
    fn merge(&mut self, incoming: &Self, b: &mut Budget) -> CompileResult<bool> {
        b.spend(
            self.work()
                .saturating_add(incoming.work())
                .saturating_mul(2),
            "closed CFG must intersection",
        )?;
        let old = self.clone();
        fn intersect<T: PartialEq>(left: &mut [Option<T>], right: &[Option<T>]) {
            for (left, right) in left.iter_mut().zip(right) {
                if left != right {
                    *left = None;
                }
            }
        }
        for (alias, other) in self.aliases.iter_mut().zip(&incoming.aliases) {
            alias.intersect(other, &self.closed, &incoming.closed, b)?;
        }
        for (alias, other) in self.local_aliases.iter_mut().zip(&incoming.local_aliases) {
            alias.intersect(other, &self.closed, &incoming.closed, b)?;
        }
        intersect(&mut self.queries, &incoming.queries);
        intersect(&mut self.local_queries, &incoming.local_queries);
        intersect(&mut self.tags, &incoming.tags);
        intersect(&mut self.local_tags, &incoming.local_tags);
        intersect(&mut self.integers, &incoming.integers);
        intersect(&mut self.local_integers, &incoming.local_integers);
        intersect(&mut self.conditions, &incoming.conditions);
        intersect(&mut self.local_conditions, &incoming.local_conditions);
        self.closed.retain(|token, leaves| {
            if let Some(other) = incoming.closed.get(token) {
                leaves.retain(|leaf| other.contains(leaf));
            } else {
                leaves.clear();
            }
            !leaves.is_empty()
        });
        Ok(*self != old)
    }
}

fn task_owner(ty: &Type) -> bool {
    matches!(ty, Type::Managed(payload) if matches!(payload.as_ref(), Type::Task { .. } | Type::TaskI32))
}

fn ordinary_teardown(module: &TypedModule, ty: &Type, b: &mut Budget) -> CompileResult<bool> {
    b.type_work(ty, false, "closed capture remaining teardown type")?;
    if super::super::super::stored_callables::scalar(ty)
        || matches!(ty, Type::Address { .. } | Type::Desc(_))
    {
        return Ok(true);
    }
    let Type::Managed(payload) = ty else {
        return Ok(false);
    };
    if !super::super::super::stored_callables::resource_payload(payload, b)? {
        return Ok(false);
    }
    b.spend(
        module.resources.len(),
        "closed capture actual resource teardown inventory",
    )?;
    let mut matched = None;
    for resource in &module.resources {
        b.type_work(
            &resource.payload,
            false,
            "closed capture nominal resource identity",
        )?;
        if resource.payload == **payload {
            if matched.replace(resource).is_some() {
                return Ok(false);
            }
        }
    }
    let Some(resource) = matched else {
        return Ok(false);
    };
    let Some(id) = resource.teardown else {
        return Ok(true);
    };
    let Some(drop) = module.functions.get(id.0 as usize) else {
        return Ok(false);
    };
    b.spend(
        drop.parameters.len().saturating_add(1),
        "closed capture exact direct teardown ABI",
    )?;
    Ok(drop.call_kind == CallKind::Direct
        && drop.foreign.is_none()
        && drop.environment.is_none()
        && drop.return_type == Type::Unit
        && matches!(drop.parameters.as_slice(), [parameter]
            if drop.locals.get(parameter.0 as usize).is_some_and(|local| local.ty == Type::Desc(payload.clone()))))
}

fn inventory(
    module: &TypedModule,
    id: ClosureId,
    b: &mut Budget,
) -> CompileResult<Option<BTreeSet<u32>>> {
    let Some(d) = module.closures.get(id.0 as usize) else {
        return Ok(None);
    };
    let Some(entry) = module.functions.get(d.function.0 as usize) else {
        return Ok(None);
    };
    if entry.environment != Some(id)
        || entry.foreign.is_some()
        || entry.call_kind != CallKind::Direct
        || entry.return_type != d.result
    {
        return Ok(None);
    }
    b.spend(
        d.captures.len(),
        "closed actual owned capture leaf inventory",
    )?;
    let mut tasks = BTreeSet::new();
    for (index, ty) in d.captures.iter().enumerate() {
        if task_owner(ty) {
            tasks.insert(index as u32);
        } else if !ordinary_teardown(module, ty, b)? {
            return Ok(None);
        }
    }
    Ok(Some(tasks))
}

#[derive(Default)]
struct ExitProof {
    always: BTreeSet<u32>,
    ready: BTreeSet<u32>,
}

fn entry_is_stable(
    f: &TypedFunction,
    captures: &BTreeSet<u32>,
    b: &mut Budget,
) -> CompileResult<bool> {
    for row in &f.blocks {
        b.spend(
            row.instructions.len(),
            "closed capture replacement/exposure inventory",
        )?;
        for instruction in &row.instructions {
            match &instruction.kind {
                InstructionKind::TakeCapture { index }
                | InstructionKind::WriteCapture { index, .. }
                    if captures.contains(index) =>
                {
                    return Ok(false);
                }
                InstructionKind::Call { .. }
                | InstructionKind::SuspendCall { .. }
                | InstructionKind::CommitPlaceReplace { .. }
                | InstructionKind::StoreStoredBorrow { .. }
                | InstructionKind::CallIndirect { .. }
                | InstructionKind::ForeignCall { .. }
                | InstructionKind::CallClosure { .. }
                | InstructionKind::CallClosureShared { .. }
                | InstructionKind::CallClosureMut { .. }
                | InstructionKind::CallClosureOnce { .. }
                | InstructionKind::CallClosureBorrowed { .. }
                | InstructionKind::WriteAddress { .. }
                | InstructionKind::WithForeignCallback { .. }
                | InstructionKind::WithScopedThread { .. }
                | InstructionKind::WithSyncCell { .. } => return Ok(false),
                _ => {}
            }
        }
    }
    Ok(true)
}

fn clone_query(query: &Option<Query>, b: &mut Budget) -> CompileResult<Option<Query>> {
    b.spend(
        query
            .as_ref()
            .map_or(1, |q| q.leaves.len().saturating_add(1)),
        "closed query transport",
    )?;
    Ok(query.clone())
}

fn step(
    module: &TypedModule,
    f: &TypedFunction,
    state: &mut Flow,
    i: &Instruction,
    body: bool,
    b: &mut Budget,
) -> CompileResult<()> {
    b.spend(1, "closed actual instruction")?;
    let result = i.result.map(|v| v.0 as usize);
    if let Some(out) = result {
        if out >= state.aliases.len() {
            return Err(fail(i.span, "closed result outside actual SSA arena"));
        }
        state.aliases[out] = OwnerAliases::default();
        state.queries[out] = None;
        state.tags[out] = None;
        state.conditions[out] = None;
        state.integers[out] = None;
    }
    match &i.kind {
        InstructionKind::BorrowCapture {
            index,
            mutable: true,
        } if body => {
            let ty = f
                .environment
                .and_then(|id| module.closures.get(id.0 as usize))
                .and_then(|d| d.captures.get(*index as usize));
            if ty.is_some_and(task_owner)
                && let Some(out) = result
            {
                state.aliases[out] = OwnerAliases::owner(Token::Capture(*index));
            }
        }
        InstructionKind::BorrowLocalMut { local } | InstructionKind::BorrowLocal { local } => {
            if matches!(i.kind, InstructionKind::BorrowLocalMut { .. })
                && f.locals
                    .get(local.0 as usize)
                    .is_some_and(|l| matches!(l.ty, Type::Closure(_)))
                && let Some(token) = state.local_aliases.get(local.0 as usize).and_then(OwnerAliases::root)
            {
                state.expose_mutable_head(token, b)?;
            }
            if let Some(out) = result {
                state.aliases[out] = state.local_aliases.get(local.0 as usize)
                    .map(|alias| alias.copy(b)).transpose()?.unwrap_or_default();
            }
        }
        InstructionKind::Load(local) | InstructionKind::TakeLocal { local } => {
            let local = local.0 as usize;
            if local >= state.local_aliases.len() {
                return Err(fail(i.span, "closed source local absent"));
            }
            if let Some(out) = result {
                state.aliases[out] = state.local_aliases[local].copy(b)?;
                state.queries[out] = clone_query(&state.local_queries[local], b)?;
                state.tags[out] = clone_query(&state.local_tags[local], b)?;
                state.integers[out] = state.local_integers[local];
                b.spend(
                    state.local_conditions[local]
                        .as_ref()
                        .map_or(1, |c| c.query.leaves.len().saturating_add(2)),
                    "closed condition move",
                )?;
                state.conditions[out] = state.local_conditions[local].clone();
            }
            if matches!(i.kind, InstructionKind::TakeLocal { .. }) {
                state.local_aliases[local] = OwnerAliases::default();
                state.local_queries[local] = None;
                state.local_tags[local] = None;
                state.local_conditions[local] = None;
                state.local_integers[local] = None;
            }
        }
        InstructionKind::MakeOwnedArray { elements } => {
            if let Some(out) = result
                && let Some(Type::Array { element, length }) = f.values.get(out)
                && matches!(element.as_ref(), Type::Closure(_)) && *length as usize == elements.len() {
                for (index, value) in elements.iter().enumerate() {
                    b.type_work(element, false, "closed array actual child type")?;
                    if f.values.get(value.0 as usize) != Some(element.as_ref()) {
                        return Err(fail(i.span, "closed array initializer changes its exact child type"));
                    }
                    let source = state.aliases.get(value.0 as usize)
                        .ok_or_else(|| fail(i.span, "closed array initializer has no actual SSA"))?.copy(b)?;
                    state.aliases[out].append_under(&[PlaceProjection::Field(index as u32)], &source, b)?;
                }
            }
        }
        InstructionKind::TakeOwnedArrayElement { local, index }
        | InstructionKind::BorrowOwnedArrayElement { local, index, .. } => {
            let Some(Type::Array { element, length }) = f.locals.get(local.0 as usize).map(|local| &local.ty) else {
                return Err(fail(i.span, "closed element read loses its actual array local"));
            };
            if !matches!(element.as_ref(), Type::Closure(_)) { return Ok(()); }
            if *index >= *length {
                return Err(fail(i.span, "closed element read changes its child ordinal"));
            }
            let source = state.local_aliases[local.0 as usize].project(&[PlaceProjection::Field(*index)], b)?;
            if matches!(i.kind, InstructionKind::BorrowOwnedArrayElement { mutable: true, .. })
                && let Some(token) = source.root() { state.expose_mutable_head(token, b)?; }
            if let Some(out) = result { state.aliases[out] = source; }
            if matches!(i.kind, InstructionKind::TakeOwnedArrayElement { .. }) {
                state.local_aliases[local.0 as usize].set_element(*index, ElementAlias::Vacant, b)?;
            }
        }
        InstructionKind::StoreOwnedArrayElement { local, index, value } => {
            let Some(Type::Array { element, length }) = f.locals.get(local.0 as usize).map(|local| &local.ty) else {
                return Err(fail(i.span, "closed element refill loses its actual array local"));
            };
            b.type_work(element, false, "closed array refill actual type")?;
            if !matches!(element.as_ref(), Type::Closure(_)) { return Ok(()); }
            if *index >= *length || f.values.get(value.0 as usize) != Some(element.as_ref()) {
                return Err(fail(i.span, "closed element refill changes its exact type or selector"));
            }
            let token = state.aliases.get(value.0 as usize).and_then(OwnerAliases::root);
            if let ElementAlias::Owner(previous) | ElementAlias::ConditionalOwner(previous, _) =
                state.local_aliases[local.0 as usize].element(*index, b)?
                && token != Some(*previous) { state.retire(*previous, b)?; }
            state.local_aliases[local.0 as usize].set_element(*index,
                token.map_or(ElementAlias::Unknown, ElementAlias::Owner), b)?;
        }
        InstructionKind::DropOwnedArrayElement { local, index } => {
            if !matches!(f.locals.get(local.0 as usize).map(|local| &local.ty),
                Some(Type::Array { element, length }) if *index < *length && matches!(element.as_ref(), Type::Closure(_))) {
                return Ok(());
            }
            let aliases = state.local_aliases.get(local.0 as usize)
                .ok_or_else(|| fail(i.span, "closed element cleanup has no actual local"))?;
            if let ElementAlias::Owner(token) | ElementAlias::ConditionalOwner(token, _) = aliases.element(*index, b)? {
                state.retire(*token, b)?;
            }
            state.local_aliases[local.0 as usize].set_element(*index, ElementAlias::Vacant, b)?;
        }
        InstructionKind::TaskCreateOwned { arguments, .. } => {
            // Only the actual failure arm returns these precise operands.
            // No success Task alias or Closed fact is manufactured here.
            if let Some(out) = result
                && let Some(Type::ErrorUnion { error, .. }) = f.values.get(out)
                && crate::stored_flow::failure_shape(error, b)? {
                let fields = match error.as_ref() {
                    Type::Package { fields, .. } | Type::Record { fields, .. } => fields,
                    _ => return Err(fail(i.span, "closed failure carrier has no canonical actual fields")),
                };
                let Some((_, Type::Tuple(inputs))) = fields.get(1) else {
                    return Err(fail(i.span, "closed failure carrier loses its full actual input tuple"));
                };
                if inputs.len() != arguments.len() { return Err(fail(i.span, "closed failure inputs change actual arity")); }
                for (index, (input, value)) in inputs.iter().zip(arguments).enumerate() {
                    b.type_work(input, false, "closed failure complete actual input type")?;
                    if f.values.get(value.0 as usize) != Some(input) {
                        return Err(fail(i.span, "closed failure input changes its actual constructor source"));
                    }
                    let aliases = state.aliases.get(value.0 as usize)
                        .ok_or_else(|| fail(i.span, "closed failure input SSA absent"))?.copy(b)?;
                    state.aliases[out].append_under(&[PlaceProjection::ErrorArm(true),
                        PlaceProjection::Field(1), PlaceProjection::Field(index as u32)], &aliases, b)?;
                }
            } else { state.forget_effects(b)?; }
        }
        InstructionKind::ExtractField { aggregate, index } => {
            if let Some(out) = result {
                state.aliases[out] = state.aliases.get(aggregate.0 as usize)
                    .ok_or_else(|| fail(i.span, "closed aggregate source SSA absent"))?
                    .project(&[PlaceProjection::Field(*index)], b)?;
            }
        }
        InstructionKind::ReadErrorUnion { value, is_error }
        | InstructionKind::TakeErrorUnion { value, is_error } => {
            if let Some(out) = result {
                state.aliases[out] = state.aliases.get(value.0 as usize)
                    .ok_or_else(|| fail(i.span, "closed error source SSA absent"))?
                    .project(&[PlaceProjection::ErrorArm(*is_error)], b)?;
            }
        }
        InstructionKind::ReadPackageField { local, index }
        | InstructionKind::TakePackageField { local, index } => {
            let prefix = [PlaceProjection::Field(*index)];
            if let Some(out) = result {
                state.aliases[out] = state.local_aliases.get(local.0 as usize)
                    .ok_or_else(|| fail(i.span, "closed field source local absent"))?.project(&prefix, b)?;
            }
            if matches!(i.kind, InstructionKind::TakePackageField { .. }) {
                state.local_aliases[local.0 as usize].remove_under(&prefix, b)?;
            }
        }
        InstructionKind::TakeTupleField { local, path }
        | InstructionKind::TakeVariantField { local, path } => {
            if let Some(out) = result {
                state.aliases[out] = state.local_aliases.get(local.0 as usize)
                    .ok_or_else(|| fail(i.span, "closed projected source local absent"))?.project(path, b)?;
            }
            state.local_aliases[local.0 as usize].remove_under(path, b)?;
        }
        InstructionKind::Store { local, value } => {
            let target = local.0 as usize;
            let source = value.0 as usize;
            if target >= state.local_aliases.len() || source >= state.aliases.len() {
                return Err(fail(i.span, "closed store arena mismatch"));
            }
            let alias = state.aliases[source].copy(b)?;
            for previous in state.local_aliases[target].tokens(b)? {
                if !alias.contains(previous, b)? { state.retire(previous, b)?; }
            }
            state.local_aliases[target] = alias;
            state.local_queries[target] = clone_query(&state.queries[source], b)?;
            state.local_tags[target] = clone_query(&state.tags[source], b)?;
            state.local_integers[target] = state.integers[source];
            b.spend(
                state.conditions[source]
                    .as_ref()
                    .map_or(1, |c| c.query.leaves.len().saturating_add(2)),
                "closed stored condition",
            )?;
            state.local_conditions[target] = state.conditions[source].clone();
        }
        InstructionKind::ReborrowAddress { address, .. } => {
            if let Some(out) = result {
                state.aliases[out] = state.aliases.get(address.0 as usize)
                    .map(|alias| alias.copy(b)).transpose()?.unwrap_or_default();
            }
        }
        InstructionKind::TaskPollClose { task } => {
            if let Some(out) = result
                && f.values.get(out) == Some(&super::super::task_poll_type(&Type::Unit))
                && let Some(token) = state.aliases.get(task.0 as usize).and_then(OwnerAliases::root)
            {
                b.spend(1, "canonical close result actual occupant")?;
                state.queries[out] = Some(Query {
                    occupant: token,
                    leaves: BTreeSet::from([CLOSED_TASK]),
                });
            }
        }
        InstructionKind::MakeVariant { variant: 0, fields }
            if fields.is_empty()
                && result.is_some_and(|out| {
                    f.values.get(out) == Some(&super::super::task_poll_type(&Type::Unit))
                }) =>
        {
            // This exit cannot return Ready. It supplies no Closed fact; it
            // only leaves a Ready-conditional postcondition vacuous here.
            state.integers[result.expect("checked canonical Pending")] = Some(0);
        }
        InstructionKind::VariantTag { value } => {
            if let Some(out) = result {
                state.tags[out] =
                    clone_query(state.queries.get(value.0 as usize).unwrap_or(&None), b)?;
            }
        }
        InstructionKind::Constant(Constant::I32(n)) => {
            if let Some(out) = result {
                state.integers[out] = Some(*n);
            }
        }
        InstructionKind::Binary {
            op: BinaryOp::Equal,
            left,
            right,
        } => {
            if let Some(out) = result {
                for (tag, number) in [(*left, *right), (*right, *left)] {
                    if let Some(query) = state.tags.get(tag.0 as usize).and_then(Option::as_ref)
                        && let Some(number @ (0 | 1)) =
                            state.integers.get(number.0 as usize).copied().flatten()
                    {
                        b.spend(
                            query.leaves.len().saturating_add(2),
                            "canonical Ready branch relation",
                        )?;
                        state.conditions[out] = Some(Condition {
                            query: query.clone(),
                            ready_if: number == 1,
                        });
                        break;
                    }
                }
            }
        }
        InstructionKind::CallClosureMut { callee, .. } if !body => {
            let token = state.local_aliases.get(callee.0 as usize).and_then(OwnerAliases::root);
            let id = f.locals.get(callee.0 as usize).and_then(|l| {
                if let Type::Closure(id) = l.ty {
                    Some(id)
                } else {
                    None
                }
            });
            if let (Some(token), Some(id)) = (token, id) {
                if let Some(proof) = exits(module, id, b)? {
                    state.add_closed(token, &proof.always, b)?;
                    if let Some(out) = result
                        && !proof.ready.is_empty()
                    {
                        state.queries[out] = Some(Query {
                            occupant: token,
                            leaves: proof.ready,
                        });
                    }
                } else {
                    state.forget_effects(b)?;
                }
            } else {
                state.forget_effects(b)?;
            }
        }
        InstructionKind::CallClosureBorrowed { callee, kind, .. } if !body => {
            let token = state.aliases.get(callee.0 as usize).and_then(OwnerAliases::root);
            let id = match f.values.get(callee.0 as usize) {
                Some(Type::Address { mutable, inner }) if *kind == CallableKind::Fn
                    || (*kind == CallableKind::FnMut && *mutable) => match inner.as_ref() {
                        Type::Closure(id) => Some(*id), _ => None,
                    },
                _ => None,
            };
            if let (Some(token), Some(id)) = (token, id)
                && let Some(proof) = exits(module, id, b)? {
                state.add_closed(token, &proof.always, b)?;
                if let Some(out) = result && !proof.ready.is_empty() {
                    state.queries[out] = Some(Query { occupant: token, leaves: proof.ready });
                }
            } else { state.forget_effects(b)?; }
        }
        InstructionKind::DropClosure { local }
        | InstructionKind::DropOwnedArray { local }
        | InstructionKind::StorageDead(local)
        | InstructionKind::StorageLive(local) => {
            if let Some(alias) = state.local_aliases.get(local.0 as usize) {
                for token in alias.tokens(b)? { state.retire(token, b)?; }
            }
            if let Some(alias) = state.local_aliases.get_mut(local.0 as usize) {
                *alias = OwnerAliases::default();
            }
            if let Some(query) = state.local_queries.get_mut(local.0 as usize) {
                *query = None;
            }
            if let Some(tag) = state.local_tags.get_mut(local.0 as usize) {
                *tag = None;
            }
            if let Some(condition) = state.local_conditions.get_mut(local.0 as usize) {
                *condition = None;
            }
            if let Some(integer) = state.local_integers.get_mut(local.0 as usize) {
                *integer = None;
            }
        }
        InstructionKind::CommitPlaceReplace { .. }
        | InstructionKind::WriteAddress { .. }
        | InstructionKind::StoreStoredBorrow { .. }
        | InstructionKind::StorePackageField { .. } => {
            state.forget_effects(b)?;
            b.spend(state.work(), "unmodelled replacement invalidates whole and child occupant identities")?;
            for alias in state.aliases.iter_mut().chain(&mut state.local_aliases) {
                *alias = OwnerAliases::default();
            }
        }
        InstructionKind::Call { .. }
        | InstructionKind::SuspendCall { .. }
        | InstructionKind::CallIndirect { .. }
        | InstructionKind::ForeignCall { .. }
        | InstructionKind::CallClosure { .. }
        | InstructionKind::CallClosureMut { .. }
        | InstructionKind::CallClosureShared { .. }
        | InstructionKind::CallClosureOnce { .. }
        | InstructionKind::CallClosureBorrowed { .. }
        | InstructionKind::WithForeignCallback { .. }
        | InstructionKind::WithScopedThread { .. }
        | InstructionKind::WithSyncCell { .. } => {
            state.forget_effects(b)?;
            if let Some(out) = result
                && matches!(f.values.get(out), Some(Type::Closure(_)))
            {
                let token = Token::Value(i.result.expect("checked result"));
                state.retire(token, b)?;
                state.aliases[out] = OwnerAliases::owner(token);
            }
        }
        InstructionKind::MakeClosure { .. } => {
            if let Some(out) = result {
                let token = Token::Value(i.result.expect("checked result"));
                state.retire(token, b)?;
                state.aliases[out] = OwnerAliases::owner(token);
            }
        }
        _ => {}
    }
    Ok(())
}

fn run(
    module: &TypedModule,
    f: &TypedFunction,
    body: bool,
    b: &mut Budget,
) -> CompileResult<Vec<Option<Flow>>> {
    b.spend(f.blocks.len().saturating_mul(3), "closed CFG work frontier")?;
    let mut incoming = vec![None; f.blocks.len()];
    let entry = f.entry.0 as usize;
    if entry >= incoming.len() {
        return Err(fail(f.span, "closed CFG entry absent"));
    }
    incoming[entry] = Some(Flow::new(f, b)?);
    let mut queue = std::collections::VecDeque::from([entry]);
    let mut queued = vec![false; f.blocks.len()];
    queued[entry] = true;
    while let Some(block) = queue.pop_front() {
        queued[block] = false;
        let mut state = incoming[block]
            .as_ref()
            .expect("queued actual state")
            .snapshot(b)?;
        for instruction in &f.blocks[block].instructions {
            step(module, f, &mut state, instruction, body, b)?;
        }
        let targets = match f.blocks[block].terminator {
            Terminator::Jump(next) => [(Some(next), None), (None, None)],
            Terminator::Branch {
                condition,
                then_block,
                else_block,
            } => [
                (Some(then_block), Some((condition, true))),
                (Some(else_block), Some((condition, false))),
            ],
            _ => [(None, None), (None, None)],
        };
        for (target, select) in targets {
            let Some(target) = target else {
                continue;
            };
            let target = target.0 as usize;
            if target >= incoming.len() {
                return Err(fail(f.span, "closed CFG successor absent"));
            }
            let mut next = state.snapshot(b)?;
            if let Some((condition, yes)) = select
                && let Some(relation) = state
                    .conditions
                    .get(condition.0 as usize)
                    .and_then(Option::as_ref)
                && yes == relation.ready_if
            {
                next.add_closed(relation.query.occupant, &relation.query.leaves, b)?;
            }
            let changed = if let Some(previous) = &mut incoming[target] {
                previous.merge(&next, b)?
            } else {
                incoming[target] = Some(next);
                true
            };
            if changed && !queued[target] {
                queued[target] = true;
                queue.push_back(target);
            }
        }
    }
    Ok(incoming)
}

fn exits(module: &TypedModule, id: ClosureId, b: &mut Budget) -> CompileResult<Option<ExitProof>> {
    let Some(tasks) = inventory(module, id, b)? else {
        return Ok(None);
    };
    let d = &module.closures[id.0 as usize];
    let f = &module.functions[d.function.0 as usize];
    if !entry_is_stable(f, &tasks, b)? {
        return Ok(None);
    }
    let incoming = run(module, f, true, b)?;
    let mut proof = ExitProof {
        always: tasks.clone(),
        ready: tasks.clone(),
    };
    b.spend(
        tasks.len().saturating_mul(2),
        "closed callable exit candidate masks",
    )?;
    let mut observed = false;
    for (index, row) in f.blocks.iter().enumerate() {
        let Some(before) = &incoming[index] else {
            continue;
        };
        let Terminator::Return(value) = row.terminator else {
            continue;
        };
        observed = true;
        let mut state = before.snapshot(b)?;
        for instruction in &row.instructions {
            step(module, f, &mut state, instruction, true, b)?;
        }
        for &capture in &tasks {
            b.spend(
                state.closed.len().saturating_add(1),
                "closed captured task exit obligation",
            )?;
            let closed = state
                .closed
                .get(&Token::Capture(capture))
                .is_some_and(|leaves| leaves.contains(&CLOSED_TASK));
            if !closed {
                proof.always.remove(&capture);
                let related = value
                    .and_then(|v| state.queries.get(v.0 as usize))
                    .and_then(Option::as_ref)
                    .is_some_and(|q| {
                        q.occupant == Token::Capture(capture) && q.leaves.contains(&CLOSED_TASK)
                    });
                let pending = value.is_some_and(|value| {
                    f.values.get(value.0 as usize)
                        == Some(&super::super::task_poll_type(&Type::Unit))
                        && state.integers.get(value.0 as usize) == Some(&Some(0))
                });
                if !related && !pending {
                    proof.ready.remove(&capture);
                }
            }
        }
    }
    if d.result != super::super::task_poll_type(&Type::Unit) {
        proof.ready.clear();
    }
    Ok(observed.then_some(proof))
}

pub(super) fn allow_at(
    module: &TypedModule,
    f: &TypedFunction,
    local: LocalId,
    at: Point,
    b: &mut Budget,
) -> CompileResult<bool> {
    let Some(declaration) = f.locals.get(local.0 as usize).filter(|entry| entry.id == local) else {
        return Ok(false);
    };
    let (closure, elements) = match &declaration.ty {
        Type::Closure(id) => (*id, None),
        Type::Array { element, length } => {
            let Type::Closure(id) = element.as_ref() else { return Ok(false); };
            let Some(operation) = f.blocks.get(at.0).and_then(|block| block.instructions.get(at.1)) else {
                return Ok(false);
            };
            let range = match operation.kind {
                InstructionKind::DropOwnedArrayElement { local: actual, index } if actual == local && index < *length =>
                    index..index + 1,
                InstructionKind::DropOwnedArray { local: actual } if actual == local => 0..*length,
                _ => return Ok(false),
            };
            (*id, Some(range))
        }
        _ => return Ok(false),
    };
    let Some(tasks) = inventory(module, closure, b)? else { return Ok(false); };
    let incoming = run(module, f, false, b)?;
    let Some(before) = incoming.get(at.0).and_then(Option::as_ref) else {
        return Ok(false);
    };
    let Some(row) = f.blocks.get(at.0) else {
        return Ok(false);
    };
    if at.1 > row.instructions.len() {
        return Ok(false);
    }
    let mut state = before.snapshot(b)?;
    for instruction in &row.instructions[..at.1] {
        step(module, f, &mut state, instruction, false, b)?;
    }
    let Some(aliases) = state.local_aliases.get(local.0 as usize) else { return Ok(false); };
    let mut proved_owner = false;
    let mut prove = |alias: &ElementAlias, b: &mut Budget| -> CompileResult<bool> {
        b.spend(tasks.len().saturating_add(state.closed.len()).saturating_add(1),
            "closed drop retains every actual selected child's task leaf obligation")?;
        match alias {
            ElementAlias::Unknown => Ok(false),
            ElementAlias::Vacant => Ok(true),
            ElementAlias::Owner(token) => {
                proved_owner = true;
                Ok(tasks.is_empty() || state.closed.get(token)
                    .is_some_and(|leaves| tasks.iter().all(|capture| leaves.contains(capture))))
            }
            ElementAlias::ConditionalOwner(token, leaves) => {
                proved_owner = true;
                b.lookup(state.closed.len(), "closed current universal occupant fact")?;
                let universal = state.closed.get(token);
                // A new actual call can establish a universal fact after the
                // conditional join. Its must-fact adds no permission on an
                // absent branch and uses this exact token, never another head.
                for capture in &tasks {
                    b.lookup(leaves.len(), "closed selector-conditional leaf")?;
                    if !leaves.contains(capture) {
                        b.lookup(universal.map_or(0, BTreeSet::len), "closed universal actual leaf")?;
                        if !universal.is_some_and(|leaves| leaves.contains(capture)) { return Ok(false); }
                    }
                }
                Ok(true)
            }
        }
    };
    if let Some(elements) = elements {
        for index in elements {
            let alias = aliases.element(index, b)?;
            if !prove(alias, b)? { return Ok(false); }
        }
    } else if !prove(&aliases.root().map_or(ElementAlias::Unknown, ElementAlias::Owner), b)? {
        return Ok(false);
    }
    // A completely vacant shell is decided by the actual resource ledger,
    // never promoted here into a Closed theorem for a missing occupant.
    Ok(proved_owner)
}

/// Applied only after this actual Direct entry has returned normally to a
/// consuming invocation. Conditional Ready facts cannot discharge this cleanup.
pub(super) fn after_once(
    module: &TypedModule,
    id: ClosureId,
    b: &mut Budget,
) -> CompileResult<bool> {
    if !b.task_origin_roots { return Ok(false); }
    let Some(tasks) = inventory(module, id, b)? else { return Ok(false); };
    let Some(proof) = exits(module, id, b)? else { return Ok(false); };
    b.spend(tasks.len().saturating_add(proof.always.len()), "actual post-body remaining closed leaves")?;
    Ok(tasks.is_subset(&proof.always))
}
