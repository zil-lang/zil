//! Original-value use state is separate from each materialized resource graph.
//! A concrete Copy specialization cannot justify a second use of an abstract T.
use super::*;
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Binder {
    Parameter {
        definition: SemanticDefinitionId,
        ordinal: u32,
    },
    Local {
        definition: SemanticDefinitionId,
        node: NodeId,
        name: String,
    },
    Pattern {
        definition: SemanticDefinitionId,
        token: FrameToken,
    },
    Capture {
        definition: SemanticDefinitionId,
        source: Box<Binder>,
    },
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct Place {
    pub(super) binder: Binder,
    pub(super) fields: Vec<String>,
}
impl CheckedOriginal {
    pub(super) fn infer_owner_captures(&mut self, child: &SemanticDefinition, bindings: &BTreeMap<String, Binding>, b: &mut Budget) -> Check<()> {
        let mut inferred = BTreeSet::new();
        for (name, binding) in bindings {
            if !matches!(self.resolved(&binding.ty,b)?, Ty::Managed(_)) { continue; }
            if zil_ir::original_capture_use(child, name, zil_ir::CaptureUse::Give,
                |work, depth, _| {
                    if depth > b.limits.max_call_depth { return Err(limit("original capture ownership depth exceeded")); }
                    b.spend(work)
                },
                |message, span| unsupported(message, Some(span)),
            )?.is_some() {
                if matches!(binding.identity, Binder::Capture { .. }) && !self.capture_owns(&binding.identity, b)? {
                    return Err(bad("original give cannot transfer an already borrowed creator capture"));
                }
                binding.identity.charge_clone(b)?;
                b.spend(inferred.len().saturating_add(1).ilog2() as usize + 2)?;
                inferred.insert(Binder::Capture {
                    definition: child.id,
                    source: Box::new(binding.identity.clone()),
                });
            }
        }
        if !inferred.is_empty() {
            b.spend(self.inferred_owner_captures.len().saturating_add(1).ilog2() as usize + 1)?;
            self.inferred_owner_captures.insert(child.id, inferred);
        }
        Ok(())
    }
    pub(super) fn capture_owns(&self, binder: &Binder, b: &mut Budget) -> Check<bool> {
        let Binder::Capture { definition, .. } = binder else { return Ok(false); };
        if self.capture_creator_moves(*definition)? { return Ok(true); }
        if self.inferred_owner_captures.is_empty() { return Ok(false); }
        b.spend(self.inferred_owner_captures.len().saturating_add(1).ilog2() as usize + 1)?;
        let Some(rows) = self.inferred_owner_captures.get(definition) else { return Ok(false); };
        for _ in 0..=rows.len().saturating_add(1).ilog2() {
            binder.charge_clone(b)?;
        }
        Ok(rows.contains(binder))
    }
    pub(super) fn capture_field_owns(&self, definition: SemanticDefinitionId, field: &str, b: &mut Budget) -> Check<bool> {
        if self.capture_creator_moves(definition)? { return Ok(true); }
        if self.inferred_owner_captures.is_empty() { return Ok(false); }
        let name = field.strip_prefix("$capture:")
            .ok_or_else(|| bad("original owning capture field has no lexical namesake"))?;
        b.spend(self.free_values.len().saturating_add(1).ilog2() as usize + 1)?;
        let Some(rows) = self.free_values.get(&definition) else { return Ok(false); };
        b.spend(name.len().saturating_add(1).saturating_mul(rows.len().saturating_add(1).ilog2() as usize + 1))?;
        let Some(binding) = rows.get(name) else { return Ok(false); };
        binding.identity.charge_clone(b)?;
        self.capture_owns(&Binder::Capture {
            definition,
            source: Box::new(binding.identity.clone()),
        }, b)
    }
    pub(super) fn capture_creator_moves(&self, definition: SemanticDefinitionId) -> Check<bool> {
        self.creator_moves.get(&definition).copied()
            .ok_or_else(|| bad("original capture has no exact creator mode"))
    }
}

impl Binder {
    pub(super) fn charge_clone(&self, b: &mut Budget) -> Check<()> {
        let mut binder = self;
        loop {
            b.spend(1)?;
            match binder {
                Self::Capture { source, .. } => binder = source,
                Self::Local { name, .. } => return b.spend(name.len()),
                _ => return Ok(()),
            }
        }
    }
}
#[derive(Clone, Default)]
pub(super) struct Uses {
    pub(super) moved: BTreeSet<Place>,
    pub(super) definite: BTreeSet<Place>,
    writes: Option<std::rc::Rc<BTreeMap<NodeId, OriginalSlotWrite>>>,
}
#[derive(Default)]
pub(super) struct LoopExits<'a> {
    // Absent while checking the syntax of an unreachable continuation.
    pub(super) outer: Option<&'a BTreeSet<Binder>>,
    pub(super) breaks: Option<Uses>,
    pub(super) backedges: Option<Uses>,
}

