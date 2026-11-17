//! A projected view has a fresh occurrence and a separately proved upper
//! bound. Choosing one contextual role never creates a global LifeEqual.
use super::*;

impl Solver<'_, '_> {
    pub(super) fn receiving_for(
        &self,
        frame: &Frame,
        node: NodeId,
        b: &mut Budget,
    ) -> CompileResult<Option<Term>> {
        b.spend(1, WORK)?;
        Ok(self
            .receiving
            .last()
            .filter(|(owner, at, _)| *owner == frame.definition && *at == node)
            .map(|(_, _, term)| *term))
    }
    pub(super) fn remember_view_upper(
        &mut self,
        owner: SemanticDefinitionId,
        ordinal: u32,
        upper: Option<Term>,
        b: &mut Budget,
    ) -> CompileResult<()> {
        let Some(upper) = upper else {
            return Ok(());
        };
        b.lookup(self.state.view_uppers.len(), WORK)?;
        let edges = self.state.view_uppers.entry((owner, ordinal)).or_default();
        b.spend(edges.len().saturating_mul(2).saturating_add(1), WORK)?;
        if !edges.contains(&upper) {
            edges.push(upper);
        }
        Ok(())
    }
    pub(super) fn select_view_life(
        &mut self,
        frame: &Frame,
        occurrence: Term,
        upper: Option<Term>,
        wanted: Option<Term>,
        local: Option<LocalAnchor>,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Term> {
        let Some(wanted) = wanted else {
            return Ok(occurrence);
        };
        let wanted = self.root(wanted, b)?;
        if matches!(
            self.state.arena[wanted],
            Kind::LifeVariable(None) | Kind::Infer(None)
        ) {
            return Ok(occurrence);
        }
        if self.same_original(occurrence, wanted, b, depth + 1)? {
            return Ok(wanted);
        }
        // A local's view may adopt a contextual region in this same
        // activation. This is directed region selection, not LifeEqual and
        // not a source of a formal/ancestor lifetime or of alias permission.
        if upper.is_none() && matches!(local, Some(LocalAnchor::Binding(_)))
            && matches!(self.state.arena[wanted], Kind::Life(Life::Implicit(owner, _)) if owner == frame.definition)
            && matches!(self.state.arena[self.root(occurrence, b)?], Kind::Life(Life::Implicit(owner, _)) if owner == frame.definition)
        {
            return Ok(wanted);
        }
        let upper = upper.ok_or_else(|| {
            fail("original contextual borrow has no genuine projected input upper bound")
        })?;
        if !self.view_upper_allows(frame, upper, wanted, &mut BTreeSet::new(), b, depth + 1)? {
            return Err(fail(
                "original contextual borrow exceeds its genuine source lifetime upper bound",
            ));
        }
        Ok(wanted)
    }
    pub(super) fn view_upper_allows(
        &mut self,
        frame: &Frame,
        upper: Term,
        wanted: Term,
        active: &mut BTreeSet<(SemanticDefinitionId, u32)>,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<bool> {
        self.step(depth, b)?;
        let upper = self.root(upper, b)?;
        if self.same_original(upper, wanted, b, depth + 1)? {
            return Ok(true);
        }
        if let Kind::Life(Life::Receiver(owner, node)) = self.state.arena[upper] {
            b.lookup(self.state.abstract_calls.len().saturating_add(self.state.calls.len()), WORK)?;
            let receiver = if let Some(call) = self.state.abstract_calls.get(&(owner, node)) {
                if roles::receiver_result(&call.signature, call.kind) {
                    Some(call.place.as_ref().filter(|place| place.indirect).map_or(call.receiver, |place| place.root))
                } else { None }
            } else if let Some(source) = self.state.calls.get(&(owner, node)).and_then(|call| call.receiver.as_ref()) {
                Some(if source.indirect { source.root } else { source.inner })
            } else { None };
            if let Some(receiver) = receiver {
                let root = self.type_root(receiver, b)?;
                if let Kind::Address { life, .. } = self.state.arena[root] {
                    // A borrowed F's result is a subview of its real incoming
                    // head. An owned local F has no such external upper and
                    // therefore cannot escape merely by sharing its Type.
                    return self.view_upper_allows(frame, life, wanted, active, b, depth + 1);
                }
            }
        }
        if let Kind::Life(Life::Implicit(owner, ordinal)) = self.state.arena[upper] {
            b.lookup(self.state.call_regions.len(), WORK)?;
            if let Some(region) = self.state.call_regions.get(&(owner, ordinal)) {
                b.spend(region.sources.len(), WORK)?;
                let sources = region.sources.clone();
                if sources.is_empty() { return Err(fail("original call region has no complete incoming sources")); }
                b.lookup(active.len(), WORK)?;
                if !active.insert((owner, ordinal)) { return Err(fail("original call region has a cyclic source bound")); }
                let mut allowed = true;
                for source in sources {
                    if let Some(life) = source.life {
                        allowed &= self.view_upper_allows(frame, life, wanted, active, b, depth + 1)?;
                    }
                }
                active.remove(&(owner, ordinal));
                return Ok(allowed);
            }
        }
        if let Kind::Life(Life::Implicit(owner, ordinal)) = self.state.arena[upper] {
            b.lookup(self.state.view_uppers.len(), WORK)?;
            if let Some(edges) = self.state.view_uppers.get(&(owner, ordinal)) {
                let mut scope = Some(frame.definition);
                let mut found = false;
                for _ in 0..=b.partial_depth {
                    let Some(current) = scope else {
                        break;
                    };
                    if current == owner {
                        found = true;
                        break;
                    }
                    scope = self.inventory.definition(current, b)?.parent;
                }
                if !found {
                    return Err(fail(
                        "original view upper leaves its genuine lexical source scope",
                    ));
                }
                b.lookup(active.len(), WORK)?;
                if !active.insert((owner, ordinal)) {
                    return Err(fail("original view upper dependency is cyclic"));
                }
                b.spend(edges.len(), WORK)?;
                let edges = edges.clone();
                let mut allowed = true;
                for parent in edges {
                    allowed &=
                        self.view_upper_allows(frame, parent, wanted, active, b, depth + 1)?;
                }
                b.lookup(active.len(), WORK)?;
                active.remove(&(owner, ordinal));
                return Ok(allowed);
            }
        }
        self.declared_outlives(frame, upper, wanted, b, depth + 1)
    }
    fn declared_outlives(
        &mut self,
        frame: &Frame,
        source: Term,
        target: Term,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<bool> {
        if self.same_original(source, target, b, depth + 1)? {
            return Ok(true);
        }
        let mut relations = Vec::new();
        let mut owner = Some(frame.definition);
        for _ in depth..=b.partial_depth {
            let Some(current) = owner else {
                break;
            };
            let definition = self.inventory.definition(current, b)?;
            let context = self.premise_frame(frame, current, b)?;
            for requirement in
                definition
                    .requirements
                    .iter()
                    .chain(
                        definition
                            .parameters
                            .iter()
                            .flat_map(|formal| match formal {
                                SemanticFormal::Type { requirements } => requirements.as_slice(),
                                _ => &[],
                            }),
                    )
            {
                let (left, right, symmetric) = match requirement {
                    SemanticRequirementRecipe::Outlives { longer, shorter } => {
                        (longer, shorter, false)
                    }
                    SemanticRequirementRecipe::LifeEqual { left, right } => (left, right, true),
                    _ => {
                        b.spend(1, WORK)?;
                        continue;
                    }
                };
                let left = self.life(&context, left, b, depth + 1)?;
                let right = self.life(&context, right, b, depth + 1)?;
                Self::push(&mut relations, (left, right), b)?;
                if symmetric {
                    Self::push(&mut relations, (right, left), b)?;
                }
            }
            owner = definition.parent;
        }
        if owner.is_some() {
            return Err(resource(
                "original contextual lifetime premise ancestry exceeds its finite bound",
            ));
        }
        let mut pending = vec![source];
        let mut visited = BTreeSet::new();
        while let Some(current) = pending.pop() {
            let current = self.root(current, b)?;
            b.lookup(visited.len(), WORK)?;
            if !visited.insert(current) {
                continue;
            }
            if self.same_original(current, target, b, depth + 1)? {
                return Ok(true);
            }
            for (longer, shorter) in &relations {
                b.spend(1, WORK)?;
                if self.same_original(current, *longer, b, depth + 1)? {
                    Self::push(&mut pending, *shorter, b)?;
                }
            }
        }
        Ok(false)
    }
    pub(super) fn description_upper(
        &self,
        place: &places::Place,
        selected: Term,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Option<Term>> {
        self.step(depth, b)?;
        let Kind::Address { life, inner, .. } = self.state.arena[self.type_root(place.root, b)?]
        else {
            return Ok(None);
        };
        let mut current = self.type_root(inner, b)?;
        for ordinal in &place.path {
            self.step(depth + place.path.len(), b)?;
            current = match &self.state.arena[current] {
                Kind::Nominal { fields, .. } | Kind::Tuple(fields) => {
                    *fields.get(*ordinal as usize).ok_or_else(|| {
                        fail("original description exceeds its exact source field path")
                    })?
                }
                _ => {
                    return Err(unknown(
                        "original description upper needs its exact nested owner boundary",
                    ));
                }
            };
            current = self.type_root(current, b)?;
        }
        if !self.same_original(current, selected, b, depth + 1)? {
            return Err(fail(
                "original description upper changes its actual selected source",
            ));
        }
        Ok(Some(life))
    }
}
