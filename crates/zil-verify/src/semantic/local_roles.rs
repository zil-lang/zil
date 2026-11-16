//! Directed shared-local transport after every original branch and actual
//! world has passed. These rows carry equations, never roots or permissions.
use super::*;
use std::collections::BTreeSet;

pub(super) struct Writes {
    ordinal: u32,
    sites: BTreeSet<(BlockId, u32, ValueId)>,
    calls: BTreeSet<(BlockId,u32,u32)>,
}
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct Transfer {
    pub(crate) local: LocalId,
    pub(crate) outputs: Vec<SignatureLifeClassBinding>,
    pub(crate) stores: Vec<(BlockId, u32, ValueId)>,
    pub(crate) calls: Vec<(BlockId,u32,u32)>,
}

// Every dependency must have an original input/static/role identity. A
// role-only cycle cannot manufacture an export, while an anchored loop keeps
// all its dependencies without equating any independent input classes.
fn grounded_roles(
    sources: &[Vec<Option<usize>>],
    unknown: &[bool],
    b: &mut Budget,
) -> Check<Vec<bool>> {
    b.spend(sources.len().saturating_mul(2))?;
    let mut invalid = unknown.to_vec();
    let mut anchored = vec![false; sources.len()];
    let mut changed = true;
    while changed {
        changed = false;
        for (index, rows) in sources.iter().enumerate() {
            b.spend(rows.len().saturating_add(1))?;
            for row in rows {
                match row {
                    None => {
                        if !anchored[index] {
                            anchored[index] = true;
                            changed = true;
                        }
                    }
                    Some(source) => {
                        if invalid[*source] && !invalid[index] {
                            invalid[index] = true;
                            changed = true;
                        }
                        if anchored[*source] && !anchored[index] {
                            anchored[index] = true;
                            changed = true;
                        }
                    }
                }
            }
        }
    }
    b.spend(sources.len())?;
    Ok(anchored
        .into_iter()
        .zip(invalid)
        .map(|(anchor, invalid)| anchor && !invalid)
        .collect())
}

