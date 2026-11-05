//! Actual replacement changes freshness as well as write effects. This is a
//! separate rerun frontier: the legacy dependency extractor assumes writes do
//! not change origin state and therefore cannot solve these rows.
use super::*;
use crate::place_replacements::{Base, Definitions, origins};
use zil_ir::PlaceProjection;
type Target = origins::Place;
mod package;
mod retirement;
mod whole_enum;
#[derive(Clone, Copy)]
pub(super) struct CheckPoint<'a> {
    pub(super) validate: bool,
    pub(super) live: Option<&'a Live>,
    pub(super) output: Option<ValueId>,
    pub(super) position: Option<(usize, usize)>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum StoreInvalidation {
    Input(u32),
    BorrowedCapture(u32),
}
struct Invocation<'a> {
    callee: usize,
    arguments: &'a [ValueId],
    captures: Option<&'a origins::Slots>,
    carrier: Option<Place>,
    sealed: bool,
}
pub(super) type Effects = BTreeMap<Target, BTreeSet<Birth>>;
pub(super) struct Program {
    routes: Vec<origins::Facts>,
    definitions: Vec<Definitions>,
    retirement_pairs: RefCell<Vec<retirement::Pairs>>,
    exact_retirement: bool,
    whole_helpers: BTreeSet<FunctionId>,
}
fn fee(b: &mut Budget, n: usize) -> CompileResult<()> {
    b.spend(n, "actual replacement origin transfer")
}
fn row_fee(rows: &Effects, target: &Target, b: &mut Budget) -> CompileResult<()> {
    fee(b, rows.len())?;
    let mut n = target
        .path
        .len()
        .checked_add(1)
        .and_then(|n| n.checked_mul(rows.len().saturating_add(1)))
        .ok_or_else(|| {
            vec![Diagnostic::limit(
                Phase::Analyze,
                "replacement summary comparison work overflow",
                None,
            )]
        })?;
    for old in rows.keys() {
        n = n
            .checked_add(old.path.len().saturating_add(1))
            .ok_or_else(|| {
                vec![Diagnostic::limit(
                    Phase::Analyze,
                    "replacement summary comparison work overflow",
                    None,
                )]
            })?;
    }
    fee(b, n)
}
fn add(
    rows: &mut Effects,
    target: &Target,
    birth: &BTreeSet<Birth>,
    b: &mut Budget,
) -> CompileResult<bool> {
    row_fee(rows, target, b)?;
    fee(
        b,
        target
            .path
            .len()
            .saturating_add(birth.len())
            .saturating_add(1),
    )?;
    let at = rows.entry(target.clone()).or_default();
    fee(b, at.len().saturating_add(birth.len()))?;
    let old = at.len();
    at.extend(birth);
    Ok(at.len() != old)
}
pub(super) fn grow(to: &mut Effects, from: &Effects, b: &mut Budget) -> CompileResult<bool> {
    fee(b, from.len())?;
    let mut changed = false;
    for (target, birth) in from {
        changed |= add(to, target, birth, b)?;
    }
    Ok(changed)
}
fn endpoint<'a>(
    mut ty: &'a Type,
    path: &[PlaceProjection],
    b: &mut Budget,
) -> CompileResult<&'a Type> {
    fee(b, path.len().saturating_add(1))?;
    for step in path {
        ty = match (step, ty) {
            (PlaceProjection::Field(i), Type::Tuple(fs)) => fs.get(*i as usize),
            (
                PlaceProjection::Field(i),
                Type::Package { fields: fs, .. } | Type::Record { fields: fs, .. },
            ) => fs.get(*i as usize).map(|(_, t)| t),
            (PlaceProjection::Variant(p), Type::Enum { variants, .. }) => variants
                .get(p.variant as usize)
                .and_then(|(_, fs)| fs.get(p.index as usize)),
            (PlaceProjection::ErrorArm(e), Type::ErrorUnion { error, result }) => {
                Some(if *e { error.as_ref() } else { result.as_ref() })
            }
            _ => None,
        }
        .ok_or_else(|| error("actual replacement route crosses a view or absent field"))?;
    }
    Ok(ty)
}
fn closed(ty: &Type, b: &mut Budget) -> CompileResult<bool> {
    b.type_work(ty, true, "closed replacement endpoint qualification")?;
    Ok(
        matches!(ty,Type::Managed(p) if matches!(p.as_ref(),Type::Record{..}) && p.is_copy_value())
            || package::field(ty, b)?.is_some()
            || whole_enum::admitted(ty, b)?,
    )
}
impl Program {
    pub(super) fn new(
        module: &TypedModule,
        graphs: &[(Vec<Vec<usize>>, Vec<bool>)],
        b: &mut Budget,
    ) -> CompileResult<Self> {
        // The nested transport producer is not implemented. Check the whole
        // graph, including unused wrappers, rather than treating an absent
        // outer description row as proof that no old instance is retained.
        fee(b, module.closures.len())?;
        for closure in &module.closures {
            fee(b, closure.captures.len())?;
            for capture in &closure.captures {
                let id = match capture {
                    Type::Closure(id) => Some(*id),
                    Type::Address { inner, .. } => {
                        if let Type::Closure(id) = inner.as_ref() {
                            Some(*id)
                        } else {
                            None
                        }
                    }
                    _ => None,
                };
                let Some(id) = id else {
                    continue;
                };
                let inner = module
                    .closures
                    .get(id.0 as usize)
                    .ok_or_else(|| error("nested closure capture identity absent"))?;
                fee(b, inner.captures.len())?;
                if inner
                    .captures
                    .iter()
                    .any(|field| matches!(field, Type::Desc(_)))
                {
                    return Err(vec![Diagnostic::unsupported(
                        Phase::Analyze,
                        "nested closure resource descriptions need their transitive carrier bridge",
                        None,
                    )]);
                }
            }
        }
        let exact_retirement = retirement::needed(module, b)?;
        let routes = origins::module(module, graphs, exact_retirement, b)?;
        fee(b, module.functions.len())?;
        let mut definitions = Vec::with_capacity(module.functions.len());
        let mut packages = false;
        let mut whole_helpers = BTreeSet::new();
        for (f, (_, reachable)) in module.functions.iter().zip(graphs) {
            let d = Definitions::new(module, f, b)?;
            if f.foreign.is_none() {
                d.activations(f, reachable, b)?;
            }
            for definition in d.entries.values() {
                if matches!(definition.ty, Type::Package { .. } | Type::Enum { .. }) {
                    packages = true;
                    fee(b, definition.path.len().saturating_add(1))?;
                    let targets = match &definition.base {
                        Base::Local(local) => BTreeSet::from([Target {
                            root: origins::Root::Local(*local),
                            path: definition.path.clone(),
                            mutable: f.locals[local.0 as usize].mutable,
                        }]),
                        Base::Address(v) => origins::project(
                            &routes[f.id.0 as usize].values[v.0 as usize],
                            &definition.path,
                            b,
                        )?,
                    };
                    fee(b, targets.len())?;
                    if targets.len() != 1 {
                        return Err(vec![Diagnostic::unsupported(
                            Phase::Analyze,
                            "complete replacement requires one actual local root",
                            None,
                        )]);
                    }
                    for target in &targets {
                        if matches!(definition.ty, Type::Enum { .. }) {
                            whole_enum::local(f, target, b)?;
                        } else {
                            if matches!(target.root, origins::Root::Local(_)) {
                                package::local(f, target, b)?;
                            } else {
                                package::borrowed(f, target, b)?;
                                b.lookup(whole_helpers.len(), "whole Package helper insertion")?;
                                if f.blocks.len() == 1 {
                                    whole_helpers.insert(f.id);
                                }
                            }
                        }
                    }
                }
                if !closed(&definition.ty, b)? {
                    return Err(vec![Diagnostic::unsupported(
                        Phase::Analyze,
                        "replacement endpoint cleanup/hold bridge is under construction",
                        None,
                    )]);
                }
            }
            definitions.push(d);
        }
        if packages {
            package::definitions(module, b)?;
        }
        let retirement_pairs = if exact_retirement {
            fee(b, module.functions.len())?;
            vec![retirement::Pairs::new(); module.functions.len()]
        } else {
            Vec::new()
        };
        Ok(Self {
            routes,
            definitions,
            retirement_pairs: RefCell::new(retirement_pairs),
            exact_retirement,
            whole_helpers,
        })
    }
    pub(super) fn whole_helper(&self, id: FunctionId, b: &mut Budget) -> CompileResult<bool> {
        if self.whole_helpers.is_empty() {
            return Ok(false);
        }
        b.lookup(
            self.whole_helpers.len(),
            "whole Package helper proof lookup",
        )?;
        Ok(self.whole_helpers.contains(&id))
    }
    pub(super) fn extend_shapes(
        &self,
        f: &TypedFunction,
        shapes: &mut Shapes,
        b: &mut Budget,
    ) -> CompileResult<()> {
        let routes = &self.routes[f.id.0 as usize].values;
        for (value, row) in routes.iter().enumerate() {
            fee(b, row.len().saturating_add(1))?;
            let ty = &f.values[value];
            if (head_address(ty) || ty.is_package_address())
                && row
                    .iter()
                    .any(|place| matches!(place.root, origins::Root::StoreElement(_)))
            {
                b.lookup(shapes.len(), "actual Store address resource shape")?;
                shapes.insert(
                    Place::Value(ValueId(value as u32)),
                    Shape {
                        owned: false,
                        description: true,
                    },
                );
            }
        }
        for block in &f.blocks {
            for i in &block.instructions {
                fee(b, 1)?;
                if let InstructionKind::Store { local, value } = i.kind {
                    let row = &routes[value.0 as usize];
                    fee(b, row.len())?;
                    let ty = &f.locals[local.0 as usize].ty;
                    if (head_address(ty) || ty.is_package_address())
                        && row
                            .iter()
                            .any(|place| matches!(place.root, origins::Root::StoreElement(_)))
                    {
                        b.lookup(shapes.len(), "actual Store address local resource shape")?;
                        shapes.insert(
                            Place::Local(local),
                            Shape {
                                owned: false,
                                description: true,
                            },
                        );
                    }
                }
            }
        }
        Ok(())
    }
    fn targets(
        &self,
        f: &TypedFunction,
        base: &Base,
        path: &[PlaceProjection],
        b: &mut Budget,
    ) -> CompileResult<origins::Places> {
        let mut out = origins::Places::new();
        match base {
            Base::Local(local) => {
                fee(b, path.len().saturating_add(1))?;
                out.insert(Target {
                    root: origins::Root::Local(*local),
                    path: path.to_vec(),
                    mutable: f.locals[local.0 as usize].mutable,
                });
            }
            Base::Address(value) => {
                let row = &self.routes[f.id.0 as usize].values[value.0 as usize];
                out = origins::project(row, path, b)?;
            }
        }
        if out.is_empty() {
            return Err(error(
                "replacement parent has no independently reconstructed actual target",
            ));
        }
        Ok(out)
    }
    fn actual_type<'a>(
        &self,
        module: &'a TypedModule,
        f: &'a TypedFunction,
        target: &Target,
        b: &mut Budget,
    ) -> CompileResult<&'a Type> {
        let root = match target.root {
            origins::Root::Local(local) => &f.locals[local.0 as usize].ty,
            origins::Root::Parameter(p) => &f.locals[f.parameters[p as usize].0 as usize].ty,
            origins::Root::Capture(p) => {
                &module.closures[f
                    .environment
                    .ok_or_else(|| error("replacement capture parent absent"))?
                    .0 as usize]
                    .captures[p as usize]
            }
            origins::Root::ParameterCapture(p, c) => {
                let ty = &f.locals[f.parameters[p as usize].0 as usize].ty;
                let ty = if let Type::Address { inner, .. } = ty {
                    inner.as_ref()
                } else {
                    ty
                };
                let Type::Closure(id) = ty else {
                    return Err(error("replacement parameter capture has no closure"));
                };
                &module.closures[id.0 as usize].captures[c as usize]
            }
            origins::Root::StoreElement(value) => &f.values[value.0 as usize],
        };
        let (mutable, ty) = if matches!(target.root, origins::Root::Local(_)) {
            (
                f.locals[match target.root {
                    origins::Root::Local(l) => l.0 as usize,
                    _ => unreachable!(),
                }]
                .mutable,
                root,
            )
        } else {
            let Type::Address { mutable, inner } = root else {
                return Err(error("actual replacement parent is not an address"));
            };
            (*mutable, inner.as_ref())
        };
        if !mutable || !target.mutable {
            return Err(error(
                "replacement cannot promote actual shared parent authority",
            ));
        }
        endpoint(ty, &target.path, b)
    }
    fn initialized(
        &self,
        flow: &Flow<'_>,
        state: &State,
        local: LocalId,
        path: &[PlaceProjection],
        b: &mut Budget,
    ) -> CompileResult<()> {
        if !exact(&copied(state, Place::Local(local), b)?, Life::Available) {
            return Err(error("replacement needs a live actual owner shell"));
        }
        state.sums.require_place(local, path, b)?;
        if path.is_empty()
            && matches!(
                flow.function.locals[local.0 as usize].ty,
                Type::Package { .. }
            )
        {
            package::complete(flow.function, state, local, b)?;
        }
        if let Type::Package { fields, .. } = &flow.function.locals[local.0 as usize].ty
            && let Some(PlaceProjection::Field(index)) = path.first()
            && matches!(
                fields[*index as usize].1,
                Type::Managed(_) | Type::Package { .. }
            )
        {
            let field = copied(
                state,
                Place::PackageField {
                    local,
                    index: *index,
                },
                b,
            )?;
            if !exact(&field, Life::Available) {
                return Err(error(
                    "replacement endpoint field is absent or already transferred",
                ));
            }
        }
        Ok(())
    }
    fn invalidate_store(
        &self,
        flow: &Flow<'_>,
        state: &State,
        effect: &mut Effect,
        parents: &BTreeSet<Source>,
        point: CheckPoint<'_>,
        b: &mut Budget,
    ) -> CompileResult<()> {
        let CheckPoint {
            validate: check,
            live,
            ..
        } = point;
        if parents.is_empty() {
            return Err(error(
                "Store replacement has no actual parent responsibility family",
            ));
        }
        fee(b, parents.len())?;
        let nonlocal = |source: Source| {
            matches!(
                source.parent(),
                Source::Argument(_) | Source::BorrowedField(_)
            )
        };
        if parents.iter().any(|source| {
            matches!(
                source.parent(),
                Source::OwnedField(_) | Source::OwnedPackageField { .. }
            )
        }) {
            return Err(vec![Diagnostic::unsupported(
                Phase::Analyze,
                "owned Store capture replacement needs its exact capture instance mapping",
                None,
            )]);
        }
        if parents.iter().any(|source| nonlocal(*source))
            && (flow.function.callable_contract.is_some()
                || flow.function.callable_return_contract.is_some())
        {
            return Err(vec![Diagnostic::unsupported(
                Phase::Analyze,
                "Store instance invalidation needs a public callable envelope",
                None,
            )]);
        }
        if check {
            let live =
                live.ok_or_else(|| error("Store replacement description liveness missing"))?;
            for place in live {
                let ty = match place {
                    Place::Local(local) => &flow.function.locals[local.0 as usize].ty,
                    Place::Value(value) => &flow.function.values[value.0 as usize],
                    _ => continue,
                };
                if !(matches!(ty, Type::Desc(_) | Type::Closure(_))
                    || matches!(ty, Type::Address { inner, .. } if matches!(inner.as_ref(), Type::Closure(_))))
                {
                    continue;
                }
                let fact = copied(state, *place, b)?;
                fee(
                    b,
                    fact.sources
                        .len()
                        .saturating_mul(parents.len().saturating_add(1)),
                )?;
                if fact.sources.iter().any(|source| {
                    !matches!(source, Source::StoreParent(_))
                        && parents.iter().any(|parent| {
                            source.parent() == parent.parent()
                                || (nonlocal(*source) && nonlocal(*parent))
                        })
                }) {
                    return Err(error(
                        "Store field replacement invalidates a live element resource description",
                    ));
                }
            }
        }
        for source in parents {
            let invalidation = match source.parent() {
                Source::Argument(p) => Some(StoreInvalidation::Input(p)),
                Source::BorrowedField(c) => Some(StoreInvalidation::BorrowedCapture(c)),
                _ => None,
            };
            if let Some(invalidation) = invalidation {
                b.lookup(
                    effect.invalidated_stores.len(),
                    "actual Store instance invalidation effect",
                )?;
                effect.invalidated_stores.insert(invalidation);
            }
        }
        record_write(parents, effect, false, check, b)
    }
    #[allow(clippy::too_many_arguments)]
    fn update(
        &self,
        flow: &Flow<'_>,
        state: &mut State,
        effect: &mut Effect,
        target: &Target,
        birth: &BTreeSet<Birth>,
        check: bool,
        live: Option<&Live>,
        b: &mut Budget,
    ) -> CompileResult<()> {
        if let Some(rows) = &mut state.whole_commits {
            must_cost(Some(rows), b)?;
            rows.clear();
        }
        let ty = self.actual_type(flow.module, flow.function, target, b)?;
        if !closed(ty, b)? {
            return Err(error(
                "actual replacement endpoint differs from closed owning leaf",
            ));
        }
        match target.root {
            origins::Root::Local(local) => {
                let shell = Place::Local(local);
                if check {
                    self.initialized(flow, state, local, &target.path, b)?;
                }
                if state
                    .sums
                    .replace_birth(local, &target.path, birth, check, b)?
                {
                    return Ok(());
                }
                if target.path.is_empty() {
                    let mut old = copied(state, shell, b)?;
                    fee(b, old.birth.len().saturating_add(birth.len()))?;
                    old.birth.extend(birth);
                    replace(state, shell, old, b)?;
                } else {
                    fee(b, target.path.len())?;
                    let mut fields = Vec::with_capacity(target.path.len());
                    for step in &target.path {
                        let PlaceProjection::Field(index) = step else {
                            return Err(error(
                                "replacement tagged local has no initialization tree",
                            ));
                        };
                        fields.push(*index)
                    }
                    let mut old =
                        product::birth(state, product::Carrier::Local(local), &fields, b)?;
                    // A may-write keeps old freshness. No shell debt/order is
                    // recreated: replacing a child is not constructing a shell.
                    fee(b, old.len().saturating_add(birth.len()))?;
                    old.extend(birth);
                    product::set(state, product::Carrier::Local(local), &fields, old, b)?;
                }
            }
            origins::Root::Parameter(parameter) | origins::Root::ParameterCapture(parameter, _) => {
                b.lookup(
                    effect.arguments.len(),
                    "replacement ordinary parameter write effect",
                )?;
                effect.arguments.insert(parameter);
                add(&mut effect.replacements, target, birth, b)?;
            }
            origins::Root::Capture(capture) => {
                if flow
                    .function
                    .environment
                    .is_some_and(|id| flow.module.closures[id.0 as usize].kind == CallableKind::Fn)
                {
                    return Err(error(
                        "shared Fn entry cannot replace through an exclusive captured address",
                    ));
                }
                b.lookup(
                    effect.borrowed_fields.len(),
                    "replacement ordinary capture write effect",
                )?;
                effect.borrowed_fields.insert(capture);
                add(&mut effect.replacements, target, birth, b)?;
            }
            origins::Root::StoreElement(value) => {
                if !matches!(target.path.as_slice(), [PlaceProjection::Field(_)]) {
                    return Err(vec![Diagnostic::unsupported(
                        Phase::Analyze,
                        "Store replacement requires a direct complete owning field",
                        None,
                    )]);
                }
                // The actual borrow operation already proves complete element
                // initialization. Preserve its parent debt; no field is taken,
                // no prefix is extended and no fresh privilege is manufactured.
                let parent = copied(state, Place::Value(value), b)?;
                if check {
                    live_sources(state, &parent, b)?;
                }
                self.invalidate_store(
                    flow,
                    state,
                    effect,
                    &parent.sources,
                    CheckPoint {
                        validate: check,
                        live,
                        output: None,
                        position: None,
                    },
                    b,
                )?;
            }
        }
        Ok(())
    }
    fn substitute_birth(
        &self,
        state: &State,
        arguments: &[ValueId],
        birth: &BTreeSet<Birth>,
        b: &mut Budget,
    ) -> CompileResult<BTreeSet<Birth>> {
        fee(b, birth.len())?;
        let mut out = BTreeSet::new();
        for term in birth {
            let terms = match term {
                Birth::Argument(p) => {
                    copied(
                        state,
                        Place::Value(
                            *arguments
                                .get(*p as usize)
                                .ok_or_else(|| error("replacement RHS argument absent"))?,
                        ),
                        b,
                    )?
                    .birth
                }
                // Unknown incoming captures remain incoming. They never gain
                // Created from an entry result-mode or a public write claim.
                Birth::Captured(_) => BTreeSet::from([Birth::Incoming]),
                term => BTreeSet::from([*term]),
            };
            fee(b, out.len().saturating_add(terms.len()).saturating_add(1))?;
            out.extend(terms);
        }
        if out.is_empty() {
            out.insert(Birth::Incoming);
        }
        Ok(out)
    }
    fn call(
        &self,
        flow: &Flow<'_>,
        state: &mut State,
        effect: &mut Effect,
        invocation: Invocation<'_>,
        point: CheckPoint<'_>,
        b: &mut Budget,
    ) -> CompileResult<()> {
        let Invocation {
            callee,
            arguments,
            captures,
            carrier,
            sealed,
        } = invocation;
        let CheckPoint {
            validate: check,
            live,
            ..
        } = point;
        let rows = &flow.effects[callee].replacements;
        fee(b, rows.len())?;
        for (target, birth) in rows {
            fee(b, target.path.len().saturating_add(1))?;
            let selected = origins::substitute(
                &BTreeSet::from([target.clone()]),
                arguments,
                captures,
                &self.routes[flow.function.id.0 as usize].values,
                &self.routes[flow.function.id.0 as usize].slots,
                b,
            )?;
            if selected.is_empty() {
                return Err(error(
                    "replacement call has no actual selected destination mapping",
                ));
            }
            let incoming = self.substitute_birth(state, arguments, birth, b)?;
            for at in selected {
                if sealed && matches!(at.root, origins::Root::StoreElement(_)) {
                    return Err(vec![Diagnostic::unsupported(
                        Phase::Analyze,
                        "Store instance invalidation across a sealed callback scope needs its simultaneous descriptor proof",
                        None,
                    )]);
                }
                // The actual Commit keys form a may-retire projection. Their
                // birth rows remain conservative may-write unions; presence
                // neither proves must-Commit nor grants a fresh origin.
                retirement::check(flow, state, &at, point, b)?;
                self.update(flow, state, effect, &at, &incoming, check, live, b)?;
            }
        }
        if let Some(rows) = &flow.effects[callee].whole_commits {
            fee(b, rows.len())?;
            for (target, birth) in rows {
                let selected = origins::substitute(
                    &BTreeSet::from([target.clone()]),
                    arguments,
                    captures,
                    &self.routes[flow.function.id.0 as usize].values,
                    &self.routes[flow.function.id.0 as usize].slots,
                    b,
                )?;
                if selected.len() != 1 {
                    return Err(error(
                        "whole Package must-Commit requires one actual destination",
                    ));
                }
                let incoming = self.substitute_birth(state, arguments, birth, b)?;
                for at in selected {
                    package::postcondition(flow, state, &at, &incoming, check, b)?;
                }
            }
        }
        let invalidations = &flow.effects[callee].invalidated_stores;
        fee(b, invalidations.len())?;
        if sealed && !invalidations.is_empty() {
            return Err(vec![Diagnostic::unsupported(
                Phase::Analyze,
                "Store instance invalidation across a sealed callback scope needs its simultaneous descriptor proof",
                None,
            )]);
        }
        for invalidation in invalidations {
            let parents = match invalidation {
                StoreInvalidation::Input(p) => {
                    let value = arguments
                        .get(*p as usize)
                        .ok_or_else(|| error("Store invalidation input absent"))?;
                    copied(state, Place::Value(*value), b)?.sources
                }
                StoreInvalidation::BorrowedCapture(_) => {
                    let carrier = carrier
                        .ok_or_else(|| error("Store invalidation capture carrier absent"))?;
                    let fact = copied(state, carrier, b)?;
                    fee(b, fact.sources.len())?;
                    fact.sources
                        .into_iter()
                        .filter(|source| matches!(source, Source::StoreParent(_)))
                        .collect()
                }
            };
            self.invalidate_store(flow, state, effect, &parents, point, b)?;
        }
        Ok(())
    }
    pub(super) fn step(
        &self,
        flow: &Flow<'_>,
        state: &mut State,
        effect: &mut Effect,
        i: &Instruction,
        point: CheckPoint<'_>,
        b: &mut Budget,
    ) -> CompileResult<()> {
        let CheckPoint {
            validate: check,
            live,
            ..
        } = point;
        let f = flow.function;
        match &i.kind {
            InstructionKind::ReservePlaceLocal { reservation, .. }
            | InstructionKind::ReservePlaceAddress { reservation, .. } => {
                let d = self.definitions[f.id.0 as usize].definition(*reservation, b)?;
                for target in self.targets(f, &d.base, &d.path, b)? {
                    let ty = self.actual_type(flow.module, f, &target, b)?;
                    b.type_work(ty, true, "reserve actual endpoint Type equality")?;
                    if ty != &d.ty {
                        return Err(error(
                            "reservation declaration differs from actual selected owner",
                        ));
                    }
                    if check && let origins::Root::Local(local) = target.root {
                        self.initialized(flow, state, local, &target.path, b)?;
                    }
                }
            }
            InstructionKind::CommitPlaceReplace { reservation, value } => {
                let d = self.definitions[f.id.0 as usize].definition(*reservation, b)?;
                let rhs = copied(state, Place::Value(*value), b)?;
                if check && !exact(&rhs, Life::Available) {
                    return Err(error(
                        "replacement RHS responsibility is absent or already consumed",
                    ));
                }
                for target in self.targets(f, &d.base, &d.path, b)? {
                    // The old local path had the same missing consumer as a
                    // call. Check actual leaf retirement independently here.
                    if matches!(d.ty, Type::Managed(_)) {
                        retirement::check(flow, state, &target, point, b)?;
                    }
                    if matches!(d.ty, Type::Package { .. }) {
                        if matches!(target.root, origins::Root::Local(_)) {
                            package::apply(flow, state, &target, *value, point, b)?;
                        } else {
                            let index = package::borrowed(f, &target, b)?;
                            let mut leaf = target.clone();
                            fee(b, leaf.path.len().saturating_add(1))?;
                            leaf.path.push(PlaceProjection::Field(index));
                            retirement::check(flow, state, &leaf, point, b)?;
                            let incoming = product::birth(
                                state,
                                product::Carrier::Value(*value),
                                &[index],
                                b,
                            )?;
                            self.update(flow, state, effect, &leaf, &incoming, check, live, b)?;
                            if let Some(rows) = &mut state.whole_commits {
                                add(rows, &leaf, &incoming, b)?;
                            }
                        }
                    } else if matches!(d.ty, Type::Enum { .. }) {
                        whole_enum::apply(flow, state, &target, *value, point, b)?;
                    } else {
                        self.update(flow, state, effect, &target, &rhs.birth, check, live, b)?;
                    }
                }
                flow.consume(state, *value, check, b)?;
            }
            InstructionKind::Call {
                function,
                arguments,
            } => {
                let callee = &flow.module.functions[function.0 as usize];
                let summary = &flow.effects[function.0 as usize].replacements;
                if (callee.callable_contract.is_some() || callee.callable_return_contract.is_some())
                    && !summary.is_empty()
                {
                    return Err(vec![Diagnostic::unsupported(
                        Phase::Analyze,
                        "replacement through a callable helper needs its public replacement envelope",
                        Some(i.span),
                    )]);
                }
                self.call(
                    flow,
                    state,
                    effect,
                    Invocation {
                        callee: function.0 as usize,
                        arguments,
                        captures: None,
                        carrier: None,
                        sealed: false,
                    },
                    point,
                    b,
                )?;
            }
            InstructionKind::WithForeignCallback { callback, body, .. } => {
                for value in [*callback, *body] {
                    let entry = effect_entry(flow.module, &f.values[value.0 as usize])?;
                    self.call(
                        flow,
                        state,
                        effect,
                        Invocation {
                            callee: entry,
                            arguments: &[],
                            captures: Some(&self.routes[f.id.0 as usize].slots[value.0 as usize]),
                            carrier: Some(Place::Value(value)),
                            sealed: true,
                        },
                        point,
                        b,
                    )?;
                }
            }
            InstructionKind::WithScopedThread { body, .. }
            | InstructionKind::WithSyncCell { body, .. } => {
                let entry = effect_entry(flow.module, &f.values[body.0 as usize])?;
                self.call(
                    flow,
                    state,
                    effect,
                    Invocation {
                        callee: entry,
                        arguments: &[],
                        captures: Some(&self.routes[f.id.0 as usize].slots[body.0 as usize]),
                        carrier: Some(Place::Value(*body)),
                        sealed: true,
                    },
                    point,
                    b,
                )?;
            }
            InstructionKind::CallClosureShared { callee, arguments }
            | InstructionKind::CallClosureMut { callee, arguments } => {
                let entry = effect_entry(flow.module, &f.locals[callee.0 as usize].ty)?;
                let facts = &self.routes[f.id.0 as usize];
                b.lookup(
                    facts.local_slots.len(),
                    "replacement selected local captures",
                )?;
                self.call(
                    flow,
                    state,
                    effect,
                    Invocation {
                        callee: entry,
                        arguments,
                        captures: facts.local_slots.get(callee),
                        carrier: Some(Place::Local(*callee)),
                        sealed: false,
                    },
                    point,
                    b,
                )?;
            }
            InstructionKind::CallClosureOnce { callee, arguments }
            | InstructionKind::CallClosure { callee, arguments }
            | InstructionKind::CallClosureBorrowed {
                callee, arguments, ..
            } => {
                let entry = effect_entry(flow.module, &f.values[callee.0 as usize])?;
                self.call(
                    flow,
                    state,
                    effect,
                    Invocation {
                        callee: entry,
                        arguments,
                        captures: Some(&self.routes[f.id.0 as usize].slots[callee.0 as usize]),
                        carrier: Some(Place::Value(*callee)),
                        sealed: false,
                    },
                    point,
                    b,
                )?;
            }
            _ => {}
        }
        Ok(())
    }
}
pub(super) fn infer(
    module: &TypedModule,
    graphs: &[(Vec<Vec<usize>>, Vec<bool>)],
    program: &Program,
    b: &mut Budget,
) -> CompileResult<Vec<Effect>> {
    fee(b, module.functions.len())?;
    let mut effects = vec![Effect::default(); module.functions.len()];
    // Replacement rows can change later resource transfers. Rerun each body
    // against the current summaries, then validate only after convergence.
    loop {
        let mut changed = false;
        let mut musts = Vec::new();
        for (f, (_, reachable)) in module.functions.iter().zip(graphs) {
            fee(b, 1)?;
            let found = analyze_function_program(
                module,
                f,
                reachable,
                &effects,
                false,
                b,
                None,
                Some(program),
            )?;
            if program.whole_helper(f.id, b)? {
                fee(b, 1)?;
                must_cost(found.whole_commits.as_ref(), b)?;
                musts.push((f.id, found.whole_commits.clone()));
            }
            changed |= grow_effect(&mut effects[f.id.0 as usize], found, b)?;
        }
        if !changed {
            for (id, rows) in musts {
                effects[id.0 as usize].whole_commits = rows;
            }
            return Ok(effects);
        }
    }
}

