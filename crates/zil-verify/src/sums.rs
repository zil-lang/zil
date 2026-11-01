//! Independent ordinary Sum admission, tag facts and deep source transport.
//! Nothing in this module reads producer responsibility/tag annotations.
use super::*;
use std::collections::{BTreeMap, VecDeque};

pub(super) fn owned_enum(ty: &Type) -> bool {
    matches!(ty, Type::Enum { .. }) && !ty.is_copy_value()
}

pub(super) fn owned(ty: &Type) -> bool {
    matches!(ty, Type::Enum { .. } | Type::ErrorUnion { .. }) && !ty.is_copy_value()
}

fn bad(span: Option<Span>, message: impl Into<String>) -> Diagnostic {
    reject("E-VERIFY-SUM", span, message)
}

/// Used only after the paid raw-tree pass has detected a new Sum profile.
pub(super) fn needs_shape(ty: &Type, budget: &mut Budget) -> Check<bool> {
    if !budget.sums {
        return Ok(false);
    }
    resources::charge_payload(ty, budget)?;
    let mut pending = vec![(ty, false, false)];
    while let Some((ty, in_error, in_product)) = pending.pop() {
        match ty {
            Type::Enum { .. } if owned_enum(ty) => return Ok(true),
            Type::ErrorUnion { error, result } => {
                if in_product && owned(ty) {
                    return Ok(true);
                }
                if **error != Type::Unit
                    && **result != Type::Unit
                    && (packages::has_children(error) || packages::has_children(result))
                {
                    return Ok(true);
                }
                pending.push((error, true, in_product));
                pending.push((result, true, in_product));
            }
            Type::Package { fields, .. } => {
                pending.extend(fields.iter().map(|(_, ty)| (ty, in_error, true)));
            }
            Type::Managed(payload)
                if in_error && matches!(payload.as_ref(), Type::OwnedStore { .. }) =>
            {
                return Ok(true);
            }
            _ => {}
        }
    }
    Ok(false)
}

pub(super) fn check_type_for(
    module: &TypedModule,
    ty: &Type,
    span: Option<Span>,
    budget: &mut Budget,
) -> Check<()> {
    resources::charge_payload(ty, budget)?;
    let mut pending = vec![ty];
    while let Some(ty) = pending.pop() {
        budget.spend(1)?;
        if ty.is_copy_value() {
            check_type(ty, span)?;
            continue;
        }
        match ty {
            Type::Managed(_) => resources::check_type_for(module, ty, span, budget)?,
            Type::Package { name, fields } => {
                budget.spend(name.len().saturating_add(fields.len().saturating_mul(2)))?;
                let mut names = BTreeSet::new();
                if name.is_empty() || fields.is_empty() {
                    return Err(bad(
                        span,
                        "ordinary Product requires a nonempty nominal identity and owning fields",
                    ));
                }
                let mut owns = false;
                for (name, field) in fields {
                    budget.spend(name.len().saturating_mul(fields.len().saturating_add(1)))?;
                    if name.is_empty() || !names.insert(name) {
                        return Err(bad(span, "Product fields must have unique nonempty names"));
                    }
                    owns |=
                        matches!(field, Type::Managed(_) | Type::Package { .. }) || owned(field);
                    pending.push(field);
                }
                if !owns {
                    return Err(bad(span, "ordinary Product requires a real owning child"));
                }
            }
            Type::Enum { name, variants } => {
                budget.spend(name.len().saturating_add(variants.len().saturating_mul(2)))?;
                let mut names = BTreeSet::new();
                if name.is_empty() || variants.is_empty() {
                    return Err(bad(
                        span,
                        "owned Enum requires a nonempty nominal identity and variants",
                    ));
                }
                for (name, fields) in variants {
                    budget.spend(
                        name.len()
                            .saturating_mul(variants.len().saturating_add(1))
                            .saturating_add(fields.len()),
                    )?;
                    if name.is_empty() || !names.insert(name) {
                        return Err(bad(span, "Enum variants must have unique nonempty names"));
                    }
                    pending.extend(fields);
                }
            }
            Type::ErrorUnion { error, result } => {
                budget.spend(2)?;
                pending.push(error);
                pending.push(result);
            }
            _ => {
                return Err(bad(
                    span,
                    "ordinary Sum payload must be Copy, canonical Managed, Product or finite Sum",
                ));
            }
        }
    }
    Ok(())
}

