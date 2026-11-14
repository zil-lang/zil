//! Sealed capabilities in the original declaration context. A concrete caller
//! cannot supply a missing premise, and ordinary signature/role equations do
//! not become thread or environment qualifications.
use super::*;
impl Solver<'_, '_> {
    pub(super) fn capability(
        &self,
        term: Term,
        kind: SemanticBuiltinTrait,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<bool> {
        self.step(depth, b)?;
        let term = self.type_root(term, b)?;
        match &self.state.arena[term] {
            Kind::Rigid(owner, ordinal) => {
                let declaration = self.inventory.definition(*owner, b)?;
                let Some(SemanticFormal::Type { requirements }) =
                    declaration.parameters.get(*ordinal as usize)
                else {
                    return Err(fail("original sealed capability changes its formal kind"));
                };
                Ok(self.assumption(
                    *owner,
                    *ordinal,
                    requirements,
                    Predicate::Capability(kind),
                    b,
                )? || self.assumption(
                    *owner,
                    *ordinal,
                    &declaration.requirements,
                    Predicate::Capability(kind),
                    b,
                )?)
            }
            Kind::Atom(ty) => match kind {
                SemanticBuiltinTrait::ClosedEnvironment
                | SemanticBuiltinTrait::Transfer
                | SemanticBuiltinTrait::Share => {
                    Ok(ty.is_b0_scalar() || *ty == Type::ForeignHandle || *ty == Type::ByteBuffer)
                }
                SemanticBuiltinTrait::ThreadCallable | SemanticBuiltinTrait::Drop => Ok(false),
                _ => Err(unknown(
                    "original sealed alias needs its independently specified capability relation",
                )),
            },
            Kind::Desc { .. } | Kind::Address { .. } | Kind::Slice(_) => Ok(false),
            Kind::Array { element, .. } => self.capability(*element, kind, b, depth + 1),
            Kind::ErrorUnion { error, result } => {
                Ok(self.capability(*error, kind, b, depth + 1)?
                    && self.capability(*result, kind, b, depth + 1)?)
            }
            Kind::Tuple(fields) => {
                if kind == SemanticBuiltinTrait::ThreadCallable {
                    return Ok(false);
                }
                for field in fields {
                    if !self.capability(*field, kind, b, depth + 1)? {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
            Kind::Nominal {
                definition,
                arguments,
                fields,
            } => {
                if kind == SemanticBuiltinTrait::ThreadCallable {
                    return Ok(false);
                }
                let declaration = self.inventory.definition(*definition, b)?;
                if matches!(
                    declaration.body,
                    SemanticDefinitionBody::Nominal {
                        kind: LifeNominalKind::Resource,
                        ..
                    }
                ) {
                    return Err(unknown(
                        "original resource capability requires its complete Drop body and execution theorem",
                    ));
                }
                if kind == SemanticBuiltinTrait::ClosedEnvironment {
                    for argument in arguments {
                        match &self.state.arena[self.type_root(*argument, b)?] {
                            Kind::Life(_) | Kind::LifeVariable(_) => return Ok(false),
                            Kind::ConstRigid { .. } | Kind::ConstValue(_) => {}
                            _ => {
                                if !self.capability(*argument, kind, b, depth + 1)? {
                                    return Ok(false);
                                }
                            }
                        }
                    }
                }
                for field in fields {
                    if !self.capability(*field, kind, b, depth + 1)? {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
            Kind::Wrapped(Wrapper::Managed, payload) => {
                if !matches!(
                    kind,
                    SemanticBuiltinTrait::ClosedEnvironment | SemanticBuiltinTrait::Transfer
                ) {
                    return Ok(false);
                }
                self.capability(*payload, kind, b, depth + 1)
            }
            Kind::Wrapped(Wrapper::Store, element) => {
                if !matches!(
                    kind,
                    SemanticBuiltinTrait::ClosedEnvironment | SemanticBuiltinTrait::Transfer
                ) {
                    return Ok(false);
                }
                Ok(
                    self.satisfies(*element, Predicate::StoreElement, b, depth + 1)?
                        && self.capability(*element, kind, b, depth + 1)?,
                )
            }
            Kind::Wrapped(Wrapper::Registration, environment) => {
                if kind == SemanticBuiltinTrait::Transfer {
                    return Err(unknown(
                        "original registration transfer needs its full provider family and driver promise",
                    ));
                }
                if kind == SemanticBuiltinTrait::ClosedEnvironment {
                    self.capability(*environment, kind, b, depth + 1)
                } else {
                    Ok(false)
                }
            }
            Kind::Wrapped(Wrapper::AsyncRegistration, _) => Ok(false),
            Kind::Wrapped(Wrapper::Mutex, payload) => {
                if matches!(
                    kind,
                    SemanticBuiltinTrait::Transfer
                        | SemanticBuiltinTrait::Share
                        | SemanticBuiltinTrait::ClosedEnvironment
                ) {
                    self.capability(*payload, kind, b, depth + 1)
                } else {
                    Ok(false)
                }
            }
            Kind::Code {
                kind: code,
                parameters,
                result,
                ..
            } => match kind {
                SemanticBuiltinTrait::ClosedEnvironment => {
                    for term in parameters.iter().chain(std::iter::once(result)) {
                        if !self.capability(*term, kind, b, depth + 1)? {
                            return Ok(false);
                        }
                    }
                    Ok(true)
                }
                SemanticBuiltinTrait::Transfer | SemanticBuiltinTrait::ThreadCallable => {
                    Err(unknown(
                        "original code transfer or invocation needs its complete target and execution graph",
                    ))
                }
                SemanticBuiltinTrait::Share => {
                    Ok(*code != SemanticCodeKind::BorrowedFunctionPointer)
                }
                _ => Ok(false),
            },
            Kind::Closure {
                definition,
                environment,
                ..
            } => {
                if !matches!(
                    kind,
                    SemanticBuiltinTrait::ClosedEnvironment
                        | SemanticBuiltinTrait::Transfer
                        | SemanticBuiltinTrait::ThreadCallable
                ) {
                    return Ok(false);
                }
                let (creator, node) = self.creators.get(definition).copied().ok_or_else(|| {
                    fail("original environment capability has no real creator occurrence")
                })?;
                b.lookup(self.indexes.len(), WORK)?;
                let original = self
                    .indexes
                    .get(&creator)
                    .ok_or_else(|| fail("original environment capability has no creator index"))?
                    .get_expr(node, b)?;
                let CoreExprKind::Closure { move_capture, .. } = original.kind else {
                    return Err(fail(
                        "original environment capability selects another creator operation",
                    ));
                };
                if !move_capture && !environment.is_empty() {
                    return Ok(false);
                }
                for captured in environment {
                    let capability = if kind == SemanticBuiltinTrait::ThreadCallable {
                        SemanticBuiltinTrait::Transfer
                    } else {
                        kind
                    };
                    match &self.state.arena[self.type_root(*captured, b)?] {
                        Kind::Life(_) | Kind::LifeVariable(_) => return Ok(false),
                        Kind::ConstRigid { .. } | Kind::ConstValue(_) => {}
                        _ => {
                            if !self.capability(*captured, capability, b, depth + 1)? {
                                return Ok(false);
                            }
                        }
                    }
                }
                if kind == SemanticBuiltinTrait::ClosedEnvironment {
                    let frame = self.state.frames.get(definition).ok_or_else(|| {
                        fail("original closure scope has no checked signature frame")
                    })?;
                    for term in frame
                        .parameters
                        .iter()
                        .chain(std::iter::once(&frame.result))
                    {
                        if !self.signature_closed(*term, *definition, b, depth + 1)? {
                            return Ok(false);
                        }
                    }
                }
                if kind == SemanticBuiltinTrait::ThreadCallable {
                    self.execution_capability(*definition, false, b)?;
                }
                Ok(true)
            }
            Kind::Life(_)
            | Kind::LifeVariable(_)
            | Kind::ConstRigid { .. }
            | Kind::ConstValue(_) => Err(fail(
                "original sealed Type capability has another semantic kind",
            )),
            Kind::Infer(_) => Err(unknown(
                "original sealed capability remains unresolved in its own declaration context",
            )),
            Kind::Associated { .. } | Kind::SelfType(_) => Err(unknown(
                "original associated sealed capability requires its exact member bound or provider theorem",
            )),
            Kind::CallResult { .. } => Err(fail(
                "original capability failed to open its checked result role",
            )),
        }
    }
    fn signature_closed(
        &self,
        term: Term,
        owner: SemanticDefinitionId,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<bool> {
        self.step(depth, b)?;
        match &self.state.arena[self.type_root(term, b)?] {
            Kind::Atom(_) => Ok(true),
            Kind::Rigid(_, _) => {
                self.capability(term, SemanticBuiltinTrait::ClosedEnvironment, b, depth + 1)
            }
            Kind::Life(Life::Invocation(actual, _)) | Kind::Life(Life::Implicit(actual, _)) => {
                Ok(*actual == owner)
            }
            Kind::Life(_) | Kind::LifeVariable(_) => Ok(false),
            Kind::ConstRigid { .. } | Kind::ConstValue(_) => Ok(true),
            Kind::Address { life, inner, .. } | Kind::Desc { life, inner } => Ok(self
                .signature_closed(*life, owner, b, depth + 1)?
                && self.signature_closed(*inner, owner, b, depth + 1)?),
            Kind::Array { element, .. } | Kind::Slice(element) | Kind::Wrapped(_, element) => {
                self.signature_closed(*element, owner, b, depth + 1)
            }
            Kind::ErrorUnion { error, result } => {
                Ok(self.signature_closed(*error, owner, b, depth + 1)?
                    && self.signature_closed(*result, owner, b, depth + 1)?)
            }
            Kind::Tuple(fields) => {
                for field in fields {
                    if !self.signature_closed(*field, owner, b, depth + 1)? {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
            Kind::Nominal {
                arguments, fields, ..
            } => {
                for field in arguments.iter().chain(fields) {
                    if !self.signature_closed(*field, owner, b, depth + 1)? {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
            Kind::Code {
                parameters, result, ..
            } => {
                for parameter in parameters.iter().chain(std::iter::once(result)) {
                    if !self.signature_closed(*parameter, owner, b, depth + 1)? {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
            Kind::Closure { .. } => {
                self.capability(term, SemanticBuiltinTrait::ClosedEnvironment, b, depth + 1)
            }
            Kind::Infer(_) => Err(unknown(
                "original closure signature free-class domain remains unresolved",
            )),
            Kind::Associated { .. } | Kind::SelfType(_) => Err(unknown(
                "original associated signature domain requires its full member substitution",
            )),
            Kind::CallResult { .. } => Err(fail(
                "original signature domain failed to open its checked call result",
            )),
        }
    }
    pub(super) fn execution_capability(
        &self,
        root: SemanticDefinitionId,
        resume: bool,
        b: &mut Budget,
    ) -> CompileResult<()> {
        let mut pending = vec![(root, resume)];
        let mut seen = BTreeSet::new();
        while let Some((owner, resume)) = pending.pop() {
            b.lookup(seen.len(), WORK)?;
            if !seen.insert((owner, resume)) {
                continue;
            }
            let declaration = self.inventory.definition(owner, b)?;
            let SemanticDefinitionBody::FunctionWithBody {
                signature,
                foreign,
                body: SemanticOriginalFunctionBody::Defined(_),
                resume_bound,
                ..
            } = &declaration.body
            else {
                return Err(unknown(
                    "original thread execution requires each complete defined body and original foreign contract",
                ));
            };
            if foreign.is_some()
                || (!resume && signature.call_kind != CallKind::Direct)
                || (resume
                    && signature.call_kind == CallKind::Suspend
                    && *resume_bound != TaskResumeBound::Transfer)
            {
                return Err(fail(
                    "original execution graph violates its declared thread or resume ABI",
                ));
            }
            let frame = self.state.frames.get(&owner).ok_or_else(|| {
                fail("original thread graph selects no universally checked body frame")
            })?;
            for term in frame
                .parameters
                .iter()
                .chain(std::iter::once(&frame.result))
                .chain(&frame.environment)
                .chain(frame.bindings.values())
                .chain(frame.expressions.values())
            {
                if !self.capability(*term, SemanticBuiltinTrait::Transfer, b, 0)? {
                    return Err(fail(
                        "original thread execution retains unproved data, an external borrowed head or cleanup dependency",
                    ));
                }
            }
            for ((caller, node), call) in &self.state.calls {
                b.spend(1, WORK)?;
                if *caller != owner {
                    continue;
                }
                if call.closure {
                    let callee = self.inventory.definition(call.target, b)?;
                    let SemanticDefinitionBody::FunctionWithBody { signature, .. } = &callee.body
                    else {
                        return Err(fail(
                            "original thread call lacks its full callable signature",
                        ));
                    };
                    if signature.call_kind != CallKind::Direct {
                        return Err(unknown(
                            "original closure thread call needs its full asynchronous frame theorem",
                        ));
                    }
                }
                let callee = self.inventory.definition(call.target, b)?;
                let SemanticDefinitionBody::FunctionWithBody { signature, .. } = &callee.body
                else {
                    return Err(fail(
                        "original execution call lacks its complete declared ABI",
                    ));
                };
                let target_resume = signature.call_kind == CallKind::Suspend;
                if target_resume && !resume {
                    return Err(fail(
                        "original Direct thread execution invokes a Suspend callee",
                    ));
                }
                let _ = node;
                Solver::push(&mut pending, (call.target, target_resume), b)?;
            }
            for ((caller, _), call) in &self.state.abstract_calls {
                b.spend(1, WORK)?;
                if *caller == owner
                    && !self.capability(call.subject, SemanticBuiltinTrait::ThreadCallable, b, 0)?
                {
                    return Err(fail(
                        "original abstract thread call lacks ThreadCallable in its own context",
                    ));
                }
            }
            for ((caller, _), intrinsic) in &self.state.intrinsics {
                b.spend(1, WORK)?;
                if *caller != owner {
                    continue;
                }
                match intrinsic {
                    sealed::Intrinsic::NumericConvert(_)
                    | sealed::Intrinsic::SliceLength
                    | sealed::Intrinsic::BytesLen
                    | sealed::Intrinsic::BytesCapacity => {}
                    sealed::Intrinsic::TaskYield if resume => {}
                    _ => {
                        return Err(unknown(
                            "original thread operation needs its complete sealed data, provider and cleanup execution scheme",
                        ));
                    }
                }
            }
        }
        Ok(())
    }
    pub(super) fn check_resume_capabilities(&self, b: &mut Budget) -> CompileResult<()> {
        for definition in self.inventory.definitions.values() {
            if let SemanticDefinitionBody::FunctionWithBody {
                resume_bound: TaskResumeBound::Transfer,
                ..
            } = definition.body
            {
                self.execution_capability(definition.id, true, b)?;
            }
        }
        Ok(())
    }
}
