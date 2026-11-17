//! Complete operation transport uses the original call's geometry and the
//! actual entry's entire P/C/R directory. It never classifies an output as a
//! special byte, Task or nominal case to decide whether its domain is kept.
use super::*;
use crate::semantic::value::{Argument,Class,Value};

/// A selected trait provider/default supplies this contextual mixed map only
/// after the ordinary trait boundary, provider identity and every rigid scope
/// have been proved. It contains no permission or additional class equation.
pub(super) struct MixedCall<'a> {
    pub(super) target: SemanticDefinitionId,
    pub(super) arguments: &'a [Argument],
    pub(super) parents: BTreeMap<SemanticInstanceId, Vec<Argument>>,
}

impl Check<'_, '_> {
    fn install_operation_class(
        &self, rows:&mut BTreeMap<StorageBinder,Class>, binder:StorageBinder,
        class:Class,b:&mut Budget,
    )->CompileResult<()> {
        let class=self.capture_class_image(class,b)?;
        b.lookup(rows.len(),WORK)?;
        if let Some(previous)=rows.get(&binder) {
            if previous!=&class {return Err(fail("operation header has conflicting complete original class sources"));}
        } else {rows.insert(binder,class);}
        Ok(())
    }

    fn operation_outer_class(
        &self, value:&Value,path:&[ContractProjection],b:&mut Budget,
    )->CompileResult<Option<u32>> {
        let mut selected=None;
        for slot in &value.life.slots {
            b.spend(slot.path.len().saturating_add(path.len()).saturating_add(1),WORK)?;
            if path.len()==slot.path.len() && path.iter().zip(&slot.path)
                .all(|(actual,expected)|matches!(actual,ContractProjection::Place(step) if step==expected)) {
                if selected.replace(slot.binder).is_some() {return Err(fail("operation value repeats a physical outer occurrence"));}
            }
        }
        for (port,class) in self.task_ports(value,b)? {
            b.spend(port.len().saturating_add(path.len()).saturating_add(1),WORK)?;
            if port==path && selected.replace(class).is_some() {
                return Err(fail("operation value repeats a retained outer occurrence"));
            }
        }
        // Physical projections of an Address describe its pointee's storage.
        // Lift the selected child occurrence through its exact retained row;
        // equality of class images never chooses a parent occurrence.
        if selected.is_none() && !path.is_empty()
            && path.iter().all(|step| matches!(step, ContractProjection::Place(_)))
            && matches!(value.ty, Type::Address { .. }) {
            let child = self.pointee(value, b)?;
            let mut child_class = None;
            for slot in &child.life.slots {
                b.spend(path.len().saturating_add(slot.path.len()).saturating_add(1), WORK)?;
                if path.len() == slot.path.len() && path.iter().zip(&slot.path)
                    .all(|(actual, expected)| matches!(actual, ContractProjection::Place(step) if step == expected)) {
                    if child_class.replace(slot.binder).is_some() {
                        return Err(fail("operation pointee repeats a physical occurrence"));
                    }
                }
            }
            if let Some(class) = child_class {
                for row in &value.life.retained {
                    b.spend(row.path.len().saturating_add(1), WORK)?;
                    if row.path.as_slice() == [LifeTypeProjection::Pointee] && row.class == class
                        && selected.replace(row.binder).is_some() {
                        return Err(fail("operation pointee repeats its parent transport"));
                    }
                }
            }
        }
        Ok(selected)
    }

    fn operation_child_class(
        &self,value:&Value,path:&[LifeTypeProjection],class:u32,b:&mut Budget,
    )->CompileResult<Option<u32>> {
        if path.is_empty() {
            return Ok((class<value.life.binder_count).then_some(class));
        }
        let mut selected=None;
        for row in self.signature_retained_classes(value,b)? {
            b.spend(row.path.len().saturating_add(path.len()).saturating_add(1),WORK)?;
            if row.path==path && row.class==class && selected.replace(row.binder).is_some() {
                return Err(fail("operation value repeats a complete intrinsic projection"));
            }
        }
        Ok(selected)
    }