fn projected_type<'a>(
    f: &'a TypedFunction,
    local: LocalId,
    path: &[VariantProjection],
    span: Option<Span>,
    budget: &mut Budget,
) -> Check<&'a Type> {
    let mut ty = local_type(f, local, span)?;
    budget.spend(path.len().saturating_add(1))?;
    for projection in path {
        let Type::Enum { variants, .. } = ty else {
            return Err(bad(
                span,
                "VariantProjection may only traverse an actual Enum",
            ));
        };
        ty = variants
            .get(projection.variant as usize)
            .and_then(|(_, fields)| fields.get(projection.index as usize))
            .ok_or_else(|| bad(span, "VariantProjection index is outside its exact Enum"))?;
    }
    Ok(ty)
}

pub(super) fn check_instruction(
    f: &TypedFunction,
    i: &Instruction,
    limits: &Limits,
    budget: &mut Budget,
) -> Check<Option<Type>> {
    let span = Some(i.span);
    budget.sums = true;
    match &i.kind {
        InstructionKind::VariantTagLocal { local, path }
        | InstructionKind::ReadVariantLocal { local, path } => {
            if path.len() > limits.max_call_depth {
                return Err(limit("Sum projection depth exceeds the selected limit"));
            }
            let ty = projected_type(f, *local, path, span, budget)?;
            if matches!(i.kind, InstructionKind::VariantTagLocal { .. }) {
                if !matches!(ty, Type::Enum { .. }) {
                    return Err(bad(span, "TagLocal projection must end at an Enum"));
                }
                Ok(Some(Type::I32))
            } else {
                if path.is_empty() || !ty.is_copy_value() {
                    return Err(bad(
                        span,
                        "ReadVariantLocal requires a nonempty path ending at finite Copy",
                    ));
                }
                resources::charge_payload(ty, budget)?;
                check_type(ty, span)?;
                Ok(Some(ty.clone()))
            }
        }
        InstructionKind::UnpackVariant {
            local,
            variant,
            fields,
        } => {
            let ty = local_type(f, *local, span)?;
            let Type::Enum { variants, .. } = ty else {
                return Err(bad(span, "UnpackVariant requires an owned Enum local"));
            };
            if !owned_enum(ty) {
                return Err(bad(span, "UnpackVariant requires a non-Copy Enum"));
            }
            let payload = variants
                .get(*variant as usize)
                .ok_or_else(|| bad(span, "Unpack variant is out of bounds"))?;
            budget.spend(fields.len().saturating_mul(fields.len().saturating_add(2)))?;
            if fields.len() != payload.1.len() {
                return Err(bad(
                    span,
                    "Unpack must initialize every selected payload field",
                ));
            }
            let mut destinations = BTreeSet::new();
            for (target, expected) in fields.iter().zip(&payload.1) {
                resources::charge_payload(expected, budget)?;
                if *target == *local
                    || !destinations.insert(*target)
                    || local_type(f, *target, span)? != expected
                {
                    return Err(bad(
                        span,
                        "Unpack targets must be distinct from the source and each other, with exact field Types",
                    ));
                }
            }
            Ok(None)
        }
        InstructionKind::DropEnum { local } => {
            if !owned_enum(local_type(f, *local, span)?) {
                return Err(bad(span, "DropEnum requires an owned Enum local"));
            }
            Ok(None)
        }
        _ => unreachable!("Sum opcode dispatch"),
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Source {
    Fresh,
    Input(u32, Vec<u32>),
    Unknown,
}

/// Tags are per carrier; alternatives are not cross-multiplied with other Sums.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Content {
    Copy,
    Leaf(BTreeSet<Source>),
    Product(Vec<Content>),
    Sum(BTreeMap<u32, Vec<Content>>),
}

impl Content {
    fn cost(&self, budget: &mut Budget) -> Check<()> {
        budget.spend(1)?;
        match self {
            Self::Leaf(sources) => {
                budget.spend(sources.len())?;
                for source in sources {
                    if let Source::Input(_, path) = source {
                        budget.spend(path.len())?;
                    }
                }
            }
            Self::Product(fields) => {
                budget.spend(fields.len())?;
                for field in fields {
                    field.cost(budget)?;
                }
            }
            Self::Sum(variants) => {
                budget.spend(variants.len())?;
                for fields in variants.values() {
                    budget.spend(fields.len())?;
                    for field in fields {
                        field.cost(budget)?;
                    }
                }
            }
            Self::Copy => {}
        }
        Ok(())
    }
    fn duplicate(&self, budget: &mut Budget) -> Check<Self> {
        self.cost(budget)?;
        Ok(self.clone())
    }
    fn merge(&mut self, other: &Self, budget: &mut Budget) -> Check<bool> {
        other.cost(budget)?;
        let mut changed = false;
        match (self, other) {
            (Self::Leaf(a), Self::Leaf(b)) => {
                let old = a.len();
                a.extend(b.iter().cloned());
                changed = old != a.len();
            }
            (Self::Product(a), Self::Product(b)) => {
                for (a, b) in a.iter_mut().zip(b) {
                    changed |= a.merge(b, budget)?;
                }
            }
            (Self::Sum(a), Self::Sum(b)) => {
                for (&tag, fields) in b {
                    if let Some(target) = a.get_mut(&tag) {
                        for (a, b) in target.iter_mut().zip(fields) {
                            changed |= a.merge(b, budget)?;
                        }
                    } else {
                        for field in fields {
                            field.cost(budget)?;
                        }
                        budget.spend(fields.len())?;
                        a.insert(tag, fields.clone());
                        changed = true;
                    }
                }
            }
            (Self::Copy, Self::Copy) => {}
            _ => return Err(bad(None, "Sum source shapes differ at transfer")),
        }
        Ok(changed)
    }
    fn fresh(&self) -> u8 {
        match self {
            Self::Leaf(sources)
                if !sources.is_empty() && sources.iter().all(|s| *s == Source::Fresh) =>
            {
                1
            }
            _ => 2,
        }
    }
}

fn content(
    ty: &Type,
    parameter: Option<u32>,
    path: &mut Vec<u32>,
    bottom: bool,
    budget: &mut Budget,
) -> Check<Content> {
    budget.spend(path.len().saturating_add(2))?;
    Ok(match ty {
        Type::Managed(_) => Content::Leaf(if bottom {
            BTreeSet::new()
        } else {
            BTreeSet::from([parameter.map_or(Source::Unknown, |p| Source::Input(p, path.clone()))])
        }),
        Type::Package { fields, .. } => {
            budget.spend(fields.len())?;
            let mut children = Vec::with_capacity(fields.len());
            for (index, (_, ty)) in fields.iter().enumerate() {
                path.extend([0, index as u32]);
                children.push(content(ty, parameter, path, bottom, budget)?);
                path.truncate(path.len() - 2);
            }
            Content::Product(children)
        }
        Type::Enum { variants, .. } => {
            budget.spend(variants.len())?;
            let mut alternatives = BTreeMap::new();
            for (variant, (_, fields)) in variants.iter().enumerate() {
                budget.spend(fields.len())?;
                let mut children = Vec::with_capacity(fields.len());
                for (index, ty) in fields.iter().enumerate() {
                    path.extend([1, variant as u32, index as u32]);
                    children.push(content(ty, parameter, path, bottom, budget)?);
                    path.truncate(path.len() - 3);
                }
                if !bottom {
                    alternatives.insert(variant as u32, children);
                }
            }
            Content::Sum(alternatives)
        }
        Type::ErrorUnion { error, result } => {
            budget.spend(2)?;
            let mut alternatives = BTreeMap::new();
            for (variant, ty) in [(0, error.as_ref()), (1, result.as_ref())] {
                path.extend([1, variant, 0]);
                let child = content(ty, parameter, path, bottom, budget)?;
                path.truncate(path.len() - 3);
                if !bottom {
                    alternatives.insert(variant, vec![child]);
                }
            }
            Content::Sum(alternatives)
        }
        _ => Content::Copy,
    })
}

type Prefix = Vec<(u32, u32)>;
type Fact = (LocalId, Prefix, u32);
#[derive(Clone, PartialEq, Eq)]
struct State {
    initialized: Vec<u8>, // 1 vacant, 2 initialized; join is may-union.
    locals: Vec<Content>,
    values: Vec<Content>,
    facts: BTreeSet<Fact>,
    observations: BTreeSet<ValueId>,
}

impl State {
    fn cost(&self, budget: &mut Budget) -> Check<()> {
        budget.spend(
            self.initialized
                .len()
                .saturating_add(self.locals.len())
                .saturating_add(self.values.len())
                .saturating_add(self.observations.len())
                .saturating_add(self.facts.len()),
        )?;
        for c in self.locals.iter().chain(&self.values) {
            c.cost(budget)?;
        }
        for (_, path, _) in &self.facts {
            budget.spend(path.len())?;
        }
        Ok(())
    }
    fn duplicate(&self, budget: &mut Budget) -> Check<Self> {
        self.cost(budget)?;
        Ok(self.clone())
    }
    fn merge(&mut self, other: &Self, budget: &mut Budget) -> Check<bool> {
        other.cost(budget)?;
        let mut changed = false;
        for (a, b) in self.initialized.iter_mut().zip(&other.initialized) {
            let old = *a;
            *a |= *b;
            changed |= old != *a;
        }
        for (a, b) in self
            .locals
            .iter_mut()
            .zip(&other.locals)
            .chain(self.values.iter_mut().zip(&other.values))
        {
            changed |= a.merge(b, budget)?;
        }
        let old = self.facts.len();
        self.facts.retain(|f| other.facts.contains(f));
        changed |= old != self.facts.len();
        let old = self.observations.len();
        self.observations.retain(|v| other.observations.contains(v));
        changed |= old != self.observations.len();
        Ok(changed)
    }
}

#[derive(Default)]
pub(super) struct Proofs {
    pub(super) rows: Vec<Vec<Vec<Bridge>>>,
}
#[derive(Clone, Default)]
pub(super) struct Bridge {
    pub(super) result: Option<u8>,
    pub(super) package: Vec<(usize, u8)>,
    pub(super) unpack: Vec<(LocalId, u8, Vec<(usize, u8)>)>,
}

fn direct_fields(value: &Content) -> Vec<(usize, u8)> {
    match value {
        Content::Product(fields) => fields
            .iter()
            .enumerate()
            .filter_map(|(i, c)| matches!(c, Content::Leaf(_)).then_some((i, c.fresh())))
            .collect(),
        _ => Vec::new(),
    }
}

struct Flow<'a> {
    f: &'a TypedFunction,
    module: &'a TypedModule,
    graph: &'a FunctionGraph,
    definitions: Vec<Option<(usize, usize)>>,
    observers: Vec<Option<(LocalId, Prefix)>>,
    summaries: &'a [Content],
}

