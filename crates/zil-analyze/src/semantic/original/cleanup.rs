//! A concrete cleanup occurrence is not a structural SyncDrop premise.
//! Bind it to every actual owner/closure instance and defer only the terminal
//! worker/frame proof until raw preflight and the resource ledger complete.
use super::*;

impl Solver<'_, '_> {
    fn bind_task_cleanup(
        &self,
        module: &TypedModule,
        term: Term,
        b: &mut Budget,
    ) -> CompileResult<bool> {
        let root = self.type_root(term, b)?;
        let Kind::Wrapped(Wrapper::Managed, payload) = self.state.arena[root] else {
            return Ok(false);
        };
        if !matches!(self.state.arena[self.type_root(payload, b)?], Kind::Task(_)) {
            return Ok(false);
        }
        let mut locals = BTreeSet::new();
        for frame in self.state.frames.values() {
            for (binding, original) in &frame.bindings {
                b.spend(1, WORK)?;
                // Select the independently checked binding node, never a
                // same-typed local, name, layout or caller's concrete image.
                if self.type_root(*original, b)? != root {
                    continue;
                }
                let index = self.inventory.original_index(frame.definition, b)?;
                let recipe = index.declaration_site(*binding, b)?;
                let mut instances = 0usize;
                for instance in &self.program.instances {
                    b.spend(1, WORK)?;
                    if instance.definition != frame.definition {
                        continue;
                    }
                    let SemanticInstanceTarget::Function(target) = &instance.target else {
                        return Err(fail("original Task cleanup binding has no actual function instance"));
                    };
                    let function = self.inventory.function(target, b)?;
                    let site = self.inventory.emitted_site(self.program, instance.id, recipe, b)?;
                    let SemanticSiteAction::DeclareLocal {
                        local: SemanticLocalTarget::Typed(local), ..
                    } = site.action else {
                        return Err(fail("original Task cleanup changes its actual binding operation"));
                    };
                    if !function.locals.get(local.0 as usize)
                        .filter(|declaration| declaration.id == local)
                        .is_some_and(|declaration| matches!(&declaration.ty,
                            Type::Managed(payload) if matches!(payload.as_ref(), Type::Task { .. })))
                    {
                        return Err(fail("original Task cleanup binding changes its actual owning family"));
                    }
                    b.lookup(locals.len(), WORK)?;
                    locals.insert((function.id, local));
                    instances += 1;
                }
                if instances == 0 {
                    return Err(fail("original Task cleanup binding has no complete actual instances"));
                }
            }
        }
        if locals.is_empty() {
            return Ok(false);
        }
        for (function, local) in locals {
            let function = module.functions.get(function.0 as usize)
                .filter(|actual| actual.id == function)
                .ok_or_else(|| fail("original Task cleanup loses its actual binding function"))?;
            let mut found = false;
            for block in &function.blocks {
                for (position, instruction) in block.instructions.iter().enumerate() {
                    b.spend(1, WORK)?;
                    if matches!(instruction.kind, InstructionKind::DropManaged { local: actual } if actual == local) {
                        b.lookup(b.semantic_cleanup_sites.len(), WORK)?;
                        b.semantic_cleanup_sites.insert((function.id, block.id, position, local, None));
                        found = true;
                    }
                }
            }
            if !found {
                return Err(fail("original Task cleanup binding has no actual DropManaged site"));
            }
        }
        Ok(true)
    }

