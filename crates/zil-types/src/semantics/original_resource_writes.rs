//! Write equations on original declaration parameters and closure receivers.
//! Actual descriptor values, owners and writes are reconstructed independently.
use super::*;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Root {
    Parameter(SemanticDefinitionId, u32),
    Receiver(SemanticDefinitionId),
    Alias(SemanticDefinitionId, NodeId),
}

type Roots = BTreeSet<Root>;

#[derive(Clone, Default)]
pub(super) struct Graph {
    writes: Roots,
    links: BTreeMap<Root, Roots>,
    receivers: BTreeSet<SemanticDefinitionId>,
    required: BTreeSet<SemanticDefinitionId>,
}

impl Graph {
    pub(super) fn seeded(required: BTreeSet<SemanticDefinitionId>) -> Self {
        Self {
            required,
            ..Self::default()
        }
    }

    pub(super) fn snapshot_work(&self) -> usize {
        let rows = self
            .writes
            .len()
            .saturating_add(self.receivers.len())
            .saturating_add(self.required.len());
        self.links.values().fold(rows, |work, links| {
            work.saturating_add(links.len()).saturating_add(1)
        })
    }
}

impl Check<'_, '_> {
    pub(super) fn capture_resource_root(
        &mut self,
        owner: SemanticDefinitionId,
        closure: SemanticDefinitionId,
        ty: &SemanticTypeRecipe,
        source: Option<Root>,
        moved: bool,
        depth: usize,
    ) -> R<Option<Root>> {
        let owned = match ty {
            SemanticTypeRecipe::Managed(_) => moved,
            SemanticTypeRecipe::Nominal { definition, .. } => matches!(
                self.definition(*definition)?.body,
                SemanticDefinitionBody::Nominal {
                    kind: LifeNominalKind::Package,
                    ..
                }
            ),
            SemanticTypeRecipe::Environment { .. } if source.is_some() && moved => {
                matches!(
                    self.observation_type(owner, ty, depth + 1)?,
                    SemanticTypeRecipe::Managed(_)
                )
            }
            _ => false,
        };
        Ok(if owned {
            Some(Root::Receiver(closure))
        } else {
            source
        })
    }

    pub(super) fn resource_place_roots(
        &mut self,
        owner: SemanticDefinitionId,
        expression: &SemanticOriginalExpr,
        variables: &BTreeMap<String, Binding>,
        depth: usize,
    ) -> R<Roots> {
        let mut pending = vec![(expression, depth)];
        let mut roots = Roots::new();
        while let Some((expression, depth)) = pending.pop() {
            self.pay(1, depth)?;
            match &expression.kind {
                CoreExprKind::Name(name) => {
                    self.pay(
                        name.len()
                            .saturating_add(variables.len().saturating_add(1).ilog2() as usize),
                        depth,
                    )?;
                    if let Some(root) = variables
                        .get(name)
                        .and_then(|binding| binding.resource_root)
                    {
                        self.pay(roots.len().saturating_add(1).ilog2() as usize + 1, depth)?;
                        roots.insert(root);
                    }
                }
                CoreExprKind::ReceiverMember(_) => {
                    let SemanticDefinitionBody::FunctionWithBody {
                        parameters,
                        receiver_parameter: Some(index),
                        ..
                    } = &self.definition(owner)?.body
                    else {
                        continue;
                    };
                    if let Some(binding) = parameters
                        .get(*index as usize)
                        .and_then(|parameter| variables.get(&parameter.name))
                        && let Some(root) = binding.resource_root
                    {
                        self.pay(roots.len().saturating_add(1).ilog2() as usize + 1, depth)?;
                        roots.insert(root);
                    }
                }
                CoreExprKind::Group(inner)
                | CoreExprKind::Ascribed { value: inner, .. }
                | CoreExprKind::Member { object: inner, .. }
                | CoreExprKind::Index { object: inner, .. }
                | CoreExprKind::Slice { object: inner, .. } => pending.push((inner, depth + 1)),
                CoreExprKind::If {
                    then_value,
                    else_value,
                    ..
                } => {
                    pending.push((else_value, depth + 1));
                    pending.push((then_value, depth + 1));
                }
                _ => {}
            }
        }
        Ok(roots)
    }

    pub(super) fn record_resource_write(
        &mut self,
        origins: Roots,
        through: Option<Root>,
        depth: usize,
    ) -> R<()> {
        for origin in origins {
            let graph = &self.proof.resource_writes;
            let work = match through {
                Some(target) if target != origin => {
                    graph.links.len().saturating_add(1).ilog2() as usize
                        + graph
                            .links
                            .get(&origin)
                            .map_or(0, |links| links.len().saturating_add(1).ilog2() as usize)
                        + 2
                }
                Some(_) => continue,
                None => graph.writes.len().saturating_add(1).ilog2() as usize + 1,
            };
            self.pay(work, depth)?;
            if let Root::Receiver(owner) = origin {
                self.pay(
                    self.proof
                        .resource_writes
                        .receivers
                        .len()
                        .saturating_add(1)
                        .ilog2() as usize
                        + 1,
                    depth,
                )?;
                self.proof
                    .proof_mut()?
                    .resource_writes
                    .receivers
                    .insert(owner);
            }
            let graph = &mut self.proof.proof_mut()?.resource_writes;
            if let Some(target) = through {
                graph.links.entry(origin).or_default().insert(target);
            } else {
                graph.writes.insert(origin);
            }
        }
        Ok(())
    }

