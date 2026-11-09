//! Frozen eager outer-COW Flow from the pre-check implementation.
use super::*;
struct OldFlow<'a, 'm> {
    actual: &'a Flow<'m>,
}
impl<'m> std::ops::Deref for OldFlow<'_, 'm> {
    type Target = Flow<'m>;
    fn deref(&self) -> &Self::Target {
        self.actual
    }
}
impl OldFlow<'_, '_> {
    fn block(
        &self,
        block: usize,
        state: &mut State,
        validate: bool,
        record: bool,
        budget: &mut Budget,
    ) -> Check<Vec<Bridge>> {
        if record {
            budget.spend(self.f.blocks[block].instructions.len())?;
        }
        let mut bridges = Vec::new();
        for instruction in &self.f.blocks[block].instructions {
            budget.spend(1)?;
            let mut result = None;
            let mut bridge = Bridge::default();
            if validate
                && self.partial
                && let InstructionKind::BorrowLocal { local }
                | InstructionKind::BorrowLocalMut { local } = instruction.kind
                && (state.initialized[local.0 as usize] != 2
                    || !state.locals[local.0 as usize].complete(budget)?)
            {
                return Err(bad(
                    Some(instruction.span),
                    "whole borrow requires complete initialized storage",
                ));
            }

            if let (Some(meta), Some(p)) = (&self.place_meta, &mut state.partial) {
                meta.step(p, instruction, budget)?;
                if partial_places::invalidates(&instruction.kind) {
                    if let Some(locals) = &self.tagged_locals {
                        budget.spend(locals.len())?;
                        for &local in locals {
                            let content = &mut state.locals[local];
                            content.cost(budget)?;
                            Arc::make_mut(content)
                                .forget_copy_tags(&self.f.locals[local].ty, budget)?;
                        }
                    } else {
                        for (local, content) in state.locals.iter_mut().enumerate() {
                            content.cost(budget)?;
                            Arc::make_mut(content)
                                .forget_copy_tags(&self.f.locals[local].ty, budget)?;
                        }
                    }
                }
            }
            if let Some(context) = self.replacements
                && let Some(value) = context.step(self, state, instruction, validate, budget)?
            {
                result = Some(value);
            }
            match &instruction.kind {
                InstructionKind::BorrowCapture { index, .. }
                    if state
                        .captures
                        .get(*index as usize)
                        .is_some_and(Option::is_some) =>
                {
                    let captured = state.captures[*index as usize]
                        .as_ref()
                        .expect("actual direct Sum capture");
                    if validate && !captured.complete(budget)? {
                        return Err(bad(
                            Some(instruction.span),
                            "direct Sum borrow reads an unfilled captured owner",
                        ));
                    }
                }
                InstructionKind::TakeCapture { index }
                    if state
                        .captures
                        .get(*index as usize)
                        .is_some_and(Option::is_some) =>
                {
                    let current = state.captures[*index as usize]
                        .as_ref()
                        .expect("actual direct Sum capture");
                    if validate && !current.complete(budget)? {
                        return Err(bad(
                            Some(instruction.span),
                            "direct Sum capture was already transferred",
                        ));
                    }
                    result = Some(current.duplicate(budget)?);
                    let ty = &self.module.closures
                        [self.f.environment.expect("capture entry").0 as usize]
                        .captures[*index as usize];
                    state.captures[*index as usize] =
                        Some(Arc::new(content(ty, None, &mut Vec::new(), true, budget)?));
                }
                InstructionKind::WriteCapture { index, value }
                    if state
                        .captures
                        .get(*index as usize)
                        .is_some_and(Option::is_some) =>
                {
                    let current = state.captures[*index as usize]
                        .as_ref()
                        .expect("actual direct Sum capture");
                    if validate && current.complete(budget)? {
                        return Err(bad(
                            Some(instruction.span),
                            "direct Sum capture refill would overwrite an active owner",
                        ));
                    }
                    let incoming = state
                        .values
                        .get(value.0 as usize, budget)?
                        .duplicate(budget)?;
                    if validate && !incoming.complete(budget)? {
                        return Err(bad(
                            Some(instruction.span),
                            "direct Sum capture refill requires a complete actual value",
                        ));
                    }
                    state.captures[*index as usize] = Some(Arc::new(incoming));
                }
                InstructionKind::BorrowLocalMut { local }
                    if matches!(
                        shared_enum_physical(&self.f.locals[local.0 as usize].ty, budget)?,
                        Type::Enum { .. }
                    ) =>
                {
                    fn forget(at: &mut Content, t: &Type, b: &mut Budget) -> Check<()> {
                        resources::charge_payload(t, b)?;
                        if matches!(t, Type::Enum { .. }) && t.is_copy_value() {
                            *at = content(t, None, &mut Vec::new(), false, b)?;
                            return Ok(());
                        }
                        match (at, t) {
                            (Content::Sum(vs, _), Type::Enum { variants, .. }) => {
                                for (tag, fields) in vs {
                                    for (child, t) in
                                        fields.iter_mut().zip(&variants[*tag as usize].1)
                                    {
                                        forget(child, t, b)?;
                                    }
                                }
                            }
                            (Content::Product(children), Type::Package { fields, .. }) => {
                                for (child, (_, t)) in children.iter_mut().zip(fields) {
                                    forget(child, t, b)?;
                                }
                            }
                            _ => {}
                        }
                        Ok(())
                    }
                    state.locals[local.0 as usize].cost(budget)?;
                    let physical =
                        shared_enum_physical(&self.f.locals[local.0 as usize].ty, budget)?;
                    forget(
                        Arc::make_mut(&mut state.locals[local.0 as usize]),
                        physical,
                        budget,
                    )?;
                    self.kill(state, *local, budget)?;
                }
                InstructionKind::MakeManaged { .. }
                | InstructionKind::ByteBufferNew
                | InstructionKind::OwnedStoreNew { .. } => {
                    result = Some(Content::Leaf(BTreeSet::from([Source::Fresh])))
                }
                InstructionKind::MakePackage { fields }
                | InstructionKind::MakeOwnedTuple { fields } => {
                    budget.spend(fields.len())?;
                    result = Some(Content::Product(
                        fields
                            .iter()
                            .map(|v| state.values.get(v.0 as usize, budget)?.duplicate(budget))
                            .collect::<Check<_>>()?,
                    ));
                }
                InstructionKind::MakeVariant { variant, fields } => {
                    budget.spend(fields.len())?;
                    result = Some(Content::Sum(
                        BTreeMap::from([(
                            *variant,
                            fields
                                .iter()
                                .map(|v| state.values.get(v.0 as usize, budget)?.duplicate(budget))
                                .collect::<Check<_>>()?,
                        )]),
                        None,
                    ));
                }
                InstructionKind::MakeErrorUnion { is_error, value } => {
                    result = Some(Content::Sum(
                        BTreeMap::from([(
                            if *is_error { 0 } else { 1 },
                            vec![
                                state
                                    .values
                                    .get(value.0 as usize, budget)?
                                    .duplicate(budget)?,
                            ],
                        )]),
                        None,
                    ))
                }
                InstructionKind::TakeErrorUnion { value, is_error } => {
                    if let Content::Sum(variants, _) =
                        state.values.get(value.0 as usize, budget)?.as_ref()
                        && let Some(fields) = variants.get(&(if *is_error { 0 } else { 1 }))
                    {
                        result = Some(fields[0].duplicate(budget)?);
                    }
                }
                InstructionKind::Store { local, value } => {
                    self.kill(state, *local, budget)?;
                    budget.spend(1)?;
                    state.locals[local.0 as usize] =
                        state.values.get(value.0 as usize, budget)?.clone();
                    state.initialized[local.0 as usize] = 2;
                }
                InstructionKind::PlaceTagLocal { local, path }
                | InstructionKind::BorrowPlaceLocal { local, path, .. } => {
                    if validate {
                        if state.initialized[local.0 as usize] != 2 {
                            return Err(bad(
                                Some(instruction.span),
                                "mixed place root is not initialized",
                            ));
                        }
                        state
                            .partial
                            .as_ref()
                            .ok_or_else(|| bad(None, "mixed place tag state absent"))?
                            .require(*local, path, budget)?;
                        let endpoint = state.locals[local.0 as usize].viewed(path, budget)?;
                        if matches!(instruction.kind, InstructionKind::BorrowPlaceLocal { .. })
                            && !endpoint.complete(budget)?
                        {
                            return Err(bad(
                                Some(instruction.span),
                                "mixed borrow requires every selected descendant initialized",
                            ));
                        }
                    }
                    if matches!(
                        instruction.kind,
                        InstructionKind::BorrowPlaceLocal { mutable: true, .. }
                    ) {
                        state.locals[local.0 as usize].cost(budget)?;
                        Arc::make_mut(&mut state.locals[local.0 as usize]).forget_place_copy_tags(
                            path,
                            &self.f.locals[local.0 as usize].ty,
                            budget,
                        )?;
                    }
                }
                InstructionKind::BorrowLocal { local }
                | InstructionKind::BorrowLocalMut { local }
                    if self.partial =>
                {
                    if validate && !state.locals[local.0 as usize].complete(budget)? {
                        return Err(bad(
                            Some(instruction.span),
                            "whole borrow requires complete initialized storage",
                        ));
                    }
                }
                InstructionKind::TakeTupleField { local, path }
                | InstructionKind::TakeVariantField { local, path } => {
                    if validate {
                        if state.initialized[local.0 as usize] != 2 {
                            return Err(bad(
                                Some(instruction.span),
                                "selective source storage is not initialized",
                            ));
                        }
                        state
                            .partial
                            .as_ref()
                            .ok_or_else(|| bad(None, "selective tag state is absent"))?
                            .require(*local, path, budget)?;
                    }
                    state.locals[local.0 as usize].cost(budget)?;
                    let selected = Arc::make_mut(&mut state.locals[local.0 as usize])
                        .take_place(path, validate, budget);
                    result = Some(match selected {
                        Ok(value) => value,
                        Err(e)
                            if !validate && e.category == zil_base::ResultCategory::SafeReject =>
                        {
                            let ty =
                                place_type(self.f, *local, path, Some(instruction.span), budget)?;
                            content(ty, None, &mut Vec::new(), true, budget)?
                        }
                        Err(e) => return Err(e),
                    });
                    self.kill(state, *local, budget)?;
                }
                InstructionKind::Load(local) | InstructionKind::TakeLocal { local } => {
                    if validate
                        && self.partial
                        && !state.locals[local.0 as usize].complete(budget)?
                    {
                        return Err(bad(
                            Some(instruction.span),
                            "whole local transfer/read requires every subtree initialized",
                        ));
                    }
                    result = Some(state.locals[local.0 as usize].duplicate(budget)?);
                    if matches!(instruction.kind, InstructionKind::TakeLocal { .. }) {
                        self.kill(state, *local, budget)?;
                        state.initialized[local.0 as usize] = 1;
                    }
                }
                InstructionKind::TakePackageField { local, index }
                | InstructionKind::ReadPackageField { local, index } => {
                    if let Content::Product(fields) = state.locals[local.0 as usize].as_ref() {
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
                    if Arc::strong_count(&state.locals[local.0 as usize]) > 1 {
                        state.locals[local.0 as usize].cost(budget)?;
                    }
                    if let Content::Product(fields) =
                        Arc::make_mut(&mut state.locals[local.0 as usize])
                    {
                        fields[*index as usize] = state
                            .values
                            .get(value.0 as usize, budget)?
                            .duplicate(budget)?;
                    }
                }
                InstructionKind::VariantTagLocal { local, path }
                | InstructionKind::ReadVariantLocal { local, path }
                | InstructionKind::BorrowVariantLocal { local, path, .. } => {
                    self.selected(
                        state,
                        *local,
                        path,
                        None,
                        validate,
                        instruction.span,
                        budget,
                    )?;
                    if matches!(
                        instruction.kind,
                        InstructionKind::BorrowVariantLocal { mutable: true, .. }
                    ) {
                        let ty =
                            projected_type(self.f, *local, path, Some(instruction.span), budget)?;
                        if matches!(ty, Type::Enum { .. } | Type::ErrorUnion { .. })
                            && ty.is_copy_value()
                        {
                            fn forget(
                                at: &mut Content,
                                path: &[VariantProjection],
                                ty: &Type,
                                budget: &mut Budget,
                            ) -> Check<()> {
                                if path.is_empty() {
                                    *at = content(ty, None, &mut Vec::new(), false, budget)?;
                                    return Ok(());
                                }
                                let Content::Sum(vs, _) = at else {
                                    return Err(bad(None, "mutable payload path crosses non-Enum"));
                                };
                                if let Some(fs) = vs.get_mut(&path[0].variant) {
                                    let child = fs
                                        .get_mut(path[0].index as usize)
                                        .ok_or_else(|| bad(None, "mutable payload field absent"))?;
                                    forget(child, &path[1..], ty, budget)?;
                                }
                                Ok(())
                            }
                            state.locals[local.0 as usize].cost(budget)?;
                            forget(
                                Arc::make_mut(&mut state.locals[local.0 as usize]),
                                path,
                                ty,
                                budget,
                            )?;
                            budget.spend(
                                path.len()
                                    .saturating_add(state.facts.len())
                                    .saturating_add(state.observations.len()),
                            )?;
                            let prefix = path
                                .iter()
                                .map(|p| (p.variant, p.index))
                                .collect::<Vec<_>>();
                            state.facts.retain(|(root, observed, _)| {
                                *root != *local || !observed.starts_with(&prefix)
                            });
                            state.observations.retain(|v| {
                                self.observers[v.0 as usize].as_ref().is_none_or(
                                    |(root, observed)| {
                                        *root != *local || !observed.starts_with(&prefix)
                                    },
                                )
                            });
                        }
                    }
                    if matches!(instruction.kind, InstructionKind::VariantTagLocal { .. }) {
                        budget.spend(state.observations.len().saturating_add(1))?;
                        state
                            .observations
                            .insert(instruction.result.expect("typed tag"));
                    }
                }
                InstructionKind::UnpackVariant {
                    local,
                    variant: _,
                    fields,
                }
                | InstructionKind::UnpackTuple { local, fields } => {
                    let variant =
                        if let InstructionKind::UnpackVariant { variant, .. } = instruction.kind {
                            Some(variant)
                        } else {
                            None
                        };
                    if variant.is_some() {
                        self.selected(
                            state,
                            *local,
                            &[],
                            variant,
                            validate,
                            instruction.span,
                            budget,
                        )?;
                    }
                    if validate
                        && self.partial
                        && !state.locals[local.0 as usize].complete(budget)?
                    {
                        return Err(bad(
                            Some(instruction.span),
                            "atomic unpack requires every subtree initialized",
                        ));
                    }
                    if validate && state.initialized[local.0 as usize] != 2 {
                        return Err(bad(
                            Some(instruction.span),
                            "Unpack source is not initialized",
                        ));
                    }
                    budget.spend(fields.len())?;
                    if validate && fields.iter().any(|l| state.initialized[l.0 as usize] != 1) {
                        return Err(bad(
                            Some(instruction.span),
                            "every Unpack destination must be vacant on every incoming path before the atomic transfer",
                        ));
                    }
                    let payload = match state.locals[local.0 as usize].as_ref() {
                        Content::Sum(variants, _) if variant.is_some() => {
                            variants.get(&variant.unwrap())
                        }
                        Content::Product(fields) if variant.is_none() => Some(fields),
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
                        if record {
                            bridge.unpack.push((
                                target,
                                value.fresh(),
                                if matches!(
                                    self.f.locals[target.0 as usize].ty,
                                    Type::Package { .. }
                                ) {
                                    direct_fields(&value, budget)?
                                } else {
                                    Vec::new()
                                },
                            ));
                        }
                        state.locals[target.0 as usize] = Arc::new(value);
                        state.initialized[target.0 as usize] = 2;
                    }
                }
                InstructionKind::DropTuple { local }
                | InstructionKind::DropEnum { local }
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
                    let result_ty = &self.module.functions[function.0 as usize].return_type;
                    resources::charge_payload(result_ty, budget)?;
                    let mut returned = if result_ty.is_copy_value() {
                        // Copy callees have no owning-source summary. Its
                        // bottom placeholder is not an impossible runtime
                        // value: rebuild all actual Copy alternatives.
                        content(result_ty, None, &mut Vec::new(), false, budget)?
                    } else {
                        self.instantiate(
                            &self.summaries[function.0 as usize],
                            arguments,
                            state,
                            budget,
                        )?
                    };
                    returned.returned(budget)?;
                    if self.module.functions[function.0 as usize].result_mode == PassingMode::Create
                        && errors::managed_success(
                            &self.module.functions[function.0 as usize].return_type,
                        )
                        && let Content::Sum(variants, _) = &mut returned
                        && let Some(fields) = variants.get_mut(&1)
                    {
                        fields[0] = Content::Leaf(BTreeSet::from([Source::Fresh]));
                    }
                    result = Some(returned);
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
                if record {
                    bridge.result = matches!(value, Content::Leaf(_)).then(|| value.fresh());
                    if matches!(self.f.values[output.0 as usize], Type::Package { .. }) {
                        bridge.package = direct_fields(&value, budget)?;
                    }
                }
                state
                    .values
                    .set(output.0 as usize, Arc::new(value), budget)?;
            }
            if record {
                bridges.push(bridge);
            }
        }
        Ok(bridges)
    }
}

fn budget(profile: usize, steps: usize) -> Budget {
    let mut b = Budget::new(&Limits {
        max_analysis_steps: steps,
        ..Limits::default()
    });
    b.pod_stores = profile == 1;
    b.store_option_results = profile == 2;
    b.ordinary_error_places = profile == 2;
    b
}
fn tag_type() -> Type {
    Type::Enum {
        name: "Observed".into(),
        variants: vec![("A".into(), vec![Type::I32]), ("B".into(), vec![Type::I32])],
    }
}
fn fixture(fail: bool) -> TypedModule {
    use crate::place_replacements::fixture::{instruction as i, module};
    let mut module = module();
    let f = &mut module.functions[0];
    let tag = tag_type();
    let base = f.locals[0].clone();
    f.parameters = vec![LocalId(0), LocalId(2)];
    f.locals = (0..if fail { 4 } else { 3 })
        .map(|id| {
            let mut l = base.clone();
            l.id = LocalId(id);
            l.mutable = true;
            l.ty = if id < 2 {
                tag.clone()
            } else {
                Type::Tuple(vec![tag.clone()])
            };
            l
        })
        .collect();
    f.values = vec![Type::I32, tag];
    f.blocks[0].instructions = vec![
        i(
            Some(ValueId(0)),
            InstructionKind::PlaceTagLocal {
                local: LocalId(2),
                path: vec![PlaceProjection::Field(0)],
            },
        ),
        i(Some(ValueId(1)), InstructionKind::Load(LocalId(0))),
        i(
            None,
            InstructionKind::Store {
                local: LocalId(1),
                value: ValueId(1),
            },
        ),
    ];
    if fail {
        f.values.push(Type::I32);
        f.blocks[0].instructions.push(i(
            Some(ValueId(2)),
            InstructionKind::PlaceTagLocal {
                local: LocalId(3),
                path: vec![PlaceProjection::Field(0)],
            },
        ));
    }
    module
}
fn flow<'m>(
    module: &'m TypedModule,
    summaries: &'m [Content],
    tagged: bool,
    profile: usize,
) -> Flow<'m> {
    let f = &module.functions[0];
    let mut definitions = vec![None; f.values.len()];
    for (block, b) in f.blocks.iter().enumerate() {
        for (position, i) in b.instructions.iter().enumerate() {
            if let Some(v) = i.result {
                definitions[v.0 as usize] = Some((block, position));
            }
        }
    }
    Flow {
        f,
        module,
        definitions,
        observers: vec![None; f.values.len()],
        summaries,
        partial: true,
        replacements: None,
        address_replacement_tags: false,
        tagged_locals: tagged.then(|| (0..f.locals.len()).collect()),
        place_meta: Some(partial_places::Meta::new(f, &mut budget(profile, 1_000_000)).unwrap()),
    }
}
fn seed(module: &TypedModule, profile: usize) -> State {
    let f = &module.functions[0];
    let mut b = budget(profile, 1_000_000);
    State {
        initialized: (0..f.locals.len())
            .map(|i| if i == 0 || i == 2 { 2 } else { 1 })
            .collect(),
        locals: f
            .locals
            .iter()
            .map(|l| {
                Ok(Arc::new(content(
                    &l.ty,
                    None,
                    &mut Vec::new(),
                    false,
                    &mut b,
                )?))
            })
            .collect::<Check<_>>()
            .unwrap(),
        values: values::Values::build(f.values.len(), &mut b, |i, b| {
            Ok(Arc::new(content(
                &f.values[i],
                None,
                &mut Vec::new(),
                true,
                b,
            )?))
        })
        .unwrap(),
        captures: vec![],
        facts: BTreeSet::new(),
        observations: BTreeSet::new(),
        partial: Some(partial_places::State::default()),
    }
}
#[derive(Debug, PartialEq, Eq)]
struct Logical {
    initialized: Vec<u8>,
    locals: Vec<Content>,
    values: Vec<Content>,
    captures: Vec<Option<Content>>,
    facts: BTreeSet<Fact>,
    observations: BTreeSet<ValueId>,
    partial: Option<partial_places::State>,
}
fn logical(state: &State) -> Logical {
    Logical {
        initialized: state.initialized.clone(),
        locals: state.locals.iter().map(|v| v.as_ref().clone()).collect(),
        values: state
            .values
            .flat()
            .iter()
            .map(|v| v.as_ref().clone())
            .collect(),
        captures: state
            .captures
            .iter()
            .map(|v| v.as_ref().map(|v| v.as_ref().clone()))
            .collect(),
        facts: state.facts.clone(),
        observations: state.observations.clone(),
        partial: state.partial.clone(),
    }
}
type BridgeFacts = (Option<u8>, FieldFresh, Vec<UnpackedFresh>);
fn bridges(result: Check<Vec<Bridge>>) -> Check<Vec<BridgeFacts>> {
    result.map(|v| {
        v.into_iter()
            .map(|v| (v.result, v.package, v.unpack))
            .collect()
    })
}