impl CheckedContext {
    pub(super) fn record_local_role_write(
        &self,
        instance: &SemanticInstance,
        local: LocalId,
        value: ValueId,
        normalized: &CheckedType,
        environment: &EvalEnv,
        b: &mut Budget,
    ) -> Check<()> {
        let frame = self.frame(environment, SemanticScopeRef::Ancestor(0), b)?;
        let Some(at) = environment.at else {
            return Err(bad("shared local transport has no actual Store occurrence"));
        };
        if !matches!(instance.target,SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(function))if function==at.function)
        {
            return Err(bad(
                "shared local transport selects another actual function",
            ));
        }
        let mut rows = normalized
            .life
            .slots
            .iter()
            .filter(|row| row.path.is_empty());
        let row = rows
            .next()
            .ok_or_else(|| bad("shared local transport has no complete outer role"))?;
        if rows.next().is_some() || row.mode != StoredBorrowMode::SharedAddress {
            return Err(bad(
                "shared local transport has another complete address mode",
            ));
        }
        let Some(ClassKey::Implicit { scope, ordinal }) =
            normalized.classes.get(row.binder as usize)
        else {
            return Err(bad(
                "shared local transport loses its original lexical role",
            ));
        };
        if *scope != frame.key || frame.definition != instance.definition {
            return Err(bad("shared local transport selects another original scope"));
        }
        let mut writes = self.local_role_writes.borrow_mut();
        b.spend(writes.len().checked_ilog2().map_or(1, |n| n as usize + 2))?;
        if !writes.contains_key(&(instance.id, local)) && writes.len() >= b.limits.max_nodes {
            return Err(limit(
                "shared local transport directory exceeds selected bound",
            ));
        }
        let row = writes
            .entry((instance.id, local))
            .or_insert_with(|| Writes {
                ordinal: *ordinal,
                sites: BTreeSet::new(),
                calls: BTreeSet::new(),
            });
        if row.ordinal != *ordinal {
            return Err(bad(
                "one actual local transports different original lexical roles",
            ));
        }
        b.spend(
            row.sites
                .len()
                .checked_ilog2()
                .map_or(1, |n| n as usize + 2),
        )?;
        if row.sites.len() >= b.limits.max_nodes
            && !row.sites.contains(&(at.block, at.instruction, value))
        {
            return Err(limit("shared local Store directory exceeds selected bound"));
        }
        row.sites.insert((at.block, at.instruction, value));
        Ok(())
    }

    /// The whole original/actual call has proved this root view. Keep the
    /// exact opcode and parameter with the already checked Store transition;
    /// neither row is an equality of the local window and its source.
    pub(super) fn record_local_role_call(&self,module:&TypedModule,instance:&SemanticInstance,parameter:u32,argument:ValueId,expected:&CheckedType,environment:&EvalEnv,b:&mut Budget)->Check<()>{
        let frame=self.frame(environment,SemanticScopeRef::Ancestor(0),b)?;
        let at=environment.at.ok_or_else(||bad("shared local call view has no actual operation"))?;
        if !matches!(instance.target,SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(function))if function==at.function)||frame.definition!=instance.definition{return Err(bad("shared local call view selects another actual/original owner"));}
        let operation=module.functions.get(at.function.0 as usize).and_then(|f|f.blocks.get(at.block.0 as usize)).and_then(|block|block.instructions.get(at.instruction as usize)).ok_or_else(||bad("shared local call operation is absent"))?;
        if !matches!(&operation.kind,InstructionKind::Call{arguments,..}if arguments.get(parameter as usize)==Some(&argument)){return Err(bad("shared local call view changes its exact opcode or argument"));}
        b.spend(expected.life.slots.len())?;let mut slots=expected.life.slots.iter().filter(|row|row.path.is_empty());
        let slot=slots.next().ok_or_else(||bad("shared local call view has no root occurrence"))?;
        if slots.next().is_some(){return Err(bad("shared local call view repeats its root"));}
        let Some(ClassKey::Implicit{scope,ordinal})=expected.classes.get(slot.binder as usize)else{return Err(bad("shared local call view has no original local role"));};
        if *scope!=frame.key{return Err(bad("shared local call view belongs to another frame"));}
        let mut writes=self.local_role_writes.borrow_mut();b.spend(writes.len())?;
        let mut selected=writes.iter_mut().filter(|((owner,_),write)|*owner==instance.id&&write.ordinal==*ordinal);
        let(_,write)=selected.next().ok_or_else(||bad("shared local call view lacks its real Store transition"))?;
        if selected.next().is_some(){return Err(bad("shared local call view selects more than one real local"));}
        b.spend(write.calls.len().checked_ilog2().map_or(1,|n|n as usize+2))?;
        if write.calls.len()>=b.limits.max_nodes&&!write.calls.contains(&(at.block,at.instruction,parameter)){return Err(limit("shared local call view directory exceeded"));}
        write.calls.insert((at.block,at.instruction,parameter));Ok(())
    }

    pub(super) fn publish_local_role_transfers(
        &self,
        module: &TypedModule,
        worlds: &[(SemanticInstanceId, EvalEnv)],
        b: &mut Budget,
    ) -> Check<()> {
        b.spend(1)?;
        if self.local_role_writes.borrow().is_empty() {
            return Ok(());
        }
        let mut ready = BTreeMap::<FunctionId, Vec<Transfer>>::new();
        for (id, environment) in worlds {
            let instance = self.instance(module, *id, b)?;
            let SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(function)) =
                instance.target
            else {
                continue;
            };
            let recipes = self
                .original
                .borrow()
                .as_ref()
                .ok_or_else(|| bad("shared local export has no complete original body"))?
                .local_role_recipes(module, instance.definition, environment, b)?;
            if recipes.is_empty() {
                continue;
            }
            let f = module
                .functions
                .get(function.0 as usize)
                .ok_or_else(|| bad("shared local export actual function is absent"))?;
            let frame = self.frame(environment, SemanticScopeRef::Ancestor(0), b)?;
            let header = f
                .storage_life
                .as_ref()
                .ok_or_else(|| bad("shared local export requires its complete actual header"))?;
            let mut input_binders = BTreeSet::new();
            let input = |base: StorageBase| {
                matches!(
                    base,
                    StorageBase::Parameter(_) | StorageBase::Capture(_) | StorageBase::Receiver
                )
            };
            b.spend(header.bindings.len().saturating_add(header.classes.len()))?;
            for row in &header.bindings {
                if input(row.position.base) {
                    input_binders.insert(row.binder);
                }
            }
            for row in &header.classes {
                if input(row.position.base) {
                    input_binders.insert(row.binder);
                }
            }
            let mut inputs = Vec::new();
            for row in &instance.header_embedding {
                b.spend(1)?;
                if input_binders.contains(&row.header) {
                    inputs.push(self.class(module, &row.actual, environment, b)?);
                }
            }
            let mut sources = Vec::new();
            let mut unknown = Vec::new();
            for role in &recipes {
                let mut deps = Vec::new();
                let mut invalid = false;
                for source in &role.sources {
                    b.spend(1)?;
                    let SemanticOuterLifeRecipe::Class(source) = source else {
                        deps.push(None);
                        continue;
                    };
                    let source = self.life(module, source, environment, b)?;
                    let mut from_input = false;
                    for input in &inputs {
                        if self.same(environment, &source, input, b)? {
                            from_input = true;
                            break;
                        }
                    }
                    if from_input {
                        deps.push(None);
                        continue;
                    }
                    let mut found = None;
                    for (index, other) in recipes.iter().enumerate() {
                        b.spend(1)?;
                        if source
                            == (ClassKey::Implicit {
                                scope: frame.key,
                                ordinal: other.ordinal,
                            })
                        {
                            found = Some(index);
                            break;
                        }
                    }
                    if let Some(source) = found {
                        deps.push(Some(source));
                    } else {
                        invalid = true;
                    }
                }
                sources.push(deps);
                unknown.push(invalid);
            }
            let grounded = grounded_roles(&sources, &unknown, b)?;
            let writes = self.local_role_writes.borrow();
            let mut transfers = Vec::new();
            for ((owner, local), write) in writes.iter() {
                b.spend(1)?;
                if *owner != instance.id {
                    continue;
                }
                b.spend(recipes.len())?;
                let index = recipes
                    .iter()
                    .position(|role| role.ordinal == write.ordinal)
                    .ok_or_else(|| {
                        bad("actual shared Store has no exact completed original role")
                    })?;
                let ty = &f
                    .locals
                    .get(local.0 as usize)
                    .ok_or_else(|| bad("shared local export target is absent"))?
                    .ty;
                if !matches!(ty, Type::Address { mutable: false, .. }) {
                    return Err(bad(
                        "shared local export changes its actual address category",
                    ));
                }
                let mut actual_stores = BTreeSet::new();
                for block in &f.blocks {
                    for (ordinal, instruction) in block.instructions.iter().enumerate() {
                        b.spend(1)?;
                        if let InstructionKind::Store {
                            local: target,
                            value,
                        } = instruction.kind
                        {
                            if target == *local {
                                b.spend(
                                    actual_stores
                                        .len()
                                        .checked_ilog2()
                                        .map_or(1, |n| n as usize + 2),
                                )?;
                                actual_stores.insert((block.id, ordinal as u32, value));
                            }
                        }
                    }
                }
                b.spend(actual_stores.len().saturating_add(write.sites.len()))?;
                if actual_stores != write.sites {
                    return Err(bad(
                        "shared local export lacks a complete actual Store transition",
                    ));
                }
                let mut outputs = Vec::new();
                if grounded[index] {
                    let target = ClassKey::Implicit {
                        scope: frame.key,
                        ordinal: write.ordinal,
                    };
                    for row in &header.classes {
                        b.spend(1)?;
                        if row.position.base != StorageBase::Result {
                            continue;
                        }
                        b.spend(instance.header_embedding.len())?;
                        let image = instance
                            .header_embedding
                            .iter()
                            .find(|image| image.header == row.binder)
                            .ok_or_else(|| {
                                bad("shared local export loses its exact output embedding")
                            })?;
                        if self.same(
                            environment,
                            &target,
                            &self.class(module, &image.actual, environment, b)?,
                            b,
                        )? {
                            b.spend(row.position.path.len().saturating_add(1))?;
                            outputs.push(row.clone());
                        }
                    }
                }
                b.spend(actual_stores.len().saturating_add(write.calls.len()).saturating_add(1))?;
                transfers.push(Transfer {
                    local: *local,
                    outputs,
                    stores: actual_stores.into_iter().collect(),
                    calls: write.calls.iter().copied().collect(),
                });
            }
            if let Some(previous) = ready.get(&function) {
                charge_transfers(previous, b)?;
                charge_transfers(&transfers, b)?;
                if *previous != transfers {
                    return Err(bad(
                        "shared local export differs between complete original worlds",
                    ));
                }
            } else {
                ready.insert(function, transfers);
            }
        }
        *self.local_role_transfers.borrow_mut() = ready;
        Ok(())
    }

    pub(crate) fn checked_local_role_transfers(
        &self,
        module: &TypedModule,
        f: &TypedFunction,
        b: &mut Budget,
    ) -> Check<Vec<Transfer>> {
        if !module
            .functions
            .get(f.id.0 as usize)
            .is_some_and(|actual| std::ptr::eq(actual, f))
        {
            return Err(bad(
                "shared local transport selects another immutable actual function",
            ));
        }
        let transfers = self.local_role_transfers.borrow();
        b.spend(
            transfers
                .len()
                .checked_ilog2()
                .map_or(1, |n| n as usize + 2),
        )?;
        let Some(rows) = transfers.get(&f.id) else {
            return Ok(Vec::new());
        };
        charge_transfers(rows, b)?;
        Ok(rows.clone())
    }
}
fn charge_transfers(rows: &[Transfer], b: &mut Budget) -> Check<()> {
    b.spend(rows.len())?;
    for row in rows {
        b.spend(row.stores.len().saturating_add(row.outputs.len()).saturating_add(row.calls.len()))?;
        for output in &row.outputs {
            b.spend(output.position.path.len())?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn complete_role_dependencies_require_an_anchor_and_reject_unknown_sources() {
        let mut b = Budget::new(&Limits::default());
        assert_eq!(
            grounded_roles(
                &[vec![None, Some(1)], vec![Some(0)]],
                &[false, false],
                &mut b
            )
            .unwrap(),
            vec![true, true]
        );
        assert_eq!(
            grounded_roles(&[vec![Some(1)], vec![Some(0)]], &[false, false], &mut b).unwrap(),
            vec![false, false]
        );
        assert_eq!(
            grounded_roles(
                &[vec![None, Some(1)], vec![Some(0)]],
                &[false, true],
                &mut b
            )
            .unwrap(),
            vec![false, false]
        );
    }
}