impl Flow<'_> {
    fn kill(&self, state: &mut State, local: LocalId, budget: &mut Budget) -> Check<()> {
        budget.spend(state.facts.len().saturating_add(state.observations.len()))?;
        state.facts.retain(|(root, _, _)| *root != local);
        state.observations.retain(|v| {
            self.observers[v.0 as usize]
                .as_ref()
                .is_none_or(|(root, _)| *root != local)
        });
        Ok(())
    }
    fn selected<'a>(
        &self,
        state: &'a State,
        local: LocalId,
        path: &[VariantProjection],
        last: Option<u32>,
        validate: bool,
        span: Span,
        budget: &mut Budget,
    ) -> Check<&'a Content> {
        let mut current = &state.locals[local.0 as usize];
        let mut prefix = Vec::new();
        budget.spend(path.len().saturating_mul(path.len().saturating_add(2)))?;
        if validate && state.initialized[local.0 as usize] != 2 {
            return Err(bad(
                Some(span),
                "Sum observation/consume requires a complete live current local",
            ));
        }
        for step in path {
            self.require_tag(
                state,
                local,
                &prefix,
                current,
                step.variant,
                validate,
                span,
                budget,
            )?;
            let Content::Sum(variants) = current else {
                return Err(bad(Some(span), "Sum path is not an Enum"));
            };
            let Some(fields) = variants.get(&step.variant) else {
                return Ok(current);
            };
            current = &fields[step.index as usize];
            prefix.push((step.variant, step.index));
        }
        if let Some(tag) = last {
            self.require_tag(state, local, &prefix, current, tag, validate, span, budget)?;
        }
        Ok(current)
    }
    fn require_tag(
        &self,
        state: &State,
        local: LocalId,
        path: &Prefix,
        value: &Content,
        tag: u32,
        validate: bool,
        span: Span,
        budget: &mut Budget,
    ) -> Check<()> {
        budget.spend(
            path.len()
                .saturating_add(state.facts.len())
                .saturating_add(1),
        )?;
        let known = matches!(value,Content::Sum(variants) if variants.len()==1 && variants.contains_key(&tag));
        if validate && !known && !state.facts.contains(&(local, path.clone(), tag)) {
            return Err(bad(
                Some(span),
                "payload requires the exact selected tag of this current local and complete prefix on every incoming path",
            ));
        }
        Ok(())
    }
    fn edge(&self, state: &mut State, block: usize, next: usize, budget: &mut Budget) -> Check<()> {
        let Terminator::Branch {
            condition,
            then_block,
            else_block,
        } = self.f.blocks[block].terminator
        else {
            return Ok(());
        };
        if then_block == else_block || then_block.0 as usize != next {
            return Ok(());
        }
        let Some((b, i)) = self.definitions[condition.0 as usize] else {
            return Ok(());
        };
        let InstructionKind::Binary {
            op: BinaryOp::Equal,
            left,
            right,
        } = self.f.blocks[b].instructions[i].kind
        else {
            return Ok(());
        };
        for (tag, constant) in [(left, right), (right, left)] {
            if !state.observations.contains(&tag) {
                continue;
            }
            let Some((cb, ci)) = self.definitions[constant.0 as usize] else {
                continue;
            };
            let InstructionKind::Constant(Constant::I32(variant)) =
                self.f.blocks[cb].instructions[ci].kind
            else {
                continue;
            };
            if variant < 0 {
                continue;
            }
            if let Some((root, path)) = &self.observers[tag.0 as usize] {
                budget.spend(
                    path.len()
                        .saturating_add(state.facts.len())
                        .saturating_add(1),
                )?;
                state.facts.insert((*root, path.clone(), variant as u32));
            }
        }
        Ok(())
    }
    fn instantiate(
        &self,
        value: &Content,
        args: &[ValueId],
        state: &State,
        budget: &mut Budget,
    ) -> Check<Content> {
        value.cost(budget)?;
        Ok(match value {
            Content::Leaf(sources) => {
                let mut result = BTreeSet::new();
                for source in sources {
                    match source {
                        Source::Input(parameter, path) => {
                            budget.spend(path.len().saturating_add(1))?;
                            let mut source = &state.values[args[*parameter as usize].0 as usize];
                            let mut cursor = 0;
                            while cursor < path.len() {
                                match (path[cursor], source) {
                                    (0, Content::Product(fields)) => {
                                        source = &fields[path[cursor + 1] as usize];
                                        cursor += 2;
                                    }
                                    (1, Content::Sum(variants)) => {
                                        let Some(fields) = variants.get(&path[cursor + 1]) else {
                                            break;
                                        };
                                        source = &fields[path[cursor + 2] as usize];
                                        cursor += 3;
                                    }
                                    _ => break,
                                }
                            }
                            if cursor == path.len() {
                                if let Content::Leaf(origins) = source {
                                    budget.spend(origins.len())?;
                                    result.extend(origins.iter().cloned());
                                } else {
                                    result.insert(Source::Unknown);
                                }
                            } else {
                                result.insert(Source::Unknown);
                            }
                        }
                        source => {
                            result.insert(source.clone());
                        }
                    }
                }
                Content::Leaf(result)
            }
            Content::Product(fields) => Content::Product(
                fields
                    .iter()
                    .map(|c| self.instantiate(c, args, state, budget))
                    .collect::<Check<_>>()?,
            ),
            Content::Sum(variants) => Content::Sum(
                variants
                    .iter()
                    .map(|(&v, fields)| {
                        Ok((
                            v,
                            fields
                                .iter()
                                .map(|c| self.instantiate(c, args, state, budget))
                                .collect::<Check<_>>()?,
                        ))
                    })
                    .collect::<Check<_>>()?,
            ),
            Content::Copy => Content::Copy,
        })
    }
    fn block(
        &self,
        block: usize,
        state: &mut State,
        validate: bool,
        budget: &mut Budget,
    ) -> Check<Vec<Bridge>> {
        budget.spend(self.f.blocks[block].instructions.len())?;
        let mut bridges = Vec::new();
        for instruction in &self.f.blocks[block].instructions {
            budget.spend(1)?;
            let mut result = None;
            let mut bridge = Bridge::default();
            match &instruction.kind {
                InstructionKind::MakeManaged { .. }
                | InstructionKind::ByteBufferNew
                | InstructionKind::OwnedStoreNew { .. } => {
                    result = Some(Content::Leaf(BTreeSet::from([Source::Fresh])))
                }
                InstructionKind::MakePackage { fields } => {
                    budget.spend(fields.len())?;
                    result = Some(Content::Product(
                        fields
                            .iter()
                            .map(|v| state.values[v.0 as usize].duplicate(budget))
                            .collect::<Check<_>>()?,
                    ));
                }
                InstructionKind::MakeVariant { variant, fields } => {
                    budget.spend(fields.len())?;
                    result = Some(Content::Sum(BTreeMap::from([(
                        *variant,
                        fields
                            .iter()
                            .map(|v| state.values[v.0 as usize].duplicate(budget))
                            .collect::<Check<_>>()?,
                    )])));
                }
                InstructionKind::MakeErrorUnion { is_error, value } => {
                    result = Some(Content::Sum(BTreeMap::from([(
                        if *is_error { 0 } else { 1 },
                        vec![state.values[value.0 as usize].duplicate(budget)?],
                    )])))
                }
                InstructionKind::TakeErrorUnion { value, is_error } => {
                    if let Content::Sum(variants) = &state.values[value.0 as usize] {
                        if let Some(fields) = variants.get(&(if *is_error { 0 } else { 1 })) {
                            result = Some(fields[0].duplicate(budget)?);
                        }
                    }
                }
                InstructionKind::Store { local, value } => {
                    self.kill(state, *local, budget)?;
                    state.locals[local.0 as usize] =
                        state.values[value.0 as usize].duplicate(budget)?;
                    state.initialized[local.0 as usize] = 2;
                }
                InstructionKind::Load(local) | InstructionKind::TakeLocal { local } => {
                    result = Some(state.locals[local.0 as usize].duplicate(budget)?);
                    if matches!(instruction.kind, InstructionKind::TakeLocal { .. }) {
                        self.kill(state, *local, budget)?;
                        state.initialized[local.0 as usize] = 1;
                    }
                }
                InstructionKind::TakePackageField { local, index }
                | InstructionKind::ReadPackageField { local, index } => {
                    if let Content::Product(fields) = &state.locals[local.0 as usize] {
                        result = Some(fields[*index as usize].duplicate(budget)?);
                    }
                    self.kill(state, *local, budget)?;
                }
                InstructionKind::StorePackageField {
                    local,
                    index,
                    value,
                } => {
                    self.kill(state, *local, budget)?;
                    if let Content::Product(fields) = &mut state.locals[local.0 as usize] {
                        fields[*index as usize] =
                            state.values[value.0 as usize].duplicate(budget)?;
                    }
                }
                InstructionKind::VariantTagLocal { local, path }
                | InstructionKind::ReadVariantLocal { local, path } => {
                    self.selected(
                        state,
                        *local,
                        path,
                        None,
                        validate,
                        instruction.span,
                        budget,
                    )?;
                    if matches!(instruction.kind, InstructionKind::VariantTagLocal { .. }) {
                        budget.spend(state.observations.len().saturating_add(1))?;
                        state
                            .observations
                            .insert(instruction.result.expect("typed tag"));
                    }
                }
                InstructionKind::UnpackVariant {
                    local,
                    variant,
                    fields,
                } => {
                    self.selected(
                        state,
                        *local,
                        &[],
                        Some(*variant),
                        validate,
                        instruction.span,
                        budget,
                    )?;
                    budget.spend(fields.len())?;
                    if validate && fields.iter().any(|l| state.initialized[l.0 as usize] != 1) {
                        return Err(bad(
                            Some(instruction.span),
                            "every Unpack destination must be vacant on every incoming path before the atomic transfer",
                        ));
                    }
                    let payload = match &state.locals[local.0 as usize] {
                        Content::Sum(variants) => variants.get(variant),
                        _ => None,
                    };
                    let mut prepared = Vec::with_capacity(fields.len());
                    for (index, target) in fields.iter().enumerate() {
                        prepared.push(if let Some(payload) = payload {
                            payload[index].duplicate(budget)?
                        } else {
                            content(
                                &self.f.locals[target.0 as usize].ty,
                                None,
                                &mut Vec::new(),
                                false,
                                budget,
                            )?
                        });
                    }
                    self.kill(state, *local, budget)?;
                    state.initialized[local.0 as usize] = 1;
                    for (&target, value) in fields.iter().zip(prepared) {
                        self.kill(state, target, budget)?;
                        bridge
                            .unpack
                            .push((target, value.fresh(), direct_fields(&value)));
                        state.locals[target.0 as usize] = value;
                        state.initialized[target.0 as usize] = 2;
                    }
                }
                InstructionKind::DropEnum { local }
                | InstructionKind::DropManaged { local }
                | InstructionKind::DropPackage { local }
                | InstructionKind::DropErrorUnion { local }
                | InstructionKind::DropClosure { local }
                | InstructionKind::DropOwnedArray { local }
                | InstructionKind::StorageLive(local)
                | InstructionKind::StorageDead(local) => {
                    self.kill(state, *local, budget)?;
                    state.initialized[local.0 as usize] = 1;
                }
                InstructionKind::Call {
                    function,
                    arguments,
                } => {
                    result = Some(self.instantiate(
                        &self.summaries[function.0 as usize],
                        arguments,
                        state,
                        budget,
                    )?);
                    if matches!(
                        self.f
                            .values
                            .get(instruction.result.map_or(usize::MAX, |v| v.0 as usize)),
                        Some(Type::Managed(_))
                    ) && self.module.functions[function.0 as usize].result_mode
                        == PassingMode::Create
                    {
                        result = Some(Content::Leaf(BTreeSet::from([Source::Fresh])));
                    }
                }
                _ => {}
            }
            if let Some(output) = instruction.result {
                let value = if let Some(result) = result {
                    result
                } else {
                    content(
                        &self.f.values[output.0 as usize],
                        None,
                        &mut Vec::new(),
                        false,
                        budget,
                    )?
                };
                bridge.result = matches!(value, Content::Leaf(_)).then(|| value.fresh());
                bridge.package = direct_fields(&value);
                state.values[output.0 as usize] = value;
            }
            bridges.push(bridge);
        }
        Ok(bridges)
    }
}

