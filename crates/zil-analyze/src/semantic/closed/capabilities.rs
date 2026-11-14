//! Concrete arguments satisfy original sealed premises through their complete
//! actual environment and execution graph, never a source flag or same shape.
use super::*;
impl Check<'_, '_> {
    pub(super) fn capability_requirement(
        &self,
        context: &SemanticInstance,
        recipe: &SemanticTypeRecipe,
        kind: SemanticBuiltinTrait,
        b: &mut Budget,
    ) -> CompileResult<()> {
        let value = self.recipe_value(context, recipe, b, 0)?;
        self.actual_type(&value.ty, b)?;
        if !value.classes.is_empty() {
            return Err(unknown(
                "actual sealed capability needs its complete nonzero retained and thread relation",
            ));
        }
        match kind {
            SemanticBuiltinTrait::ClosedEnvironment => match value.ty {
                Type::Closure(id) => {
                    if !self.capture_directory(id, b)?.is_empty() {
                        return Err(fail(
                            "actual ClosedEnvironment retains an original captured head",
                        ));
                    }
                    crate::registration::semantic_closed_environment(self.module, id, b)
                }
                _ => self.closed_data(&value.ty, b, 0),
            },
            SemanticBuiltinTrait::ThreadCallable => {
                let Type::Closure(id) = value.ty else {
                    return Err(unknown(
                        "actual ThreadCallable requires its complete finite callable definition and entry",
                    ));
                };
                if !self.capture_directory(id, b)?.is_empty() {
                    return Err(fail(
                        "actual ThreadCallable retains an unproved captured head",
                    ));
                }
                crate::registration::semantic_thread_callable(self.module, id, b)
            }
            SemanticBuiltinTrait::Transfer => {
                self.closed_data(&value.ty, b, 0)?;
                crate::tasks::transfer::semantic_value(
                    self.module,
                    &value.ty,
                    b,
                    zil_base::Span::default(),
                )
            }
            SemanticBuiltinTrait::Share => {
                if value.ty.is_b0_scalar() || value.ty == Type::ForeignHandle {
                    Ok(())
                } else {
                    Err(unknown(
                        "actual Share requires its complete synchronization and concurrent-access capability",
                    ))
                }
            }
            _ => Err(unknown(
                "actual sealed alias or Drop requirement needs its exact original provider and full execution proof",
            )),
        }
    }
    fn closed_data(&self, ty: &Type, b: &mut Budget, depth: usize) -> CompileResult<()> {
        b.spend(1, WORK)?;
        if depth > b.partial_depth {
            return Err(unknown(
                "actual closed data capability exceeds its bounded type depth",
            ));
        }
        if ty.is_b0_scalar() || *ty == Type::ForeignHandle || *ty == Type::ByteBuffer {
            return Ok(());
        }
        match ty {
            Type::Closure(id) => {
                if !self.capture_directory(*id, b)?.is_empty() {
                    return Err(fail("actual closed data retains an original captured head"));
                }
                crate::registration::semantic_closed_environment(self.module, *id, b)
            }
            Type::Managed(inner)
            | Type::OwnedStore { element: inner }
            | Type::Array { element: inner, .. } => self.closed_data(inner, b, depth + 1),
            Type::Tuple(fields) => {
                for field in fields {
                    self.closed_data(field, b, depth + 1)?;
                }
                Ok(())
            }
            Type::Record { fields, .. } | Type::Package { fields, .. } => {
                self.nominal_inventory(ty, b)?;
                for (_, field) in fields {
                    self.closed_data(field, b, depth + 1)?;
                }
                Ok(())
            }
            Type::Enum { variants, .. } => {
                self.nominal_inventory(ty, b)?;
                for (_, fields) in variants {
                    for field in fields {
                        self.closed_data(field, b, depth + 1)?;
                    }
                }
                Ok(())
            }
            Type::ErrorUnion { error, result } => {
                self.closed_data(error, b, depth + 1)?;
                self.closed_data(result, b, depth + 1)
            }
            Type::BorrowedStorage { storage, life } => {
                self.nominal_inventory(ty, b)?;
                if !empty(life) {
                    return Err(fail(
                        "actual closed data retains original nominal or phantom classes",
                    ));
                }
                match storage.as_ref() {
                    Type::Record { fields, .. } | Type::Package { fields, .. } => {
                        for (_, field) in fields {
                            self.closed_data(field, b, depth + 1)?;
                        }
                        Ok(())
                    }
                    Type::Enum { variants, .. } => {
                        for (_, fields) in variants {
                            for field in fields {
                                self.closed_data(field, b, depth + 1)?;
                            }
                        }
                        Ok(())
                    }
                    _ => Err(unknown(
                        "actual closed qualified data needs its complete intrinsic inventory",
                    )),
                }
            }
            Type::FunctionPointer { .. } | Type::ForeignCallback { .. } => Ok(()),
            _ => Err(unknown(
                "actual closed data retains an external borrow, asynchronous owner or unknown inventory",
            )),
        }
    }
}
