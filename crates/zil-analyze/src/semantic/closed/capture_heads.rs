//! The implicit class of an actual borrowed scalar capture. Its original
//! occurrence, active entry, physical capture and creator operand are all
//! checked; the class never supplies a target, parent loan or storage epoch.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Head {
    Local(FunctionId, LocalId),
    Capture(FunctionId, u32),
}
type Heads = BTreeSet<Head>;

impl Check<'_, '_> {
    fn instruction<'a>(&self, function: &'a TypedFunction, value: ValueId, b: &mut Budget) -> CompileResult<&'a InstructionKind> {
        let mut found = None;
        for block in &function.blocks {
            b.spend(block.instructions.len().saturating_add(1), WORK)?;
            for instruction in &block.instructions {
                if instruction.result == Some(value) && found.replace(&instruction.kind).is_some() {
                    return Err(fail("implicit capture operand has competing actual definitions"));
                }
            }
        }
        found.ok_or_else(|| fail("implicit capture operand has no actual definition"))
    }

    fn captured_read(&self, function: &TypedFunction, value: ValueId, b: &mut Budget) -> CompileResult<u32> {
        let InstructionKind::ReadAddress { address } = self.instruction(function, value, b)? else {
            return Err(unknown("implicit capture occurrence needs its exact actual captured-head read"));
        };
        let InstructionKind::ReadCapture { index } = self.instruction(function, *address, b)? else {
            return Err(fail("original implicit capture occurrence selects another actual head"));
        };
        Ok(*index)
    }

    /// Fixed ordinal -> actual physical capture ordinal. Every row is derived
    /// from the original free-name occurrence and its real ReadCapture, never
    /// from the order or count of captured Types.
    pub(super) fn capture_directory(&self, id: ClosureId, b: &mut Budget) -> CompileResult<Vec<u32>> {
        b.spend(self.module.closures.len().saturating_add(1), WORK)?;
        let closure = self.module.closures.get(id.0 as usize).filter(|closure| closure.id == id)
            .ok_or_else(|| fail("implicit head directory names an absent closure"))?;
        let life = closure.storage_life.as_ref().ok_or_else(|| fail("actual capture directory is Unknown"))?;
        let instance = self.function_instance(closure.function, b)?;
        let definition = self.inventory.definition(instance.definition, b)?;
        let function = self.inventory.function(&SemanticFunctionTarget::Typed(closure.function), b)?;
        let SemanticDefinitionBody::Function { sites: recipes, .. } = &definition.body else {
            return Err(fail("borrowed capture has no original closure body"));
        };
        if definition.kind != SemanticDefinitionKind::Closure || function.environment != Some(id)
            || life.invocation_binder_count != 0 || !life.classes.is_empty() || !life.static_bindings.is_empty()
            || function.storage_life.as_ref() != Some(life) || !closure.lending_captures.is_empty() {
            return Err(unknown("actual closure needs its full retained, lending or invocation context"));
        }
        b.spend((life.fixed_binder_count as usize).saturating_add(closure.captures.len()), WORK)?;
        let mut directory = vec![None; life.fixed_binder_count as usize];
        let mut seen_capture = BTreeSet::new();
        for binding in &life.bindings {
            b.spend(directory.len().saturating_add(seen_capture.len()).saturating_add(1), WORK)?;
            let (StorageBase::Capture(index), StorageBinder::Fixed(class)) = (binding.position.base, binding.binder) else {
                return Err(unknown("implicit scalar-capture directory contains another scope occurrence"));
            };
            if !binding.position.path.is_empty() || !seen_capture.insert(index)
                || !matches!(closure.captures.get(index as usize), Some(Type::Address { mutable: false, inner }) if inner.is_b0_scalar()) {
                return Err(fail("implicit scalar-capture binding loses its exact borrowed head or mode"));
            }
            let slot = directory.get_mut(class as usize).ok_or_else(|| fail("implicit capture Fixed ordinal is outside the complete directory"))?;
            if slot.replace(index).is_some() {
                return Err(unknown("several implicit captured heads need their independently proved joint relation"));
            }
        }
        for (index, ty) in closure.captures.iter().enumerate() {
            b.spend(seen_capture.len().saturating_add(1), WORK)?;
            if ty.is_b0_scalar() { continue; }
            if !matches!(ty, Type::Address { mutable: false, inner } if inner.is_b0_scalar()) || !seen_capture.contains(&(index as u32)) {
                return Err(unknown("actual closure has an unproved external, owning or phantom capture domain"));
            }
        }
        if instance.header_embedding.len() != directory.len() {
            return Err(fail("closure header omits its complete actual implicit head domain"));
        }
        for (class, index) in directory.iter().enumerate() {
            let index = index.ok_or_else(|| fail("actual capture directory contains an ungrounded Fixed class"))?;
            b.spend(instance.header_embedding.len(), WORK)?;
            let mut bindings = instance.header_embedding.iter().filter(|row| row.header == StorageBinder::Fixed(class as u32));
            let binding = bindings.next().ok_or_else(|| fail("actual capture Fixed class has no original occurrence embedding"))?;
            if bindings.next().is_some() { return Err(fail("actual capture Fixed embedding is repeated")); }
            let SemanticClassExpr::Implicit { scope: SemanticScopeRef::Ancestor(0), ordinal } = binding.actual else {
                return Err(unknown("capture Fixed class is not bound to its exact original implicit occurrence"));
            };
            b.spend(definition.implicit_lifetimes.len(), WORK)?;
            let mut implicit = definition.implicit_lifetimes.iter().filter(|row| row.ordinal == ordinal);
            let occurrence = implicit.next().ok_or_else(|| fail("capture head names an absent original implicit lifetime"))?;
            if implicit.next().is_some() { return Err(fail("original implicit capture ordinal is ambiguous")); }
            let mut found = false;
            for recipe in recipes {
                b.spend(recipe.occurrence_path.len().saturating_add(occurrence.occurrence_path.len()).saturating_add(1), WORK)?;
                if recipe.occurrence_path != occurrence.occurrence_path || !matches!(recipe.kind, SemanticSiteRecipeKind::TypeUse { .. }) { continue; }
                for site in &self.program.sites {
                    b.spend(1, WORK)?;
                    if site.instance != instance.id || site.recipe != recipe.id { continue; }
                    let SemanticSiteAction::TypeUse { subject: SemanticOperand::Value(value), .. } = site.action else {
                        return Err(fail("implicit capture occurrence has no actual scalar read"));
                    };
                    if self.captured_read(function, value, b)? != index { return Err(fail("capture implicit occurrence is rebound to another actual captured head")); }
                    found = true;
                }
            }
            if !found { return Err(fail("implicit captured head has no exact original name emission")); }
        }
        b.spend(directory.len(), WORK)?;
        directory.into_iter().map(|row| row.ok_or_else(|| fail("implicit head directory is incomplete"))).collect()
    }

    fn local_heads(&self, function: &TypedFunction, local: LocalId, class: Option<u32>, active: &mut BTreeSet<(FunctionId, ValueId, Option<u32>)>, depth: usize, b: &mut Budget) -> CompileResult<Heads> {
        let mut out = Heads::new();
        for block in &function.blocks {
            for instruction in &block.instructions {
                b.spend(1, WORK)?;
                if let InstructionKind::Store { local: destination, value } = instruction.kind && destination == local {
                    let sources = self.heads(function, value, class, active, depth + 1, b)?;
                    b.spend(out.len().saturating_add(sources.len()), WORK)?;
                    out.extend(sources);
                }
            }
        }
        if out.is_empty() { return Err(fail("implicit head has no actual initialized local origin")); }
        Ok(out)
    }

    fn heads(&self, function: &TypedFunction, value: ValueId, class: Option<u32>, active: &mut BTreeSet<(FunctionId, ValueId, Option<u32>)>, depth: usize, b: &mut Budget) -> CompileResult<Heads> {
        if depth > b.partial_depth { return Err(unknown("actual implicit-head derivation exceeds its finite bound")); }
        b.spend(active.len().saturating_add(1), WORK)?;
        let key = (function.id, value, class);
        if !active.insert(key) { return Err(fail("actual implicit-head source has an ungrounded cycle")); }
        let result = match (self.instruction(function, value, b)?, class) {
            (InstructionKind::BorrowLocal { local }, None) => {
                if !function.locals.get(local.0 as usize).is_some_and(|local| local.ty.is_b0_scalar()) { return Err(unknown("implicit capture borrow needs its complete actual local type")); }
                b.spend(1, WORK)?;
                Ok(Heads::from([Head::Local(function.id, *local)]))
            }
            (InstructionKind::ReadCapture { index }, None) => {
                let closure = function.environment.ok_or_else(|| fail("implicit captured address has no real environment"))?;
                let directory = self.capture_directory(closure, b)?;
                if !directory.contains(index) { return Err(fail("implicit captured address is not in the complete head directory")); }
                b.spend(1, WORK)?;
                Ok(Heads::from([Head::Capture(function.id, *index)]))
            }
            (InstructionKind::ReborrowAddress { address, mutable: false }, None) => self.heads(function, *address, None, active, depth + 1, b),
            (InstructionKind::Load(local) | InstructionKind::TakeLocal { local }, class) => self.local_heads(function, *local, class, active, depth + 1, b),
            (InstructionKind::MakeClosure { closure, captures }, Some(class)) => {
                let directory = self.capture_directory(*closure, b)?;
                let index = directory.get(class as usize).ok_or_else(|| fail("implicit closure class is outside the actual complete directory"))?;
                let operand = captures.get(*index as usize).ok_or_else(|| fail("actual creator omits the implicit capture operand"))?;
                self.heads(function, *operand, None, active, depth + 1, b)
            }
            _ => Err(unknown("implicit captured-head origin needs its complete actual transport")),
        };
        active.remove(&key);
        result
    }

    pub(super) fn captured_value(&self, instance: &SemanticInstance, value: &SemanticTypeValue, operand: &SemanticOperand, b: &mut Budget) -> CompileResult<Type> {
        let SemanticTypeValue::Resolved { ty: Type::Closure(id), life, classes } = value else {
            return self.value_type(value, b);
        };
        let directory = self.capture_directory(*id, b)?;
        if directory.is_empty() { return self.value_type(value, b); }
        if life.binder_count as usize != directory.len() || classes.len() != directory.len()
            || !life.slots.is_empty() || !life.retained.is_empty() || !life.nominal_arguments.is_empty() {
            return Err(fail("opaque closure value loses its actual complete intrinsic head domain"));
        }
        let SemanticInstanceTarget::Function(target) = &instance.target else { return Err(fail("capture value has no actual code context")); };
        let function = self.inventory.function(target, b)?;
        for (class, expression) in classes.iter().enumerate() {
            let expected = match operand {
                SemanticOperand::Value(value) => self.heads(function, *value, Some(class as u32), &mut BTreeSet::new(), 0, b)?,
                SemanticOperand::Local(local) => self.local_heads(function, *local, Some(class as u32), &mut BTreeSet::new(), 0, b)?,
                _ => return Err(unknown("implicit closure domain needs its exact actual value or occupant")),
            };
            let actual = match expression {
                SemanticClassExpr::OuterOccurrence { operand: SemanticOperand::Value(value), path, mode: StoredBorrowMode::SharedAddress } if path.is_empty() => self.heads(function, *value, None, &mut BTreeSet::new(), 0, b)?,
                SemanticClassExpr::IntrinsicClass { operand: SemanticOperand::Value(value), path, class } if path.is_empty() => self.heads(function, *value, Some(*class), &mut BTreeSet::new(), 0, b)?,
                SemanticClassExpr::IntrinsicClass { operand: SemanticOperand::Local(local), path, class } if path.is_empty() => self.local_heads(function, *local, Some(*class), &mut BTreeSet::new(), 0, b)?,
                _ => return Err(unknown("implicit closure image requires its actual emitted capture-head selector")),
            };
            b.spend(actual.len().saturating_add(expected.len()), WORK)?;
            if actual != expected { return Err(fail("closure intrinsic image belongs to another actual borrowed head")); }
        }
        Ok(Type::Closure(*id))
    }
}
