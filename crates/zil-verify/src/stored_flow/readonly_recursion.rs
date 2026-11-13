//! Conditional scheduling of genuinely read-only recursive helpers. The graph
//! is reconstructed from the actual reachable calls; no public flag, header
//! upper, or missing after-row establishes a completed memory proof.
use super::*;

pub(super) struct Inventory {
    components: Vec<Vec<usize>>,
    component: Vec<Option<usize>>,
    dependencies: Vec<Vec<usize>>,
}
fn pointee(ty: &Type) -> bool {
    stored_callables::scalar(ty)
        || matches!(ty, Type::Slice { element } if stored_callables::scalar(element))
}
fn input(ty: &Type) -> bool {
    stored_callables::scalar(ty)
        || matches!(ty, Type::Address { mutable: false, inner } if pointee(inner))
}
fn eligible(function: &TypedFunction, b: &mut Budget) -> Check<bool> {
    b.spend(function.parameters.len().saturating_add(1))?;
    Ok(function.foreign.is_none()
        && function.environment.is_none()
        && function.dyn_instance.is_none()
        && function.call_kind == CallKind::Direct
        && function.result_mode == PassingMode::Infer
        && function.callable_contract.is_none()
        && function.callable_return_contract.is_none()
        && function.protocol_contracts.is_empty()
        && matches!(&function.return_type, Type::Address { mutable: false, inner } if pointee(inner))
        && function.parameters.iter().all(|id| {
            function
                .locals
                .get(id.0 as usize)
                .is_some_and(|local| input(&local.ty))
        }))
}

impl Inventory {
    pub(super) fn build(
        module: &TypedModule,
        graphs: &[FunctionGraph],
        b: &mut Budget,
    ) -> Check<Self> {
        let count = module.functions.len();
        b.spend(amount(count, 8)?)?;
        let mut allowed = Vec::with_capacity(count);
        for function in &module.functions {
            allowed.push(eligible(function, b)?);
        }
        let mut dependencies = vec![Vec::new(); count];
        let mut reverse = vec![Vec::new(); count];
        for (index, function) in module.functions.iter().enumerate() {
            if !allowed[index] {
                continue;
            }
            b.spend(function.blocks.len())?;
            for (block, reachable) in function.blocks.iter().zip(&graphs[index].reachable) {
                if !reachable {
                    continue;
                }
                b.spend(block.instructions.len())?;
                for instruction in &block.instructions {
                    let target = match instruction.kind {
                        InstructionKind::Call { function, .. } => function.0 as usize,
                        _ => {
                            let Some((definition, _)) =
                                stored_callables::called(module, function, &instruction.kind)
                            else {
                                continue;
                            };
                            if static_borrows::stateless_result(module, definition, b)? {
                                continue;
                            }
                            definition.function.0 as usize
                        }
                    };
                    if target >= count {
                        return Err(err("recursive helper target is absent"));
                    }
                    b.spend(dependencies[index].len().saturating_add(1))?;
                    dependencies[index].push(target);
                    if allowed[target] {
                        b.spend(reverse[target].len().saturating_add(1))?;
                        reverse[target].push(index);
                    }
                }
            }
        }
        // Iterative Kosaraju: no host recursion and no inferred relation from
        // two equal class numbers. Only actual function/call identities form edges.
        let mut visited = vec![false; count];
        let mut order = Vec::with_capacity(count);
        let mut stack = Vec::with_capacity(count);
        for start in 0..count {
            b.spend(1)?;
            if !allowed[start] || visited[start] {
                continue;
            }
            visited[start] = true;
            stack.push((start, 0usize));
            while let Some((node, cursor)) = stack.last_mut() {
                b.spend(1)?;
                if let Some(target) = dependencies[*node].get(*cursor).copied() {
                    *cursor += 1;
                    if allowed[target] && !visited[target] {
                        visited[target] = true;
                        stack.push((target, 0));
                    }
                } else {
                    order.push(*node);
                    stack.pop();
                }
            }
        }
        let mut component = vec![None; count];
        let mut components = Vec::new();
        let mut pending = Vec::with_capacity(count);
        while let Some(start) = order.pop() {
            b.spend(1)?;
            if component[start].is_some() {
                continue;
            }
            let id = components.len();
            let mut members = Vec::new();
            pending.push(start);
            component[start] = Some(id);
            while let Some(node) = pending.pop() {
                b.spend(
                    members
                        .len()
                        .saturating_add(reverse[node].len())
                        .saturating_add(1),
                )?;
                members.push(node);
                for source in &reverse[node] {
                    if component[*source].is_none() {
                        component[*source] = Some(id);
                        pending.push(*source);
                    }
                }
            }
            b.spend(components.len().saturating_add(1))?;
            components.push(members);
        }
        Ok(Self {
            components,
            component,
            dependencies,
        })
    }