#[cfg(test)]
mod tests;

// A normal-return must relation is distinct from the monotone may-write ledger.
pub(super) fn must_cost(rows: Option<&Effects>, b: &mut Budget) -> CompileResult<()> {
    if let Some(rows) = rows {
        fee(b, rows.len())?;
        for (target, birth) in rows {
            fee(
                b,
                target
                    .path
                    .len()
                    .saturating_add(birth.len())
                    .saturating_add(1),
            )?;
        }
    }
    Ok(())
}
pub(super) fn must_join(
    to: &mut Option<Effects>,
    from: &Option<Effects>,
    b: &mut Budget,
) -> CompileResult<bool> {
    let (Some(to), Some(from)) = (to, from) else {
        return Ok(false);
    };
    must_cost(Some(to), b)?;
    must_cost(Some(from), b)?;
    let mut changed = false;
    let mut remove = Vec::new();
    for (target, birth) in to.iter_mut() {
        row_fee(from, target, b)?;
        if let Some(other) = from.get(target) {
            fee(b, birth.len().saturating_add(other.len()))?;
            let old = birth.len();
            birth.extend(other);
            changed |= birth.len() != old;
        } else {
            fee(b, target.path.len().saturating_add(1))?;
            remove.push(target.clone());
        }
    }
    for target in remove {
        row_fee(to, &target, b)?;
        to.remove(&target);
        changed = true;
    }
    Ok(changed)
}
