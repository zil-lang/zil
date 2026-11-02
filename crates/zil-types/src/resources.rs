//! Finite resource heads and explicit owner/descriptor operations.
use super::*;

pub(super) fn supported(ty: &Type) -> bool {
    dynamic_store::is_carrier(ty)
        || byte_buffer::resource(ty)
        || matches!(ty, Type::Managed(p) | Type::Desc(p) if matches!(p.as_ref(), Type::Record { .. }) && p.is_copy_value())
}
pub(super) fn definitions(
    core: &CoreModule,
    signatures: &BTreeMap<String, Signature>,
) -> R<Vec<ResourceDefinition>> {
    core.aggregates
        .iter()
        .filter(|a| a.kind == AggregateKind::Resource)
        .map(|a| {
            let name = format!("@module{}:{}", core.module_id.0, a.name);
            let payload = Type::Record {
                name: name.clone(),
                fields: a
                    .fields
                    .iter()
                    .map(|f| (f.name.clone(), f.ty.clone()))
                    .collect(),
            };
            if !payload.is_copy_value() {
                return Err(unsupported(
                    a.span,
                    "resource payload requires finite Copy fields",
                ));
            }
            let teardown = a
                .teardown
                .as_ref()
                .map(|name| {
                    signatures.get(name).and_then(|s| s.id).ok_or_else(|| {
                        error("E-DROP-BINDING", a.span, "resource teardown is absent")
                    })
                })
                .transpose()?;
            Ok(ResourceDefinition {
                name,
                payload,
                teardown,
            })
        })
        .collect()
}
impl Builder<'_, '_> {
    pub(super) fn owned_closure(&self, ty: &Type) -> bool {
        matches!(ty, Type::Closure(id) if (self.closures.pending[id.0 as usize].defer_action || self.closures.pending[id.0 as usize].owns_resources()))
    }
    pub(super) fn has_cleanup(&self, ty: &Type) -> bool {
        matches!(ty, Type::Managed(_) | Type::Package { .. })
            || errors::owned(ty)
            || owned_shapes::enumeration(ty)
            || packages::owned_array_supported(ty)
            || self.owned_closure(ty)
    }
    pub(super) fn drop_owned_local(&mut self, local: LocalId, span: Span) -> R<()> {
        let ty = &self.function.locals[local.0 as usize].ty;
        let kind = if matches!(ty, Type::Managed(_)) {
            Some(InstructionKind::DropManaged { local })
        } else if matches!(ty, Type::Package { .. }) {
            Some(InstructionKind::DropPackage { local })
        } else if owned_shapes::enumeration(ty) {
            Some(InstructionKind::DropEnum { local })
        } else if errors::owned(ty) {
            Some(InstructionKind::DropErrorUnion { local })
        } else if packages::owned_array_supported(ty) {
            Some(InstructionKind::DropOwnedArray { local })
        } else if self.owned_closure(ty) {
            Some(InstructionKind::DropClosure { local })
        } else {
            None
        };
        if let Some(kind) = kind {
            self.emit(kind, None, span)?;
        }
        Ok(())
    }
    pub(super) fn resource_give(&mut self, operand: &CoreExpr, span: Span) -> R<(ValueId, Type)> {
        if let Some((local, index, ty)) = self.package_place(operand)? {
            if !matches!(ty, Type::Managed(_)) {
                return Err(error(
                    "E-GIVE-TYPE",
                    span,
                    "give requires a Managed package field",
                ));
            }
            return Ok((
                self.value(
                    InstructionKind::TakePackageField { local, index },
                    ty.clone(),
                    span,
                )?,
                ty,
            ));
        }
        let CoreExprKind::Name(name) = &operand.kind else {
            return Err(unsupported(
                span,
                "give requires a managed local or a direct Managed package field",
            ));
        };
        let local = self
            .lookup(name)
            .ok_or_else(|| error("E-UNKNOWN-NAME", span, format!("unknown variable {name}")))?;
        if let Some((index, ty)) = self.managed_captures.get(&local).cloned() {
            return Ok((
                self.value(InstructionKind::TakeCapture { index }, ty.clone(), span)?,
                ty,
            ));
        }
        let ty = self.function.locals[local.0 as usize].ty.clone();
        match ty {
            Type::Managed(_) => Ok((
                self.value(InstructionKind::TakeLocal { local }, ty.clone(), span)?,
                ty,
            )),
            Type::Desc(_) => Err(error(
                "E-GIVE-DESCRIPTOR",
                span,
                "a descriptor does not own the resource carrier",
            )),
            _ => Err(error(
                "E-GIVE-TYPE",
                span,
                "give requires a managed resource carrier",
            )),
        }
    }
    pub(super) fn stage_argument(
        &mut self,
        value: ValueId,
        ty: &Type,
        span: Span,
    ) -> R<(ValueId, Option<LocalId>)> {
        if self.has_cleanup(ty) {
            let local = self.temp(ty.clone(), span)?;
            self.store(local, value, span)?;
            Ok((value, Some(local)))
        } else {
            Ok((value, None))
        }
    }
    pub(super) fn commit_arguments(
        &mut self,
        staged: Vec<(ValueId, Option<LocalId>)>,
        span: Span,
    ) -> R<Vec<ValueId>> {
        staged
            .into_iter()
            .map(|(value, local)| match local {
                Some(local) => self.value(
                    InstructionKind::TakeLocal { local },
                    self.function.locals[local.0 as usize].ty.clone(),
                    span,
                ),
                None => Ok(value),
            })
            .collect()
    }
    pub(super) fn resource_descriptor(
        &mut self,
        value: ValueId,
        ty: Type,
        span: Span,
    ) -> R<(ValueId, Type)> {
        match ty {
            Type::Managed(payload) => {
                let managed = Type::Managed(payload.clone());
                let local = self.temp(managed, span)?;
                self.store(local, value, span)?;
                let desc = Type::Desc(payload);
                Ok((
                    self.value(InstructionKind::DescribeLocal { local }, desc.clone(), span)?,
                    desc,
                ))
            }
            Type::Desc(_) => Ok((value, ty)),
            _ => Err(error(
                "E-RESOURCE-TYPE",
                span,
                "resource descriptor required",
            )),
        }
    }
    pub(super) fn finish_resource_temps(&mut self, span: Span) -> R<()> {
        self.finish_resource_temps_from(0, span)
    }
    pub(super) fn finish_resource_temps_from(&mut self, first: usize, span: Span) -> R<()> {
        let locals = self.resource_temps.split_off(first);
        for local in locals.into_iter().rev() {
            self.drop_owned_local(local, span)?;
        }
        Ok(())
    }
    pub(super) fn discard_resource(&mut self, value: ValueId, ty: Type, span: Span) -> R<()> {
        if matches!(ty, Type::Managed(_)) {
            self.resource_descriptor(value, ty, span)?;
        } else if matches!(ty, Type::Package { .. })
            || errors::owned(&ty)
            || packages::owned_array_supported(&ty)
            || self.owned_closure(&ty)
        {
            let local = self.temp(ty, span)?;
            self.store(local, value, span)?;
        }
        Ok(())
    }
    pub(super) fn resource_field(
        &mut self,
        object: &CoreExpr,
        member: &str,
        span: Span,
    ) -> R<(ValueId, Type)> {
        let (value, ty) = self.expr(object, None)?;
        let (desc, ty) = self.resource_descriptor(value, ty, span)?;
        let Type::Desc(payload) = ty else {
            unreachable!()
        };
        let (index, field) = aggregates::field(&payload, member, span)?;
        Ok((
            self.value(
                InstructionKind::ReadDesc { desc, index },
                field.clone(),
                span,
            )?,
            field,
        ))
    }
    pub(super) fn resource_assign(&mut self, target: &CoreExpr, value: &CoreExpr) -> R<bool> {
        let CoreExprKind::Member { object, member } = &target.kind else {
            return Ok(false);
        };
        if !matches!(
            self.known_type(object),
            Some(Type::Managed(_) | Type::Desc(_))
        ) {
            return Ok(false);
        }
        // Save the target once before evaluating RHS. Verify checks that its head
        // still has authority after RHS calls or transfers.
        let (desc, ty) = self.expr(object, None)?;
        let (desc, ty) = self.resource_descriptor(desc, ty, target.span)?;
        let Type::Desc(payload) = ty else {
            unreachable!()
        };
        let (index, expected) = aggregates::field(&payload, member, target.span)?;
        let (value, actual) = self.expr(value, Some(&expected))?;
        self.same(&actual, &expected, target.span)?;
        self.emit(
            InstructionKind::WriteDesc { desc, index, value },
            None,
            target.span,
        )?;
        Ok(true)
    }
}