impl LoopExits<'_> {
    pub(super) fn merge(slot: &mut Option<Uses>, state: Uses, b: &mut Budget) -> Check<()> {
        if let Some(previous) = slot {
            previous.join(&state, b)?;
        } else {
            *slot = Some(state);
        }
        Ok(())
    }
}

impl Uses {
    pub(super) fn writes(&self) -> &BTreeMap<NodeId, OriginalSlotWrite> {
        static EMPTY:BTreeMap<NodeId,OriginalSlotWrite>=BTreeMap::new();
        self.writes.as_deref().unwrap_or(&EMPTY)
    }
    pub(super) fn edit_writes(&mut self,b:&mut Budget)->Check<&mut BTreeMap<NodeId,OriginalSlotWrite>> {
        let map=self.writes.get_or_insert_with(||std::rc::Rc::new(BTreeMap::new()));
        if std::rc::Rc::strong_count(map)>1 {b.spend(map.len())?;}
        Ok(std::rc::Rc::make_mut(map))
    }
    pub(super) fn navigation_conflicts(&self, place: &Place, b: &mut Budget) -> Check<bool> {
        b.spend(
            self.moved
                .len()
                .saturating_mul(place.fields.len().saturating_add(1)),
        )?;
        Ok(self
            .moved
            .iter()
            .any(|old| old.binder == place.binder && place.fields.starts_with(&old.fields)))
    }
    pub(super) fn conflicts(&self, place: &Place, b: &mut Budget) -> Check<bool> {
        b.spend(
            self.moved
                .len()
                .saturating_mul(place.fields.len().saturating_add(1)),
        )?;
        Ok(self.moved.iter().any(|old| {
            old.binder == place.binder
                && (old.fields.starts_with(&place.fields) || place.fields.starts_with(&old.fields))
        }))
    }
    pub(super) fn initialize(&mut self, place: &Place, b: &mut Budget) -> Check<()> {
        b.spend(
            self.moved
                .len()
                .saturating_mul(place.fields.len().saturating_add(1)),
        )?;
        if self.moved.iter().any(|old| {
            old.binder == place.binder
                && old.fields.len() < place.fields.len()
                && place.fields.starts_with(&old.fields)
        }) {
            return Err(bad(
                "original partial assignment cannot resurrect a moved ancestor",
            ));
        }
        self.moved
            .retain(|old| old.binder != place.binder || !old.fields.starts_with(&place.fields));
        self.definite
            .retain(|old| old.binder != place.binder || !old.fields.starts_with(&place.fields));
        Ok(())
    }
    pub(super) fn join(&mut self, other: &Self, b: &mut Budget) -> Check<()> {
        b.spend(
            self.definite
                .len()
                .saturating_mul(other.definite.len().saturating_add(1))
                .saturating_add(self.moved.len())
                .saturating_add(other.moved.len()),
        )?;
        let mut definite = BTreeSet::new();
        for place in &self.definite {
            if other.definite.iter().any(|other| {
                other.binder == place.binder && place.fields.starts_with(&other.fields)
            }) {
                definite.insert(place.clone());
            }
        }
        for place in &other.definite {
            if self.definite.iter().any(|other| {
                other.binder == place.binder && place.fields.starts_with(&other.fields)
            }) {
                definite.insert(place.clone());
            }
        }
        // A strong write survives a join only on agreement. Exposed slots
        // stay exposed on either arm and cannot be refined on a later write.
        let shared=match (&self.writes,&other.writes){
            (Some(left),Some(right))=>std::rc::Rc::ptr_eq(left,right),
            (None,None)=>true,
            _=>false,
        };
        if !shared {
            let next=other.writes();
            let states=self.edit_writes(b)?;
            // The declaration initializes this state for every live tracked
            // slot. A binding present on only one arm is local to that arm.
            b.spend(states.len().saturating_mul(next.len().saturating_add(1).ilog2() as usize+2))?;
            states.retain(|node,_|next.contains_key(node));
            for (node,mark) in states.iter_mut() {
                b.spend(next.len().saturating_add(1).ilog2() as usize+2)?;
                *mark=mark.merged(&next[node],|cost|b.spend(cost))?;
            }
        }
        self.moved.extend(other.moved.iter().cloned());
        self.definite = definite;
        Ok(())
    }
}
impl CheckedOriginal {
    /// Selecting a product child observes its parent shell, not all sibling
    /// values. A moved parent/ancestor is still a read conflict; a moved
    /// descendant does not stop a disjoint field use or exact field refill.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn read_original_parent(
        &mut self,
        module: &TypedModule,
        context: &CheckedContext,
        place: &Place,
        ty: &Ty,
        env: &Env,
        active: &mut ActiveQueries,
        b: &mut Budget,
    ) -> Check<()> {
        let id = env.current()?;
        b.spend(1)?;
        self.used
            .entry(id)
            .or_default()
            .insert(place.binder.clone());
        if self
            .uses
            .get(&id)
            .map(|uses| uses.navigation_conflicts(place, b))
            .transpose()?
            .unwrap_or(false)
        {
            self.read_original(module, context, place, ty, env, false, active, b)?;
        }
        Ok(())
    }
    pub(super) fn is_copy(
        &mut self,
        module: &TypedModule,
        context: &CheckedContext,
        ty: &Ty,
        env: &Env,
        active: &mut ActiveQueries,
        b: &mut Budget,
    ) -> Check<bool> {
        let token = active.fresh(b)?;
        active.with_query(
            DependencyKey {
                kind: QueryKind::Type,
                token,
            },
            b,
            |active, b| {
                let ty = self.resolved(ty, b)?;
                Ok(match &ty {
                    Ty::SealedError(_) => true,
                    Ty::Atom(ty) => {
                        ty.is_b0_scalar() || matches!(ty, Type::ForeignHandle | Type::CWideString)
                    }
                    Ty::Address { mutable, .. } => !*mutable,
                    Ty::Desc { .. } | Ty::Code { .. } | Ty::Function(_) => true,
                    Ty::Tuple(fields) => {
                        for field in fields {
                            if !self.is_copy(module, context, field, env, active, b)? {
                                return Ok(false);
                            }
                        }
                        true
                    }
                    Ty::ErrorUnion(error, result) => {
                        self.is_copy(module, context, error, env, active, b)?
                            && self.is_copy(module, context, result, env, active, b)?
                    }
                    Ty::Array(element, _) => {
                        self.is_copy(module, context, element, env, active, b)?
                    }
                    Ty::Nominal(id, arguments) => {
                        let definition = context.definition(module, *id, b)?;
                        let SemanticDefinitionBody::Nominal { kind, body, .. } = &definition.body
                        else {
                            return Err(bad("original Copy query has no exact nominal body"));
                        };
                        if !matches!(kind, LifeNominalKind::Record | LifeNominalKind::Enum)
                            || self.custom_drop(module, *id, b)?
                        {
                            return Ok(false);
                        }
                        let mut selected = self.environment(module, context, *id, active, b)?;
                        selected.lexical[0].actuals = arguments.clone();
                        match body {
                            SemanticNominalBody::Product { fields } => {
                                for field in fields {
                                    let field =
                                        self.ty(module, context, &field.ty, &selected, active, b)?;
                                    if !self.is_copy(module, context, &field, env, active, b)? {
                                        return Ok(false);
                                    }
                                }
                            }
                            SemanticNominalBody::Enum { variants } => {
                                for variant in variants {
                                    for field in &variant.fields {
                                        let field =
                                            self.ty(module, context, field, &selected, active, b)?;
                                        if !self.is_copy(module, context, &field, env, active, b)? {
                                            return Ok(false);
                                        }
                                    }
                                }
                            }
                        }
                        true
                    }
                    Ty::Variable(..) | Ty::SelfType(_) | Ty::Projection { .. } => {
                        if self.structural_premise(
                            module,
                            context,
                            &ty,
                            DynStructuralBound::Copy,
                            env,
                            active,
                            b,
                        )? {
                            return Ok(true);
                        }
                        let equations = self
                            .current
                            .and_then(|id| self.equalities.get(&id))
                            .cloned()
                            .unwrap_or_default();
                        for (left, right) in equations {
                            for candidate in [left, right] {
                                if !matches!(
                                    candidate,
                                    Ty::Variable(..) | Ty::SelfType(_) | Ty::Infer(_)
                                ) && self.same(&ty, &candidate, b)?
                                    && self.is_copy(module, context, &candidate, env, active, b)?
                                {
                                    return Ok(true);
                                }
                            }
                        }
                        false
                    }
                    Ty::Closure(definition, arguments, sealed) => {
                        if !self.checked.contains(definition) {
                            return Ok(false);
                        }
                        let Some(kind) = self
                            .prototypes
                            .get(definition)
                            .and_then(|prototype| prototype.callable_kind)
                        else {
                            return Ok(false);
                        };
                        if !matches!(kind, CallableKind::Fn | CallableKind::FnMut) {
                            return Ok(false);
                        }
                        let captures = self.closure_capture_types(
                            module,
                            context,
                            *definition,
                            arguments,
                            sealed,
                            b,
                        )?;
                        let moves =
                            self.creator_moves.get(definition).copied().ok_or_else(|| {
                                bad("original environment Copy query lacks its exact capture mode")
                            })?;
                        if !moves {
                            // A checked default Fn stores shared observations,
                            // rather than copies/owners of the free values. An
                            // exclusive default FnMut environment is not Copy.
                            captures.is_empty() || kind == CallableKind::Fn
                        } else {
                            for capture in captures {
                                if !self.is_copy(module, context, &capture, env, active, b)? {
                                    return Ok(false);
                                }
                            }
                            true
                        }
                    }
                    Ty::Opaque(owner, arguments) => {
                        // Copy follows an actual complete concrete environment,
                        // never a callable bound, hidden generic T, Value or an
                        // advertised consuming-only FnOnce interface.
                        let advertised =
                            self.opaque_callable(module, context, *owner, arguments, active, b)?;
                        if !advertised.is_some_and(|prototype| {
                            matches!(
                                prototype.callable_kind,
                                Some(CallableKind::Fn | CallableKind::FnMut)
                            )
                        }) {
                            return Ok(false);
                        }
                        let hidden =
                            self.opaque_hidden(module, context, *owner, arguments, active, b)?;
                        if !matches!(hidden, Ty::Closure(..)) {
                            return Ok(false);
                        }
                        self.is_copy(module, context, &hidden, env, active, b)?
                    }
                    Ty::Infer(id) => self.integer_variables.contains(id),
                    _ => false,
                })
            },
        )
    }
}
impl CheckedOriginal {
    pub(super) fn read_original(
        &mut self,
        module: &TypedModule,
        context: &CheckedContext,
        place: &Place,
        ty: &Ty,
        env: &Env,
        consume: bool,
        active: &mut ActiveQueries,
        b: &mut Budget,
    ) -> Check<()> {
        let id = env.current()?;
        self.used
            .entry(id)
            .or_default()
            .insert(place.binder.clone());
        if self.is_copy(module, context, ty, env, active, b)? {
            return Ok(());
        }
        let conflict = match self.uses.get(&id) {
            Some(uses) => uses.conflicts(place, b)?,
            None => false,
        };
        if conflict {
            if has_inference(ty, &self.substitutions, b)? {
                b.spend(env.lexical.len().saturating_add(1))?;
                self.copy_obligations.push((ty.clone(), env.clone()));
            } else {
                return Err(bad(
                    "original abstract value is used after move without an original Copy proof",
                ));
            }
        }
        if consume {
            b.spend(place.fields.len().saturating_add(1))?;
            let uses = self.uses.entry(id).or_default();
            uses.moved.insert(place.clone());
            uses.definite.insert(place.clone());
        }
        Ok(())
    }
    pub(super) fn expose_slot(&mut self,binder:&Binder,b:&mut Budget)->Check<()> {
        if let Binder::Local{definition,node,..}=binder && let Some(uses)=self.uses.get_mut(definition) && !uses.writes().is_empty(){
            b.spend(uses.writes().len().ilog2() as usize+2)?;
            if let Some(mark)=uses.edit_writes(b)?.get_mut(node){*mark=OriginalSlotWrite::Exposed;}
        }
        Ok(())
    }
    pub(super) fn use_snapshot(&self, env: &Env, b: &mut Budget) -> Check<Uses> {
        let uses = self.uses.get(&env.current()?).cloned().unwrap_or_default();
        b.spend(uses.moved.len())?;
        Ok(uses)
    }
    pub(super) fn set_uses(&mut self, env: &Env, uses: Uses) -> Check<()> {
        self.uses.insert(env.current()?, uses);
        Ok(())
    }
    pub(super) fn join_uses(
        &mut self,
        env: &Env,
        left: &Uses,
        right: &Uses,
        b: &mut Budget,
    ) -> Check<()> {
        let mut joined = left.clone();
        b.spend(left.moved.len())?;
        joined.join(right, b)?;
        self.set_uses(env, joined)
    }
}
pub(super) fn has_inference(
    ty: &Ty,
    substitutions: &BTreeMap<u64, Ty>,
    b: &mut Budget,
) -> Check<bool> {
    unresolved_inference(ty, substitutions, |_| true, b)
}