fn run(
    module: &TypedModule,
    f: &TypedFunction,
    graph: &FunctionGraph,
    summaries: &[Content],
    validate: bool,
    budget: &mut Budget,
) -> Check<(Content, Vec<Vec<Bridge>>)> {
    let mut locals = Vec::new();
    let mut values = Vec::new();
    budget.spend(
        f.locals
            .len()
            .saturating_add(f.values.len())
            .saturating_add(f.blocks.len().saturating_mul(3)),
    )?;
    for local in &f.locals {
        locals.push(content(&local.ty, None, &mut Vec::new(), false, budget)?);
    }
    for ty in &f.values {
        values.push(content(ty, None, &mut Vec::new(), false, budget)?);
    }
    let mut state = State {
        initialized: vec![1; f.locals.len()],
        locals,
        values,
        facts: BTreeSet::new(),
        observations: BTreeSet::new(),
    };
    for (index, local) in f.parameters.iter().enumerate() {
        state.initialized[local.0 as usize] = 2;
        state.locals[local.0 as usize] = content(
            &f.locals[local.0 as usize].ty,
            Some(index as u32),
            &mut Vec::new(),
            false,
            budget,
        )?;
    }
    let mut definitions = vec![None; f.values.len()];
    let mut observers = vec![None; f.values.len()];
    for (b, block) in f.blocks.iter().enumerate() {
        for (i, instruction) in block.instructions.iter().enumerate() {
            budget.spend(1)?;
            if let Some(v) = instruction.result {
                definitions[v.0 as usize] = Some((b, i));
                if let InstructionKind::VariantTagLocal { local, path } = &instruction.kind {
                    budget.spend(path.len())?;
                    observers[v.0 as usize] =
                        Some((*local, path.iter().map(|p| (p.variant, p.index)).collect()));
                }
            }
        }
    }
    let flow = Flow {
        f,
        module,
        graph,
        definitions,
        observers,
        summaries,
    };
    let mut inputs = vec![None; f.blocks.len()];
    inputs[f.entry.0 as usize] = Some(state);
    let mut pending = VecDeque::from([f.entry.0 as usize]);
    let mut queued = vec![false; f.blocks.len()];
    queued[f.entry.0 as usize] = true;
    while let Some(block) = pending.pop_front() {
        queued[block] = false;
        let mut output = inputs[block].as_ref().unwrap().duplicate(budget)?;
        flow.block(block, &mut output, false, budget)?;
        for &next in &graph.successors[block] {
            let mut edge = output.duplicate(budget)?;
            flow.edge(&mut edge, block, next, budget)?;
            let changed = if let Some(input) = &mut inputs[next] {
                input.merge(&edge, budget)?
            } else {
                inputs[next] = Some(edge);
                true
            };
            if changed && !queued[next] {
                queued[next] = true;
                pending.push_back(next);
            }
        }
    }
    let mut result = content(&f.return_type, None, &mut Vec::new(), true, budget)?;
    let mut rows = vec![Vec::new(); f.blocks.len()];
    for (block, input) in inputs.into_iter().enumerate() {
        if let Some(mut state) = input {
            rows[block] = flow.block(block, &mut state, validate, budget)?;
            if let Terminator::Return(Some(value)) = f.blocks[block].terminator {
                result.merge(&state.values[value.0 as usize], budget)?;
            }
        }
    }
    Ok((result, rows))
}

pub(super) fn check_module(
    module: &TypedModule,
    graphs: &[FunctionGraph],
    budget: &mut Budget,
) -> Check<Proofs> {
    if !budget.sums {
        return Ok(Proofs::default());
    }
    budget.spend(module.functions.len().saturating_mul(2))?;
    let mut summaries = Vec::new();
    for f in &module.functions {
        summaries.push(content(
            &f.return_type,
            None,
            &mut Vec::new(),
            true,
            budget,
        )?);
    }
    loop {
        let mut changed = false;
        for (index, f) in module.functions.iter().enumerate() {
            if f.foreign.is_some() {
                continue;
            }
            let (result, _) = run(module, f, &graphs[index], &summaries, false, budget)?;
            changed |= summaries[index].merge(&result, budget)?;
        }
        if !changed {
            break;
        }
    }
    let mut rows = Vec::new();
    for (index, f) in module.functions.iter().enumerate() {
        rows.push(if f.foreign.is_some() {
            Vec::new()
        } else {
            run(module, f, &graphs[index], &summaries, true, budget)?.1
        });
    }
    Ok(Proofs { rows })
}