    fn task_cleanup_environment(
        &self,
        term: Term,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<bool> {
        self.step(depth, b)?;
        let term = self.root(term, b)?;
        match &self.state.arena[term] {
            Kind::CallResult { hidden, .. } => {
                self.task_cleanup_environment(*hidden, b, depth + 1)
            }
            Kind::Wrapped(Wrapper::Managed, payload) => {
                Ok(matches!(self.state.arena[self.root(*payload, b)?], Kind::Task(_)))
            }
            Kind::Closure { environment, .. } => {
                let mut task = false;
                for captured in environment {
                    let root = self.root(*captured, b)?;
                    if matches!(self.state.arena[root], Kind::Life(_) | Kind::LifeVariable(_)
                        | Kind::ConstRigid { .. } | Kind::ConstValue(_))
                    {
                        continue;
                    }
                    if self.satisfies(root, Predicate::SyncDrop, b, depth + 1)? {
                        continue;
                    }
                    if !self.task_cleanup_environment(root, b, depth + 1)? {
                        return Ok(false);
                    }
                    task = true;
                }
                Ok(task)
            }
            // An abstract F or arbitrary resource does not acquire a cleanup
            // theorem from one of its concrete callers.
            _ => Ok(false),
        }
    }

    fn bind_cleanup(
        &self,
        module: &TypedModule,
        term: Term,
        b: &mut Budget,
    ) -> CompileResult<bool> {
        let mut term = self.root(term, b)?;
        for depth in 0..=b.partial_depth {
            self.step(depth, b)?;
            if let Kind::CallResult { hidden, .. } = self.state.arena[term] {
                term = self.root(hidden, b)?;
            } else {
                break;
            }
        }
        let Kind::Closure { definition, .. } = self.state.arena[term] else {
            return Ok(false);
        };
        if !self.task_cleanup_environment(term, b, 0)? {
            return Ok(false);
        }
        let mut closures = BTreeSet::new();
        for instance in &self.program.instances {
            b.spend(1, WORK)?;
            if instance.definition != definition {
                continue;
            }
            let SemanticInstanceTarget::Function(target) = &instance.target else {
                return Err(fail("original concrete cleanup has no actual function instance"));
            };
            let entry = self.inventory.function(target, b)?;
            let id = entry.environment.ok_or_else(|| fail(
                "original concrete cleanup loses its actual closure environment"))?;
            let environment = instance.environment.as_ref().ok_or_else(|| fail(
                "original concrete cleanup has no complete creator environment"))?;
            let closure = module.closures.get(id.0 as usize)
                .filter(|closure| closure.id == id && closure.function == entry.id)
                .ok_or_else(|| fail("original concrete cleanup has another actual closure entry"))?;
            if environment.closure != SemanticClosureTarget::Typed(closure.id) {
                return Err(fail("original concrete cleanup changes its actual environment identity"));
            }
            b.lookup(closures.len(), WORK)?;
            closures.insert(id);
        }
        if closures.is_empty() {
            return Err(fail("original concrete cleanup has no complete actual closure instance"));
        }
        let mut found = BTreeSet::new();
        for function in &module.functions {
            for block in &function.blocks {
                for (position, instruction) in block.instructions.iter().enumerate() {
                    b.spend(1, WORK)?;
                    let InstructionKind::DropClosure { local } = instruction.kind else {
                        continue;
                    };
                    let Some(Type::Closure(id)) = function.locals.get(local.0 as usize)
                        .filter(|declaration| declaration.id == local).map(|declaration| &declaration.ty)
                    else { continue; };
                    b.lookup(closures.len(), WORK)?;
                    if !closures.contains(id) { continue; }
                    b.lookup(b.semantic_cleanup_sites.len(), WORK)?;
                    b.semantic_cleanup_sites.insert((function.id, block.id, position, local, Some(*id)));
                    b.lookup(found.len(), WORK)?;
                    found.insert(*id);
                }
            }
        }
        b.spend(closures.len().saturating_add(found.len()), WORK)?;
        if found != closures {
            return Err(fail("original concrete cleanup has no actual DropClosure site for every instance"));
        }
        Ok(true)
    }

    pub(super) fn check_cleanups(&self, module: &TypedModule, b: &mut Budget) -> CompileResult<()> {
        for term in &self.cleanup_terms {
            if self.satisfies(*term, Predicate::SyncDrop, b, 0)? {
                continue;
            }
            if !self.bind_cleanup(module, *term, b)? && !self.bind_task_cleanup(module, *term, b)? {
                let mut errors = fail("original live cleanup edge requires SyncDrop or its complete actual Task cleanup proof");
                errors[0].notes.push(format!("original cleanup term {}, whole kind {:?}",
                    self.type_root(*term, b)?, std::mem::discriminant(&self.state.arena[self.type_root(*term, b)?])));
                return Err(errors);
            }
        }
        Ok(())
    }
}

pub(super) fn finish(module: &TypedModule, b: &mut Budget) -> CompileResult<()> {
    if b.semantic_cleanup_sites.is_empty() {
        return if b.semantic_cleanup_proofs.is_empty() { Ok(()) }
            else { Err(fail("actual Task cleanup proof has no original pending occurrence")) };
    }
    if b.task_input_scopes.is_none() {
        return Err(fail("original concrete cleanup has no complete actual Task input scopes"));
    }
    b.spend(b.semantic_cleanup_sites.len().saturating_mul(5), WORK)?;
    let sites: Vec<_> = b.semantic_cleanup_sites.iter().copied().collect();
    let mut frames = BTreeSet::new();
    for (function, block, position, local, closure) in sites {
        if !b.task_origin_roots || !b.tasks {
            return Err(fail("original concrete cleanup reached no complete actual Task preflight"));
        }
        let function = module.functions.get(function.0 as usize)
            .filter(|actual| actual.id == function)
            .ok_or_else(|| fail("original concrete cleanup loses its actual function"))?;
        let instruction = function.blocks.get(block.0 as usize)
            .filter(|actual| actual.id == block)
            .and_then(|actual| actual.instructions.get(position))
            .ok_or_else(|| fail("original concrete cleanup loses its actual instruction"))?;
        let declaration = function.locals.get(local.0 as usize)
            .filter(|actual| actual.id == local)
            .ok_or_else(|| fail("original concrete cleanup loses its actual local"))?;
        let exact = match closure {
            Some(closure) => matches!(instruction.kind, InstructionKind::DropClosure { local: actual } if actual == local)
                && declaration.ty == Type::Closure(closure),
            None => matches!(instruction.kind, InstructionKind::DropManaged { local: actual } if actual == local)
                && matches!(&declaration.ty, Type::Managed(payload) if matches!(payload.as_ref(), Type::Task { .. })),
        };
        if !exact || !crate::tasks::needs_synchronous_proof(module, &declaration.ty, b)? {
            return Err(fail("original concrete cleanup changes its exact actual Task-bearing site"));
        }
        b.spend(function.name.len().saturating_add(128), WORK)?;
        if closure.is_none() {
            b.lookup(b.semantic_cleanup_proofs.len(), WORK)?;
            if !b.semantic_cleanup_proofs.contains(&(function.id, block, position, local)) {
                let mut errors = fail("original Task cleanup lacks its complete actual resource ledger proof");
                errors[0].notes.push(format!("function {} {}, block {} instruction {}, original-bound local {}",
                    function.id.0, function.name, block.0, position, local.0));
                return Err(errors);
            }
            if function.call_kind == CallKind::Suspend && frames.insert(function.id) {
                crate::tasks::cleanup_frame_profile(module, function, b)?;
            }
            continue;
        }
        let proof = match function.call_kind {
            CallKind::Direct => crate::tasks::require_original_synchronous_drop(
                module, function, local, (block.0 as usize, position), b, Some(instruction.span)),
            CallKind::Suspend => {
                b.lookup(frames.len(), WORK)?;
                if frames.insert(function.id) {
                    crate::tasks::cleanup_frame_profile(module, function, b)
                } else {
                    Ok(())
                }
            }
        };
        proof.map_err(|mut errors| {
            for error in &mut errors {
                error.notes.push(format!("original concrete cleanup actual function {} {}, block {} instruction {}, local {}, closure {:?}", function.id.0, function.name, block.0, position, local.0, closure));
            }
            errors
        })?;
    }
    // Called only after tasks::check and resources::check_module succeeded.
    // A failure above leaves every pending site intact in this request.
    b.semantic_cleanup_sites.clear();
    b.semantic_cleanup_proofs.clear();
    Ok(())
}