pub(super) fn unresolved_inference(
    ty: &Ty,
    substitutions: &BTreeMap<u64, Ty>,
    matches: impl Fn(u64) -> bool,
    b: &mut Budget,
) -> Check<bool> {
    let mut pending = vec![ty];
    let mut seen = BTreeSet::new();
    while let Some(ty) = pending.pop() {
        b.spend(1)?;
        match ty {
            Ty::Infer(id) => {
                if !seen.insert(*id) {
                    continue;
                }
                match substitutions.get(id) {
                    Some(ty) => pending.push(ty),
                    None if matches(*id) => return Ok(true),
                    None => {}
                }
            }
            Ty::Tuple(fields) => pending.extend(fields),
            Ty::Address { inner, .. }
            | Ty::Desc { inner, .. }
            | Ty::Managed(inner)
            | Ty::Store(inner)
            | Ty::Slice(inner)
            | Ty::Array(inner, _) => pending.push(inner),
            Ty::Nominal(_, arguments)
            | Ty::Opaque(_, arguments)
            | Ty::Closure(_, arguments, _)
            | Ty::Projection { arguments, .. } => {
                if let Ty::Projection { receiver, .. } = ty {
                    pending.push(receiver);
                }
                for argument in arguments {
                    match argument {
                        Arg::Type(ty) => pending.push(ty),
                        Arg::Const(value) => pending.push(&value.carrier),
                        _ => {}
                    }
                }
            }
            Ty::Dyn(value) => value.types(&mut pending, b)?,
            Ty::Carrier {
                child, retained, ..
            } => {
                pending.push(child);
                retained.types(&mut pending, b)?;
            }
            Ty::ErrorUnion(error, result) => {
                pending.push(error);
                pending.push(result);
            }
            Ty::Code {
                parameters, result, ..
            } => {
                pending.extend(parameters);
                pending.push(result);
            }
            _ => {}
        }
    }
    Ok(false)
}
impl CheckedOriginal {
    pub(super) fn branch_uses(
        &mut self,
        env: &Env,
        before: &Uses,
        yes: &Uses,
        yes_terminal: bool,
        no: &Uses,
        no_terminal: bool,
        b: &mut Budget,
    ) -> Check<()> {
        match (yes_terminal, no_terminal) {
            (true, true) => self.set_uses(env, before.clone()),
            (true, false) => self.set_uses(env, no.clone()),
            (false, true) => self.set_uses(env, yes.clone()),
            (false, false) => self.join_uses(env, yes, no, b),
        }
    }
    pub(super) fn project_loop_uses(
        &self, env: &Env, outer: &BTreeSet<Binder>, b: &mut Budget,
    ) -> Check<Uses> {
        let mut state = Uses::default();
        let Some(source) = self.uses.get(&env.current()?) else { return Ok(state); };
        if !source.writes().is_empty() {
            for binder in outer {
                b.spend(source.writes().len().ilog2() as usize+2)?;
                if let Binder::Local{node,..}=binder && let Some(mark)=source.writes().get(node) {
                    b.spend(state.writes().len().saturating_add(1).ilog2() as usize+2)?;
                    state.edit_writes(b)?.insert(*node,mark.clone());
                }
            }
        }
        for (places, projected) in [(&source.moved, &mut state.moved), (&source.definite, &mut state.definite)] {
            for place in places {
                for _ in 0..=outer.len().saturating_add(1).ilog2() {
                    place.binder.charge_clone(b)?;
                }
                if outer.contains(&place.binder) {
                    for _ in 0..=projected.len().saturating_add(1).ilog2() {
                        place.binder.charge_clone(b)?;
                        b.spend(place.fields.len())?;
                        for field in &place.fields { b.spend(field.len())?; }
                    }
                    projected.insert(place.clone());
                }
            }
        }
        Ok(state)
    }

