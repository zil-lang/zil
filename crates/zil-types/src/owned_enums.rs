//! Consuming match observes a complete hidden owner before any decomposition.
use super::*;
type CopyBindings = Vec<(String, ValueId, Type, Span)>;
type Leaves = Vec<(Option<String>, LocalId, Type, Span)>;
impl Builder<'_, '_> {
    pub(super) fn observe_owned_pattern(
        &mut self,
        pattern: &CorePattern,
        root: LocalId,
        path: &mut Vec<VariantProjection>,
        ty: &Type,
        fail: BlockId,
        copies: &mut CopyBindings,
    ) -> R<()> {
        self.closures
            .snapshot(path.len().saturating_add(1), pattern.span)?;
        match &pattern.kind {
            CorePatternKind::Name(_) | CorePatternKind::Wildcard => Ok(()),
            CorePatternKind::Variant {
                variant, fields, ..
            } => {
                let Type::Enum { variants, .. } = ty else {
                    return Err(unsupported(pattern.span, "owning pattern requires an Enum"));
                };
                let (index, (_, types)) = variants
                    .iter()
                    .enumerate()
                    .find(|(_, (name, _))| name == variant)
                    .expect("covered variant");
                let tag = self.value(
                    InstructionKind::VariantTagLocal {
                        local: root,
                        path: path.clone(),
                    },
                    Type::I32,
                    pattern.span,
                )?;
                self.pattern_branch(tag, Constant::I32(index as i32), fail, pattern.span)?;
                for (field, (pattern, ty)) in fields.iter().zip(types).enumerate() {
                    self.closures
                        .snapshot(path.len().saturating_mul(2).saturating_add(2), pattern.span)?;
                    path.push(VariantProjection {
                        variant: index as u32,
                        index: field as u32,
                    });
                    if ty.is_copy_value() {
                        let work = inference::type_snapshot_work(ty, pattern.span, self.limits)?;
                        self.closures.snapshot(work, pattern.span)?;
                        let value = self.value(
                            InstructionKind::ReadVariantLocal {
                                local: root,
                                path: path.clone(),
                            },
                            ty.clone(),
                            pattern.span,
                        )?;
                        self.pattern_test(pattern, value, ty, fail, copies)?;
                    } else {
                        self.observe_owned_pattern(pattern, root, path, ty, fail, copies)?;
                    }
                    path.pop();
                }
                Ok(())
            }
            _ => Err(unsupported(
                pattern.span,
                "owning payload pattern must bind, discard or select a variant",
            )),
        }
    }
    pub(super) fn unpack_owned_pattern(
        &mut self,
        pattern: &CorePattern,
        local: LocalId,
        ty: &Type,
        leaves: &mut Leaves,
    ) -> R<()> {
        let work = inference::type_snapshot_work(ty, pattern.span, self.limits)?;
        self.closures
            .snapshot(work.saturating_mul(3), pattern.span)?;
        match &pattern.kind {
            CorePatternKind::Name(name) => {
                self.closures
                    .snapshot(name.len().saturating_add(1), pattern.span)?;
                leaves.push((Some(name.clone()), local, ty.clone(), pattern.span));
                Ok(())
            }
            CorePatternKind::Wildcard => {
                leaves.push((None, local, ty.clone(), pattern.span));
                Ok(())
            }
            CorePatternKind::Variant {
                variant, fields, ..
            } => {
                let Type::Enum { variants, .. } = ty else {
                    unreachable!("checked pattern")
                };
                let (variant, (_, types)) = variants
                    .iter()
                    .enumerate()
                    .find(|(_, (name, _))| name == variant)
                    .expect("checked variant");
                let mut destinations = Vec::with_capacity(types.len());
                for ty in types {
                    destinations.push(self.temp(ty.clone(), pattern.span)?);
                }
                // The original nested path selected the arm, but this new local
                // has its own activation. Re-establish its own exact tag edge.
                let tag = self.value(
                    InstructionKind::VariantTagLocal {
                        local,
                        path: vec![],
                    },
                    Type::I32,
                    pattern.span,
                )?;
                let impossible = self.block_id()?;
                self.pattern_branch(tag, Constant::I32(variant as i32), impossible, pattern.span)?;
                self.function.blocks[impossible.0 as usize].terminator = Terminator::Trap;
                self.closures.snapshot(destinations.len(), pattern.span)?;
                self.emit(
                    InstructionKind::UnpackVariant {
                        local,
                        variant: variant as u32,
                        fields: destinations.clone(),
                    },
                    None,
                    pattern.span,
                )?;
                for ((pattern, ty), local) in fields.iter().zip(types).zip(destinations) {
                    if !ty.is_copy_value() {
                        self.unpack_owned_pattern(pattern, local, ty, leaves)?;
                    }
                }
                Ok(())
            }
            _ => Err(unsupported(pattern.span, "invalid owning payload pattern")),
        }
    }
    pub(super) fn owned_enum_match(
        &mut self,
        expression: &CoreExpr,
        arms: &[CoreMatchArm],
        span: Span,
    ) -> R<()> {
        let (value, ty) = self.transfer_expr(expression, None)?;
        let work = inference::type_snapshot_work(&ty, span, self.limits)?;
        self.closures
            .snapshot(work.saturating_mul(3).saturating_add(arms.len()), span)?;
        // End the scrutinee's full-expression preparation before the hidden
        // match owner is registered. These older temporaries can still own
        // resource headers used only to compute Copy payload expressions.
        self.finish_resource_temps(expression.span)?;
        let hidden = self.temp(ty.clone(), expression.span)?;
        self.store(hidden, value, expression.span)?;
        self.check_match_coverage(&ty, arms, span)?;
        let mut ends = Vec::new();
        for arm in arms {
            let fail = self.block_id()?;
            let mut copies = Vec::new();
            self.observe_owned_pattern(
                &arm.pattern,
                hidden,
                &mut Vec::new(),
                &ty,
                fail,
                &mut copies,
            )?;
            self.push_scope(arm.span)?;
            let mut leaves = Vec::new();
            self.unpack_owned_pattern(&arm.pattern, hidden, &ty, &mut leaves)?;
            // Move every final leaf into SSA before empty temporary shells
            // are closed. A retained Sum wrapper is older than the payload
            // locals; publishing first would make its cleanup out of order.
            self.closures.snapshot(leaves.len(), arm.span)?;
            let mut publication = Vec::with_capacity(leaves.len());
            for (name, source, ty, span) in leaves.into_iter().rev() {
                let work = inference::type_snapshot_work(&ty, span, self.limits)?;
                self.closures.snapshot(work, span)?;
                let value = self.value(
                    InstructionKind::TakeLocal { local: source },
                    ty.clone(),
                    span,
                )?;
                publication.push((name, value, ty, span));
            }
            self.finish_resource_temps(arm.span)?;
            // The same reverse DFS order now establishes actual new ages.
            // No user callback, recoverable failure or early exit intervenes.
            for (name, value, ty, span) in publication {
                let name =
                    name.unwrap_or_else(|| format!("$discard240${}", self.function.locals.len()));
                let local = self.local(&name, ty, false, span)?;
                self.store(local, value, span)?;
            }
            for (name, value, ty, span) in copies {
                let local = self.local(&name, ty, false, span)?;
                self.store(local, value, span)?;
            }
            self.block(&arm.body)?;
            if self.current.is_some() {
                self.end_scopes(self.scopes.len() - 1, arm.span)?;
            }
            if let Some(end) = self.current.take() {
                ends.push(end);
            }
            self.pop_scope();
            self.current = Some(fail);
        }
        self.terminate(Terminator::Trap);
        if !ends.is_empty() {
            let join = self.block_id()?;
            for end in ends {
                self.function.blocks[end.0 as usize].terminator = Terminator::Jump(join);
            }
            self.current = Some(join);
        }
        Ok(())
    }
}
