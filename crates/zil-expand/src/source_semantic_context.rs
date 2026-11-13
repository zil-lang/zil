//! Explicit opaque Context syntax is a complete upper shape, never a value.
use super::*;
impl Builder<'_, '_, '_, '_> {
    pub(super) fn explicit_dyn_context(
        &mut self,
        context: &s::TypeRef,
        application: &SemanticTraitRecipe,
        path: &mut Vec<u32>,
        depth: usize,
    ) -> R<SemanticDynContextRecipe> {
        self.budget.node(context.span, depth)?;
        if let s::TypeKind::Group(inner) = &context.kind {
            return self.explicit_dyn_context(inner, application, path, depth + 1);
        }
        let s::TypeKind::Named { path: names, args } = &context.kind else {
            return Ok(SemanticDynContextRecipe::Type(Box::new(self.ty(
                context,
                path,
                depth + 1,
                false,
            )?)));
        };
        if !matches!(names.as_slice(), [name] if matches!(name.as_str(), "Ctx" | "Context"))
            || self.builtin_shadow(names, context.span)?
        {
            return Ok(SemanticDynContextRecipe::Type(Box::new(self.ty(
                context,
                path,
                depth + 1,
                false,
            )?)));
        }
        let mut domain = Vec::new();
        let mut ports = Vec::new();
        for (ordinal, arg) in args.iter().enumerate() {
            self.budget.node(context.span, depth + 1)?;
            path.push(
                u32::try_from(ordinal)
                    .map_err(|_| error(context.span, "Context argument ordinal overflow"))?,
            );
            let result = (|| {
                match arg {
                    s::GenericArg::Lifetime(name) => {
                        let class = self.life(Some(name), path, context.span)?;
                        self.context_class(&mut domain, class, context.span, depth + 1)?;
                    }
                    s::GenericArg::Type(ty) => {
                        let recipe = self.ty(ty, path, depth + 1, false)?;
                        if let SemanticTypeRecipe::Address {
                            mutable,
                            lifetime: SemanticOuterLifeRecipe::Class(class),
                            ..
                        } = &recipe
                        {
                            prepay_recipe(class, self.budget, ty.span, depth + 1)?;
                            let class = class.clone();
                            self.context_class(&mut domain, class.clone(), ty.span, depth + 1)?;
                            let mode = if *mutable {
                                StoredBorrowMode::ExclusiveAddress
                            } else {
                                StoredBorrowMode::SharedAddress
                            };
                            self.push(
                                &mut ports,
                                SemanticPortPart::One {
                                    leaf: Box::new(recipe),
                                    mode,
                                    class,
                                },
                                ty.span,
                            )?;
                        } else {
                            // A whole input type keeps its complete intrinsic domain,
                            // including phantom classes and duplicate actual images.
                            prepay_recipe(&recipe, self.budget, ty.span, depth + 1)?;
                            let classes = SemanticClassSequenceRecipe {
                                parts: vec![SemanticClassSequencePart::AllClasses {
                                    ty: Box::new(recipe.clone()),
                                    path: Vec::new(),
                                }],
                            };
                            prepay_recipe(&classes, self.budget, ty.span, depth + 1)?;
                            self.push(&mut domain, classes.parts[0].clone(), ty.span)?;
                            self.push(
                                &mut ports,
                                SemanticPortPart::FromType {
                                    ty: Box::new(recipe),
                                    classes,
                                },
                                ty.span,
                            )?;
                        }
                    }
                    _ => {
                        return Err(error(
                            context.span,
                            "Context upper accepts complete Type or Lifetime operands",
                        ));
                    }
                }
                Ok(())
            })();
            path.pop();
            result?;
        }
        let mut nominal = Vec::new();
        let mut retained = Vec::new();
        for (ordinal, actual) in application.arguments.iter().enumerate() {
            let ordinal = u32::try_from(ordinal)
                .map_err(|_| error(context.span, "trait argument ordinal overflow"))?;
            match actual {
                SemanticActualRecipe::Life(class) => {
                    prepay_recipe(class, self.budget, context.span, depth + 1)?;
                    self.push(
                        &mut nominal,
                        SemanticNominalPart::One {
                            path: Vec::new(),
                            declaration: application.declaration,
                            argument: ordinal,
                            class: class.clone(),
                        },
                        context.span,
                    )?;
                }
                SemanticActualRecipe::Type(ty) => {
                    prepay_recipe(ty, self.budget, context.span, depth + 1)?;
                    self.push(
                        &mut retained,
                        SemanticRetainedPart::All {
                            path: vec![LifeTypeProjection::TypeArgument(ordinal)],
                            parents: SemanticClassSequenceRecipe {
                                parts: vec![SemanticClassSequencePart::AllClasses {
                                    ty: Box::new(ty.clone()),
                                    path: Vec::new(),
                                }],
                            },
                        },
                        context.span,
                    )?;
                }
                SemanticActualRecipe::Const { .. } => {}
                SemanticActualRecipe::Infer => {
                    return Err(error(
                        context.span,
                        "trait Context application contains an unresolved actual",
                    ));
                }
            }
        }
        Ok(SemanticDynContextRecipe::Shape(Box::new(
            SemanticRetainedShapeRecipe {
                domain: SemanticClassSequenceRecipe { parts: domain },
                ports,
                retained,
                nominal,
            },
        )))
    }
    fn context_class(
        &mut self,
        domain: &mut Vec<SemanticClassSequencePart>,
        class: SemanticLifeRecipe,
        span: Span,
        depth: usize,
    ) -> R<()> {
        self.budget.identity_work(domain.len(), span)?;
        prepay_recipe(&class, self.budget, span, depth)?;
        for prior in domain.iter() {
            prepay_recipe_work(prior, self.budget, span, depth)?;
        }
        let part = SemanticClassSequencePart::One(class);
        if !domain.contains(&part) {
            self.push(domain, part, span)?;
        }
        Ok(())
    }
}