    pub(super) fn loop_jump(
        &mut self, env: &Env, exits: &mut LoopExits<'_>, is_break: bool, b: &mut Budget,
    ) -> Check<()> {
        let Some(outer) = exits.outer else { return Ok(()); };
        // Discharge only scopes exited by this jump. The caller's lexical
        // tables remain in place for independently checked sibling branches.
        let mut leaving = BTreeSet::new();
        if let Some(live) = self.live.get(&env.current()?) {
            for key in live.keys() {
                for _ in 0..=outer.len().saturating_add(1).ilog2() {
                    key.charge_clone(b)?;
                }
                if !outer.contains(key) {
                    for _ in 0..=leaving.len().saturating_add(1).ilog2() {
                        key.charge_clone(b)?;
                    }
                    leaving.insert(key.clone());
                }
            }
        }
        if !leaving.is_empty() {
            self.queue_cleanup(env, Some(&leaving), b)?;
        }
        let state = self.project_loop_uses(env, outer, b)?;
        let target = if is_break { &mut exits.breaks } else { &mut exits.backedges };
        LoopExits::merge(target, state, b)
    }

    pub(super) fn loop_uses(
        &mut self,
        module: &TypedModule,
        context: &CheckedContext,
        env: &Env,
        before: &Uses,
        after: &Uses,
        bindings: &BTreeMap<String, Binding>,
        active: &mut ActiveQueries,
        b: &mut Budget,
    ) -> Check<()> {
        for place in after.moved.difference(&before.moved) {
            b.spend(bindings.len().saturating_add(place.fields.len()))?;
            if let Some(binding) = bindings
                .values()
                .find(|binding| binding.identity == place.binder)
            {
                if !self.is_copy(module, context, &binding.ty, env, active, b)? {
                    if has_inference(&binding.ty, &self.substitutions, b)? {
                        self.copy_obligations
                            .push((binding.ty.clone(), env.clone()));
                    } else {
                        return Err(bad(
                            "original loop backedge repeats a move of a non-Copy outer value",
                        ));
                    }
                }
            }
        }
        Ok(())
    }
}
impl CheckedOriginal {
    pub(super) fn custom_drop(
        &self,
        module: &TypedModule,
        definition: SemanticDefinitionId,
        b: &mut Budget,
    ) -> Check<bool> {
        let program = module
            .semantic
            .as_ref()
            .ok_or_else(|| bad("original Drop declaration directory absent"))?;
        b.spend(program.definitions.len())?;
        for row in &program.definitions {
            if let SemanticDefinitionBody::Scope(SemanticScopeRecipe::BuiltinImpl {
                self_type,
                trait_kind: SemanticBuiltinTrait::Drop,
            }) = &row.body
            {
                let mut receiver = self_type;
                while let SemanticTypeRecipe::Qualified { storage, .. } = receiver {
                    b.spend(1)?;
                    receiver = storage;
                }
                match receiver {
                    SemanticTypeRecipe::Nominal {
                        definition: actual, ..
                    } if *actual == definition => return Ok(true),
                    SemanticTypeRecipe::Nominal { .. } => {}
                    _ => {
                        return Err(unsupported(
                            "original generic Drop receiver requires complete original witness matching",
                            None,
                        ));
                    }
                }
            }
        }
        Ok(false)
    }
    pub(super) fn require_copy(
        &mut self,
        module: &TypedModule,
        context: &CheckedContext,
        ty: &Ty,
        env: &Env,
        active: &mut ActiveQueries,
        b: &mut Budget,
    ) -> Check<()> {
        if self.is_copy(module, context, ty, env, active, b)? {
            return Ok(());
        }
        if has_inference(ty, &self.substitutions, b)? {
            self.copy_obligations.push((ty.clone(), env.clone()));
            Ok(())
        } else {
            Err(bad(
                "original explicit Copy operation lacks an original Copy proof",
            ))
        }
    }
}