    pub(super) fn eligible(&self, function: usize) -> bool {
        self.component[function].is_some()
    }

    pub(super) fn internal(&self, caller: usize, callee: usize, assumed: &[bool]) -> bool {
        assumed[caller]
            && self.component[caller].is_some()
            && self.component[caller] == self.component[callee]
    }

    pub(super) fn lower_roots(
        function: &TypedFunction,
        solution: &Solution,
        b: &mut Budget,
    ) -> Check<bool> {
        b.spend(function.values.len())?;
        for (ty, value) in function.values.iter().zip(&solution.values) {
            if matches!(ty, Type::Address { .. }) {
                source_work(&value.address.sources, b)?;
                if !value
                    .address
                    .sources
                    .iter()
                    .any(|source| source.static_dependency != StaticProvenance::UpperOnly)
                {
                    return Ok(false);
                }
            }
        }
        Ok(true)
    }

    /// These are assumptions for the *next* complete solve, never completed
    /// summaries. That solve replays every actual access, effect, parent and
    /// epoch check; it may revoke the assumptions when lower facts change.
    pub(super) fn assumptions(
        &self,
        local: &[bool],
        summaries: &[Summary],
        b: &mut Budget,
    ) -> Check<Vec<bool>> {
        b.spend(self.component.len())?;
        let mut result = vec![false; self.component.len()];
        b.spend(self.components.len())?;
        for (id, members) in self.components.iter().enumerate() {
            let mut valid = true;
            let mut recursive = members.len() > 1;
            b.spend(members.len())?;
            for member in members {
                valid &= local[*member];
                b.spend(self.dependencies[*member].len())?;
                for target in &self.dependencies[*member] {
                    if self.component[*target] == Some(id) {
                        recursive |= target == member;
                    } else {
                        valid &= summaries[*target].complete;
                    }
                }
            }
            if valid && recursive {
                for member in members {
                    result[*member] = true;
                }
            }
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn recursive_scheduling_does_not_admit_mutable_opaque_or_captured_inputs() {
        let limits = Limits::default();
        let (module, _) = zil_ir::wire::decode_module_candidate(
            include_bytes!(
                "../../tests/fixtures/dyn-context/static-callback-getter-cycle.typed.wire"
            ),
            &limits,
        )
        .unwrap();
        let function = module
            .functions
            .iter()
            .find(|function| function.name == "feedback")
            .unwrap();
        let mut budget = Budget::new(&limits);
        assert!(eligible(function, &mut budget).unwrap());
        for mutation in 0..3 {
            let mut forged = function.clone();
            match mutation {
                0 => {
                    let Type::Address { mutable, .. } = &mut forged.locals[0].ty else {
                        panic!()
                    };
                    *mutable = true;
                }
                1 => forged.locals[0].ty = Type::Closure(ClosureId(0)),
                2 => forged.environment = Some(ClosureId(0)),
                _ => unreachable!(),
            }
            assert!(!eligible(&forged, &mut budget).unwrap());
        }
    }
}