    pub(super) fn operation_mixed_arguments(&self, scope: &SemanticInstance,
        expected: &[Argument], b: &mut Budget) -> CompileResult<()> {
        let definition = self.inventory.definition(scope.definition, b)?;
        b.spend(expected.len().saturating_add(scope.actuals.len()).saturating_add(1), WORK)?;
        if definition.parameters.len() != expected.len() || scope.actuals.len() != expected.len() {
            return Err(fail("operation context changes its complete mixed formal arity"));
        }
        for (ordinal, ((formal, expected), actual)) in definition.parameters.iter()
            .zip(expected).zip(&scope.actuals).enumerate() {
            let actual = self.actual_value(scope, actual, b, 0)?;
            if !self.body_call_actual(scope, ordinal, formal, expected, &actual, b)? {
                return Err(fail("operation context changes its full rigid mixed ordinal or invariant domain"));
            }
        }
        Ok(())
    }

    pub(super) fn operation_mixed_class(&self, class: &Class,
        owner: SemanticInstanceId, arguments: &[Argument], b: &mut Budget)
        -> CompileResult<Option<Class>> {
        b.spend(1, WORK)?;
        Ok(match class {
            Class::Formal(scope, index) if *scope == owner => {
                let Some(Argument::Life(class)) = arguments.get(*index as usize) else {
                    return Err(fail("operation Fixed life has no full original mixed ordinal"));
                };
                Some(class.clone())
            }
            Class::TypeArgument(scope, index, child) if *scope == owner => {
                let Some(Argument::Type(value)) = arguments.get(*index as usize) else {
                    return Err(fail("operation Fixed Type has no full original mixed ordinal"));
                };
                Some(value.classes.get(*child as usize).cloned()
                    .ok_or_else(|| fail("operation Fixed selector leaves its complete Type argument"))?)
            }
            _ => None,
        })
    }

    fn operation_fixed_sources(&self, instance: &SemanticInstance,
        at: (BlockId, usize), callee: &TypedFunction,
        contract: &StorageLifeContract, rows: &mut BTreeMap<StorageBinder, Class>,
        mixed: Option<&MixedCall<'_>>, b: &mut Budget) -> CompileResult<()> {
        if contract.fixed_binder_count == 0 { return Ok(()); }
        let ordinary;
        let (target, arguments) = if let Some(mixed) = mixed {
            (mixed.target, mixed.arguments)
        } else {
            let mut receipt = None;
            for site in self.inventory.sites_at(self.program, instance.id, at.0, at.1, SemanticSiteEdge::After, b)? {
                if !matches!(site.action, SemanticSiteAction::Call { .. } | SemanticSiteAction::BodyCall { .. }) { continue; }
                if receipt.replace(site).is_some() { return Err(fail("operation Fixed transport repeats its original receipt")); }
            }
            let receipt = receipt.ok_or_else(|| fail("operation Fixed transport has no original receipt"))?;
            ordinary = self.original.call_arguments(instance, receipt.recipe, self.program, self.inventory, b)?;
            (ordinary.0, ordinary.1.as_slice())
        };
        let worlds = self.function_worlds(callee.id, b)?;
        if worlds.is_empty() { return Err(fail("operation Fixed transport has no complete callee worlds")); }
        for world in worlds {
            if world.definition != target { return Err(fail("operation Fixed transport changes its original target")); }
            self.operation_mixed_arguments(world, arguments, b)?;
            for ordinal in 0..contract.fixed_binder_count {
                let binder = StorageBinder::Fixed(ordinal);
                let original = self.header_class(world, binder, b)?;
                let mut source = self.operation_mixed_class(&original, world.id, arguments, b)?;
                if source.is_none() && let Some(mixed) = mixed {
                    let owner = match original {
                        Class::Formal(owner, _) | Class::TypeArgument(owner, _, _) => Some(owner),
                        _ => None,
                    };
                    if let Some(owner) = owner {
                        b.lookup(mixed.parents.len(), WORK)?;
                        if let Some(arguments) = mixed.parents.get(&owner) {
                            source = self.operation_mixed_class(&original, owner, arguments, b)?;
                        }
                    }
                }
                let source = if let Some(source) = source { source } else {
                    let owner = match original {
                        Class::Formal(owner, _) | Class::TypeArgument(owner, _, _) => owner,
                        _ => return Err(fail("operation Fixed source is neither a genuine mixed formal nor a lexical ancestor")),
                    };
                    // Unsubstituted lexical ancestors must genuinely belong to
                    // this caller; class-image equality cannot choose a scope.
                    let mut current = Some(instance);
                    let mut found = false;
                    for _ in 0..=b.partial_depth {
                        let Some(scope) = current else { break; };
                        b.spend(1, WORK)?;
                        if scope.id == owner { found = true; break; }
                        current = scope.parent.map(|parent| self.inventory.instance(parent, b)).transpose()?;
                    }
                    if !found { return Err(unknown("operation Fixed class retains an unresolved lexical dependency")); }
                    original
                };
                self.install_operation_class(rows, binder, source, b)?;
            }
        }
        Ok(())
    }