#[cfg(test)]
mod partial_place_tests {
    use super::*;
    #[test]
    fn field_refill_keeps_parent_live_without_reading_moved_siblings() {
        let limits = Limits::default();
        let (module, _) = zil_ir::wire::decode_module_candidate(
            include_bytes!("../fixtures/ordinary-identity-complete-head.wire"),
            &limits,
        )
        .unwrap();
        let mut b = Budget::new(&limits);
        let context = CheckedContext::catalogue(&module, &mut b).unwrap();
        let mut active = ActiveQueries::new();
        let mut checked = CheckedOriginal::check(&module, &context, &mut active, &mut b).unwrap();
        let id = module
            .semantic
            .as_ref()
            .unwrap()
            .definitions
            .iter()
            .find(|row| {
                row.kind == SemanticDefinitionKind::Function
                    && matches!(row.parameters.as_slice(), [SemanticFormal::Type { .. }])
            })
            .unwrap()
            .id;
        checked.current = Some(id);
        let env = checked.prototypes[&id].environment.clone();
        let f = Ty::Variable(id, 0);
        let whole = Ty::Tuple(vec![f.clone(), f.clone(), Ty::Atom(Type::I32)]);
        assert!(
            !checked
                .is_copy(&module, &context, &f, &env, &mut active, &mut b)
                .unwrap()
        );
        assert!(
            !checked
                .sync_drop(&module, &context, &f, &env, &mut active, &mut b)
                .unwrap()
        );
        let binder = Binder::Local {
            definition: id,
            node: NodeId(900),
            name: "next".into(),
        };
        let root = Place {
            binder: binder.clone(),
            fields: vec![],
        };
        let a = Place {
            binder: binder.clone(),
            fields: vec!["0".into()],
        };
        let sibling = Place {
            binder: binder.clone(),
            fields: vec!["1".into()],
        };
        let binding = Binding {
            ty: whole.clone(),
            mutable: true,
            identity: binder,
        };
        let bindings = BTreeMap::from([("next".into(), binding)]);
        checked.set_uses(&env, Uses::default()).unwrap();
        checked
            .read_original(&module, &context, &a, &f, &env, true, &mut active, &mut b)
            .unwrap();
        checked
            .read_original(
                &module,
                &context,
                &sibling,
                &f,
                &env,
                false,
                &mut active,
                &mut b,
            )
            .unwrap();
        assert!(
            checked
                .read_original(
                    &module,
                    &context,
                    &root,
                    &whole,
                    &env,
                    false,
                    &mut active,
                    &mut b
                )
                .is_err()
        );
        assert!(
            checked
                .read_original(&module, &context, &a, &f, &env, false, &mut active, &mut b)
                .is_err()
        );
        let span = zil_base::Span::default();
        let target = SemanticOriginalExpr {
            id: NodeId(901),
            kind: CoreExprKind::Member {
                object: Box::new(SemanticOriginalExpr {
                    id: NodeId(902),
                    kind: CoreExprKind::Name("next".into()),
                    span,
                }),
                member: "0".into(),
                owner: None,
            },
            span,
        };
        let (ty, place) = checked
            .place(
                &module,
                &context,
                &target,
                &env,
                &bindings,
                &mut active,
                &mut b,
            )
            .unwrap();
        checked
            .mutable_place(
                &module,
                &context,
                &target,
                &env,
                &bindings,
                &mut active,
                &mut b,
            )
            .unwrap();
        checked
            .overwrite_cleanup(&env, target.id, &ty, place.as_ref(), &mut b)
            .unwrap();
        checked
            .uses
            .get_mut(&id)
            .unwrap()
            .initialize(&a, &mut b)
            .unwrap();
        checked
            .check_cleanups(&module, &context, &mut active, &mut b)
            .unwrap();
        checked.current = Some(id);
        checked
            .read_original(
                &module,
                &context,
                &root,
                &whole,
                &env,
                false,
                &mut active,
                &mut b,
            )
            .unwrap();
        // Copy fields do not revive a moved non-Copy parent. A different
        // target likewise cannot refill the missing field or resurrect it.
        checked
            .set_uses(
                &env,
                Uses {
                    moved: BTreeSet::from([root.clone()]),
                    definite: BTreeSet::from([root]),
                    ..Default::default()
                },
            )
            .unwrap();
        let mut copy_field = target.clone();
        let CoreExprKind::Member { member, .. } = &mut copy_field.kind else {
            unreachable!()
        };
        *member = "2".into();
        assert!(
            checked
                .place(
                    &module,
                    &context,
                    &copy_field,
                    &env,
                    &bindings,
                    &mut active,
                    &mut b
                )
                .is_err()
        );
        assert!(
            checked
                .uses
                .get_mut(&id)
                .unwrap()
                .initialize(&a, &mut b)
                .is_err()
        );
        let mut state = Uses {
            moved: BTreeSet::from([a.clone()]),
            definite: BTreeSet::from([a.clone()]),
            ..Default::default()
        };
        state.initialize(&sibling, &mut b).unwrap();
        assert!(state.conflicts(&a, &mut b).unwrap());
        let mut restored = state.clone();
        restored.initialize(&a, &mut b).unwrap();
        let mut one_branch = restored.clone();
        one_branch.join(&state, &mut b).unwrap();
        assert!(one_branch.conflicts(&a, &mut b).unwrap());
        restored.join(&restored.clone(), &mut b).unwrap();
        assert!(!restored.conflicts(&a, &mut b).unwrap());
    }
}