    pub(super) fn description_call(
        &mut self,
        owner: SemanticDefinitionId,
        target: SemanticDefinitionId,
        operands: &[&SemanticOriginalExpr],
        signature: &SemanticSignatureRecipe,
        variables: &BTreeMap<String, Binding>,
        depth: usize,
    ) -> R<()> {
        for (ordinal, (operand, parameter)) in
            operands.iter().zip(&signature.parameters).enumerate()
        {
            if !matches!(parameter.ty, SemanticTypeRecipe::Desc { .. }) {
                continue;
            }
            let roots = self.resource_place_roots(owner, operand, variables, depth + 1)?;
            if roots.is_empty() {
                continue;
            }
            let body = &self.definition(target)?.body;
            let through = matches!(
                body,
                SemanticDefinitionBody::FunctionWithBody {
                    body: SemanticOriginalFunctionBody::Defined(_),
                    ..
                }
            )
            .then_some(Root::Parameter(
                target,
                u32::try_from(ordinal)
                    .map_err(|_| bad("original description parameter ordinal overflows"))?,
            ));
            self.record_resource_write(roots, through, depth + 1)?;
        }
        Ok(())
    }

    pub(super) fn intrinsic_resource_write(
        &mut self,
        owner: SemanticDefinitionId,
        site: SemanticSiteId,
        arguments: &[SemanticOriginalExpr],
        variables: &BTreeMap<String, Binding>,
        depth: usize,
    ) -> R<()> {
        self.pay(
            self.proof.intrinsics.len().saturating_add(1).ilog2() as usize + 1,
            depth,
        )?;
        let kind = self
            .proof
            .intrinsics
            .get(&(owner, site))
            .map(|call| &call.kind);
        if matches!(
            kind,
            Some(
                IntrinsicKind::BytesReserve
                    | IntrinsicKind::BytesTruncate
                    | IntrinsicKind::BytesPush
                    | IntrinsicKind::BytesBorrow { mutable: true }
                    | IntrinsicKind::StoreReserve
                    | IntrinsicKind::StorePush
                    | IntrinsicKind::StorePop
                    | IntrinsicKind::StoreBorrow { mutable: true }
            )
        ) {
            let operand = arguments
                .first()
                .ok_or_else(|| bad("original write intrinsic has no description"))?;
            let roots = self.resource_place_roots(owner, operand, variables, depth + 1)?;
            self.record_resource_write(roots, None, depth + 1)?;
        }
        Ok(())
    }

    pub(super) fn resource_receiver_written(
        &mut self,
        owner: SemanticDefinitionId,
        depth: usize,
    ) -> R<bool> {
        let required = &self.proof.resource_writes.required;
        if !required.is_empty() {
            self.pay(required.len().saturating_add(1).ilog2() as usize + 1, depth)?;
            if self.proof.resource_writes.required.contains(&owner) {
                return Ok(true);
            }
        }
        self.proved_resource_receiver_write(owner, depth)
    }

    fn proved_resource_receiver_write(
        &mut self,
        owner: SemanticDefinitionId,
        depth: usize,
    ) -> R<bool> {
        let graph = &self.proof.resource_writes;
        if graph.writes.is_empty() {
            return Ok(false);
        }
        let lookup = graph.links.len().saturating_add(1).ilog2() as usize
            + graph.writes.len().saturating_add(1).ilog2() as usize
            + 2;
        let mut pending = vec![Root::Receiver(owner)];
        let mut seen = Roots::new();
        while let Some(root) = pending.pop() {
            self.pay(
                lookup.saturating_add(seen.len().saturating_add(1).ilog2() as usize),
                depth,
            )?;
            if !seen.insert(root) {
                continue;
            }
            let graph = &self.proof.resource_writes;
            if graph.writes.contains(&root) {
                return Ok(true);
            }
            if let Some(links) = graph.links.get(&root) {
                self.pay(links.len(), depth)?;
                pending.extend(self.proof.resource_writes.links[&root].iter().copied());
            }
        }
        Ok(false)
    }

    // If a literal is checked inside a recursive descriptor function, its
    // call may precede writes in the still active target's body. Recheck the
    // original body only when the closed graph proves an additional write.
    // Every premise carried into that recheck must be proved again before
    // any externally visible result, so provisional rows grant no authority.
    pub(super) fn settle_resource_receivers(&mut self) -> R<BTreeSet<SemanticDefinitionId>> {
        let graph = &self.proof.resource_writes;
        self.pay(graph.receivers.len(), 0)?;
        let receivers = self
            .proof
            .resource_writes
            .receivers
            .iter()
            .copied()
            .collect::<Vec<_>>();
        let mut required = std::mem::take(&mut self.proof.proof_mut()?.resource_writes.required);
        let mut retry = false;
        let mut recovered = 0;
        for receiver in receivers {
            self.pay(required.len().saturating_add(1).ilog2() as usize + 1, 0)?;
            if !self.proved_resource_receiver_write(receiver, 0)? {
                continue;
            }
            if required.contains(&receiver) {
                recovered += 1;
            } else if self.proof.kinds.get(&receiver) == Some(&CallableKind::Fn) {
                required.insert(receiver);
                recovered += 1;
                retry = true;
            }
        }
        if recovered != required.len() {
            return Err(bad(
                "original recursive description writes changed during receiver rechecking",
            ));
        }
        Ok(if retry { required } else { BTreeSet::new() })
    }
}