    /// `receiving` names only an already independently proved directed outer
    /// Receiver view. Every other class, including phantom/nominal children,
    /// still comes from the exact full actual header below.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn transport_operation_output(
        &self,instance:&SemanticInstance,function:&TypedFunction,at:(BlockId,usize),
        callee:&TypedFunction,arguments:&[ValueId],parameters:&[Value],output:Value,
        fixed:Option<&[Class]>,receiving:Option<u32>,mixed:Option<&MixedCall<'_>>,b:&mut Budget,depth:usize,
    )->CompileResult<Value> {
        b.spend(1,WORK)?;
        if depth>b.partial_depth {return Err(vec![zil_base::Diagnostic::limit(
            zil_base::Phase::Analyze,"complete operation transport exceeds finite type depth",None)]);}
        output.check_geometry(b)?;
        if !geometry::same_type(&output.ty,&callee.return_type,b)?
            || output.classes.len()!=output.life.binder_count as usize
            || arguments.len()!=parameters.len() || callee.parameters.len()!=parameters.len() {
            return Err(fail("operation changes its original complete P/R geometry or arity"));
        }
        let contract=callee.storage_life.as_ref()
            .ok_or_else(||unknown("operation entry has no completed class directory"))?;
        crate::signature_life::charge(contract,b)?;
        let mut sources=BTreeMap::new();
        if let Some(fixed)=fixed {
            if fixed.len()!=contract.fixed_binder_count as usize {
                return Err(fail("operation receiver loses its complete captured domain"));
            }
            for (index,class) in fixed.iter().enumerate() {
                b.spend(1,WORK)?;
                self.install_operation_class(&mut sources,StorageBinder::Fixed(index as u32),class.clone(),b)?;
            }
        }
        if fixed.is_none() {
            self.operation_fixed_sources(instance, at, callee, contract, &mut sources, mixed, b)?;
        }
        for (ordinal,(operand,expected)) in arguments.iter().zip(parameters).enumerate() {
            operations::available_before(function,(at.0,at.1,SemanticSiteEdge::Before),*operand,b)?;
            let actual=self.ssa_value(instance,function,*operand,b,depth+1)?;
            if !geometry::same_type(&actual.ty,&expected.ty,b)? {
                return Err(fail("operation argument changes its whole original type"));
            }
            let valid=if matches!(expected.ty,Type::Address {..}|Type::Desc(_)) {
                self.outer_call_role(instance,function,at,ordinal,*operand,&actual,expected,b,depth+1)?
            } else {self.capture_values_equal(&actual,expected,b)?};
            if !valid {return Err(fail("operation argument loses its complete original class/outer role transport"));}
            for row in &contract.bindings {
                b.spend(row.position.path.len().saturating_add(1),WORK)?;
                if row.position.base!=StorageBase::Parameter(ordinal as u32) {continue;}
                let class=self.operation_outer_class(expected,&row.position.path,b)?
                    .ok_or_else(||fail("operation parameter omits an exact full physical occurrence"))?;
                let source=expected.classes.get(class as usize)
                    .ok_or_else(||fail("operation parameter outer selector leaves its whole domain"))?;
                self.install_operation_class(&mut sources,row.binder,source.clone(),b)?;
            }
            for row in &contract.classes {
                b.spend(row.position.path.len().saturating_add(1),WORK)?;
                if row.position.base!=StorageBase::Parameter(ordinal as u32) {continue;}
                let class=self.operation_child_class(expected,&row.position.path,row.position.class,b)?
                    .ok_or_else(||fail("operation parameter omits an invariant/phantom child occurrence"))?;
                let source=expected.classes.get(class as usize)
                    .ok_or_else(||fail("operation parameter intrinsic selector leaves its whole domain"))?;
                self.install_operation_class(&mut sources,row.binder,source.clone(),b)?;
            }
        }
        let mut outputs:BTreeMap<u32,Vec<StorageBinder>>=BTreeMap::new();
        for row in &contract.bindings {
            b.spend(row.position.path.len().saturating_add(1),WORK)?;
            if row.position.base!=StorageBase::Result {continue;}
            let class=self.operation_outer_class(&output,&row.position.path,b)?
                .ok_or_else(||fail("operation result omits a complete actual physical occurrence"))?;
            b.lookup(outputs.len(),WORK)?;
            self.push_value(outputs.entry(class).or_default(),row.binder,b)?;
        }
        for row in &contract.classes {
            b.spend(row.position.path.len().saturating_add(1),WORK)?;
            if row.position.base!=StorageBase::Result {continue;}
            let class=self.operation_child_class(&output,&row.position.path,row.position.class,b)?
                .ok_or_else(||fail("operation result omits a complete invariant/phantom occurrence"))?;
            b.lookup(outputs.len(),WORK)?;
            self.push_value(outputs.entry(class).or_default(),row.binder,b)?;
        }
        for (child,expected) in output.classes.iter().enumerate() {
            let child=u32::try_from(child).map_err(|_|fail("operation result class ordinal overflows"))?;
            if receiving==Some(child) {continue;}
            b.lookup(outputs.len(),WORK)?;
            let binders=outputs.get(&child)
                .ok_or_else(||fail("operation result has an untransported complete class"))?;
            let expected=self.capture_class_image(expected.clone(),b)?;
            for binder in binders {
                b.lookup(sources.len(),WORK)?;
                let mut source=sources.get(binder).cloned();
                if source.is_none() {
                    for candidate in self.call_result_input_binders(callee,*binder,b)? {
                        b.lookup(sources.len(),WORK)?;
                        if let Some(class)=sources.get(&candidate) {
                            if source.as_ref().is_some_and(|old|old!=class) {
                                return Err(fail("operation result has conflicting independently proved input relations"));
                            }
                            b.spend(1,WORK)?;source=Some(class.clone());
                        }
                    }
                }
                if source.is_none() {
                    source=self.reference_call_result_class(instance,function,at,callee,child,&output,b,depth+1)?;
                }
                let source=source.ok_or_else(||unknown("operation result retains an unresolved original source dependency"))?;
                if self.capture_class_image(source,b)?!=expected {
                    return Err(fail("operation result changes its original full class transport"));
                }
            }
        }
        // Empty domains still passed the exact original type, complete entry,
        // argument, and occurrence checks. They never imply Static/NoExternal.
        Ok(output)
    }
}