#[cfg(test)]
mod concrete_environment_copy_tests {
    use super::*;
    #[test]
    fn concrete_environment_copy_requires_actual_capture_subjects_and_nonconsuming_mode_raw_wire() {
        let limits = Limits::default();
        let (original, plan) = zil_ir::wire::decode_module_candidate(
            include_bytes!("../fixtures/local-closure-complete-creator.wire"),
            &limits,
        )
        .unwrap();
        let bytes = zil_ir::wire::encode_module_candidate(&original, &plan, &limits).unwrap();
        let decoded = zil_ir::wire::decode_module_candidate(&bytes, &limits)
            .unwrap()
            .0;
        for module in [&original, &decoded] {
            let mut b = Budget::new(&limits);
            let context = CheckedContext::catalogue(module, &mut b).unwrap();
            let mut active = ActiveQueries::new();
            let mut checked =
                CheckedOriginal::check(module, &context, &mut active, &mut b).unwrap();
            let ty = checked
                .expressions
                .values()
                .find_map(|ty| matches!(ty, Ty::Closure(..)).then(|| ty.clone()))
                .expect("the complete original contains its real closure creation");
            let Ty::Closure(definition, arguments, sealed) = &ty else {
                unreachable!()
            };
            let row = context.definition(module, *definition, &mut b).unwrap();
            let parent = row.parent.unwrap();
            let env = checked.prototypes[&parent].environment.clone();
            checked.current = Some(parent);
            assert!(
                checked
                    .is_copy(module, &context, &ty, &env, &mut active, &mut b)
                    .unwrap()
            );
            let capture_mode = checked.creator_moves[definition];
            checked.creator_moves.insert(*definition, true);
            assert!(
                checked
                    .is_copy(module, &context, &ty, &env, &mut active, &mut b)
                    .unwrap(),
                "a concrete scalar value capture is Copy"
            );
            // Component subjects below are deliberately changed after the
            // real original check. They test the Copy predicate, not admission
            // of an altered body, creator, source or cleanup proof.
            let name = checked.free_values[definition]
                .keys()
                .next()
                .unwrap()
                .clone();
            let old = checked.free_values[definition][&name].ty.clone();
            let owner = Ty::Managed(Box::new(Ty::Atom(Type::I32)));
            let mut owning = sealed.clone();
            if let Some(ordinal) = row.environment.iter().position(|formal| matches!(&formal.source,
                SemanticActualRecipe::Type(SemanticTypeRecipe::OriginalType { source: SemanticOriginalTypeSource::NameAt { name: source, .. }, .. }) if *source == name)) {
                owning[ordinal] = Arg::Type(owner);
            } else {
                checked.free_values.get_mut(definition).unwrap().get_mut(&name).unwrap().ty = owner;
            }
            let owning = Ty::Closure(*definition, arguments.clone(), owning);
            assert!(
                !checked
                    .is_copy(module, &context, &owning, &env, &mut active, &mut b)
                    .unwrap(),
                "Fn spelling cannot copy an actual owner capture"
            );
            checked
                .free_values
                .get_mut(definition)
                .unwrap()
                .get_mut(&name)
                .unwrap()
                .ty = old;
            checked
                .prototypes
                .get_mut(definition)
                .unwrap()
                .callable_kind = Some(CallableKind::FnOnce);
            assert!(
                !checked
                    .is_copy(module, &context, &ty, &env, &mut active, &mut b)
                    .unwrap(),
                "a consuming interface stays affine even for scalar captures"
            );
            checked.creator_moves.insert(*definition, capture_mode);
        }
    }
}