#[test]
fn shared_outer_cow_matches_complete_eager_flow_for_both_actual_invalidation_loops() {
    for profile in 0..3 {
        for tagged in [false, true] {
            for validate in [false, true] {
                let module = fixture(false);
                let f = flow(&module, &[Content::Copy], tagged, profile);
                let old = OldFlow { actual: &f };
                assert!(partial_places::invalidates(
                    &module.functions[0].blocks[0].instructions[2].kind
                ));
                let template = seed(&module, profile);
                let alias = template.clone();
                let snapshot = logical(&alias);
                let mut a = template.clone();
                let mut b = template.clone();
                let x =
                    bridges(old.block(0, &mut a, validate, true, &mut budget(profile, 1_000_000)))
                        .unwrap();
                let y =
                    bridges(f.block(0, &mut b, validate, true, &mut budget(profile, 1_000_000)))
                        .unwrap();
                assert_eq!(x, y);
                assert_eq!(logical(&a), logical(&b));
                assert_eq!(logical(&alias), snapshot);
                // Preserving a fixed-point pointer does not change a later merge's
                // complete state or changed bit, compared with the old eager pointer.
                assert_eq!(
                    a.merge(&template, &mut budget(profile, 1_000_000)),
                    b.merge(&template, &mut budget(profile, 1_000_000))
                );
                assert_eq!(logical(&a), logical(&b));
            }
        }
    }
}
#[test]
fn shared_outer_cow_preserves_full_flow_semantic_error_prefix_and_initialization() {
    for profile in 0..3 {
        for tagged in [false, true] {
            let module = fixture(true);
            let f = flow(&module, &[Content::Copy], tagged, profile);
            let old = OldFlow { actual: &f };
            let template = seed(&module, profile);
            let mut a = template.clone();
            let mut b = template.clone();
            let x = bridges(old.block(0, &mut a, true, true, &mut budget(profile, 1_000_000)));
            let y = bridges(f.block(0, &mut b, true, true, &mut budget(profile, 1_000_000)));
            assert_eq!(x, y);
            let error = x.unwrap_err();
            assert!(
                error
                    .message
                    .contains("mixed place root is not initialized"),
                "{error:?}"
            );
            assert_eq!(logical(&a), logical(&b));
            assert_eq!(b.initialized[1], 2);
            assert_eq!(b.initialized[3], 1);
        }
    }
}
fn eager(value: &mut Arc<Content>, ty: &Type, b: &mut Budget) -> Check<()> {
    value.cost(b)?;
    Arc::make_mut(value).forget_copy_tags(ty, b)
}
#[test]
fn shared_outer_cow_fallbacks_preserve_sources_routes_shapes_and_root_kinds() {
    let ty = tag_type();
    let unknown = content(&ty, None, &mut Vec::new(), false, &mut budget(1, 1_000_000)).unwrap();
    let mut routed = unknown.clone();
    let Content::Sum(_, route) = &mut routed else {
        unreachable!()
    };
    *route = Some((0, vec![1, 0, 0]));
    let owning = Type::Enum {
        name: "Owned".into(),
        variants: vec![("A".into(), vec![Type::Managed(Box::new(Type::ByteBuffer))])],
    };
    let sources = Content::Leaf(BTreeSet::from([
        Source::Fresh,
        Source::Input(2, vec![0, 1]),
        Source::Captured(3, vec![]),
    ]));
    let inputs = vec![
        (ty.clone(), routed),
        (ty.clone(), Content::Sum(BTreeMap::new(), None)),
        (
            ty.clone(),
            Content::Sum(
                BTreeMap::from([(0, vec![sources.clone()]), (1, vec![Content::Copy])]),
                None,
            ),
        ),
        (ty.clone(), Content::Missing(Box::new(unknown.clone()))),
        (
            Type::Tuple(vec![ty.clone()]),
            Content::Product(vec![unknown.clone()]),
        ),
        (Type::Managed(Box::new(Type::ByteBuffer)), sources.clone()),
        (
            owning,
            Content::Sum(BTreeMap::from([(0, vec![sources])]), None),
        ),
    ];
    for profile in 0..3 {
        for (ty, input) in &inputs {
            for shared in [false, true] {
                let mut old = Arc::new(input.clone());
                let mut new = Arc::new(input.clone());
                let old_alias = shared.then(|| old.clone());
                let new_alias = shared.then(|| new.clone());
                let x = eager(&mut old, ty, &mut budget(profile, 1_000_000));
                let y = forget_shared_copy_tags(&mut new, ty, &mut budget(profile, 1_000_000));
                assert_eq!(x, y);
                assert_eq!(old, new);
                if let (Some(a), Some(b)) = (old_alias, new_alias) {
                    assert_eq!(a.as_ref(), input);
                    assert_eq!(b.as_ref(), input);
                    assert!(
                        !Arc::ptr_eq(&new, &b),
                        "fallback unexpectedly preserved root"
                    );
                }
            }
        }
    }
}
#[test]
fn shared_fixed_point_keeps_its_alias_until_an_actual_later_cow_change() {
    let ty = tag_type();
    let original = content(&ty, None, &mut Vec::new(), false, &mut budget(1, 1_000_000)).unwrap();
    let mut value = Arc::new(original.clone());
    let alias = value.clone();
    let snapshot = alias.as_ref().clone();
    forget_shared_copy_tags(&mut value, &ty, &mut budget(1, 1_000_000)).unwrap();
    assert!(Arc::ptr_eq(&value, &alias));
    let mut b = budget(1, 1_000_000);
    value.cost(&mut b).unwrap();
    Arc::make_mut(&mut value)
        .merge(&Content::Missing(Box::new(original)), &mut b)
        .unwrap();
    assert!(!Arc::ptr_eq(&value, &alias));
    assert_eq!(alias.as_ref(), &snapshot);
    assert!(matches!(value.as_ref(), Content::Missing(_)));
}
#[test]
fn shared_outer_cow_has_an_exact_smaller_complete_flow_request_boundary() {
    let module = fixture(false);
    let f = flow(&module, &[Content::Copy], true, 1);
    let old = OldFlow { actual: &f };
    let template = seed(&module, 1);
    let run = |original: bool, n| {
        let mut state = template.clone();
        let mut b = budget(1, n);
        for _ in 0..32 {
            if original {
                old.block(0, &mut state, true, false, &mut b)?;
            } else {
                f.block(0, &mut state, true, false, &mut b)?;
            }
        }
        Ok::<_, Diagnostic>(logical(&state))
    };
    let boundary = |old| {
        let mut lo = 0;
        let mut hi = 1_000_000;
        assert!(run(old, hi).is_ok());
        while lo < hi {
            let mid = (lo + hi) / 2;
            if run(old, mid).is_ok() {
                hi = mid
            } else {
                lo = mid + 1
            }
        }
        lo
    };
    let n = boundary(false);
    let old_n = boundary(true);
    assert!(n < old_n, "{n} >= {old_n}");
    assert_eq!(run(false, n).unwrap(), run(true, old_n).unwrap());
    assert_eq!(run(false, n), run(false, n));
    assert_eq!(
        run(false, n - 1).unwrap_err().category,
        zil_base::ResultCategory::ResourceLimit
    );
    eprintln!("V shared outer-COW 32 full blocks N={n}, eager N={old_n}; N-1/fresh checked");
}
