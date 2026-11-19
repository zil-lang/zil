//! Constructor coverage over independently checked original type recipes.
use super::*;

impl Check<'_, '_> {
    pub(super) fn exhaustive(
        &mut self,
        owner: SemanticDefinitionId,
        ty: &SemanticTypeRecipe,
        arms: &[SemanticOriginalMatchArm],
        depth: usize,
    ) -> R<()> {
        self.pay(arms.len(), depth)?;
        if arms
            .iter()
            .any(|arm| original_pattern_any(Some(&arm.pattern)))
        {
            return Ok(());
        }
        self.pay(arms.len().saturating_mul(2), depth)?;
        let rows = arms
            .iter()
            .map(|arm| vec![Some(&arm.pattern)])
            .collect::<Vec<_>>();
        if !self.covers_patterns(owner, &rows, &[ty], depth + 1)? {
            return Err(Diagnostic::error(
                Phase::Types,
                "E-MATCH-NONEXHAUSTIVE",
                arms.first().map(|arm| arm.pattern.span),
                "match does not cover every possible value",
            ));
        }
        Ok(())
    }

    fn covers_patterns(
        &mut self,
        owner: SemanticDefinitionId,
        rows: &[OriginalPatternRow<'_>],
        types: &[&SemanticTypeRecipe],
        depth: usize,
    ) -> R<bool> {
        self.pay(
            rows.len()
                .saturating_mul(types.len().saturating_add(1))
                .saturating_add(1),
            depth,
        )?;
        if rows.is_empty() {
            return Ok(false);
        }
        if rows
            .iter()
            .any(|row| row.iter().copied().all(original_pattern_any))
        {
            return Ok(true);
        }
        let Some((ty, tail)) = types.split_first() else {
            return Err(bad("original pattern matrix has inconsistent column arity"));
        };
        if rows.iter().all(|row| original_pattern_any(row[0])) {
            return self.pattern_branch(
                owner,
                rows,
                OriginalPatternHead::Default,
                &[],
                tail,
                depth,
            );
        }
        match self.logical(owner, ty, depth + 1)? {
            SemanticTypeRecipe::Atom(Type::Bool) => {
                for value in [false, true] {
                    if !self.pattern_branch(
                        owner,
                        rows,
                        OriginalPatternHead::Bool(value),
                        &[],
                        tail,
                        depth,
                    )? {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
            SemanticTypeRecipe::Atom(Type::Unit) => {
                self.pattern_branch(owner, rows, OriginalPatternHead::Tuple, &[], tail, depth)
            }
            SemanticTypeRecipe::Tuple(fields) => self.pattern_branch(
                owner,
                rows,
                OriginalPatternHead::Tuple,
                &fields,
                tail,
                depth,
            ),
            SemanticTypeRecipe::Address { inner, .. } => self.pattern_branch(
                owner,
                rows,
                OriginalPatternHead::Reference,
                std::slice::from_ref(inner.as_ref()),
                tail,
                depth,
            ),
            SemanticTypeRecipe::ErrorUnion { error, result } => {
                for (is_error, payload) in [(false, result), (true, error)] {
                    if !self.pattern_branch(
                        owner,
                        rows,
                        OriginalPatternHead::ErrorArm(is_error),
                        std::slice::from_ref(payload.as_ref()),
                        tail,
                        depth,
                    )? {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
            SemanticTypeRecipe::Nominal {
                definition,
                arguments,
            } => {
                let declaration = self.definition(definition)?;
                match &declaration.body {
                    SemanticDefinitionBody::Nominal {
                        body: SemanticNominalBody::Enum { variants },
                        ..
                    } => {
                        for variant in variants {
                            self.pay(variant.fields.len().saturating_add(1), depth)?;
                            let fields = variant
                                .fields
                                .iter()
                                .map(|field| {
                                    self.instantiate_field(
                                        owner,
                                        definition,
                                        field,
                                        &arguments,
                                        depth + 1,
                                    )
                                })
                                .collect::<R<Vec<_>>>()?;
                            if !self.pattern_branch(
                                owner,
                                rows,
                                OriginalPatternHead::Variant(&variant.name),
                                &fields,
                                tail,
                                depth,
                            )? {
                                return Ok(false);
                            }
                        }
                        Ok(!variants.is_empty())
                    }
                    SemanticDefinitionBody::Nominal {
                        body: SemanticNominalBody::Product { fields },
                        ..
                    } => {
                        self.pay(fields.len().saturating_mul(2), depth)?;
                        let names = fields
                            .iter()
                            .map(|field| field.name.as_str())
                            .collect::<Vec<_>>();
                        let payload = fields
                            .iter()
                            .map(|field| {
                                self.instantiate_field(
                                    owner,
                                    definition,
                                    &field.ty,
                                    &arguments,
                                    depth + 1,
                                )
                            })
                            .collect::<R<Vec<_>>>()?;
                        self.pattern_branch(
                            owner,
                            rows,
                            OriginalPatternHead::Record(&names),
                            &payload,
                            tail,
                            depth,
                        )
                    }
                    _ => Err(bad(
                        "original pattern matrix loses its exact nominal declaration",
                    )),
                }
            }
            // Literals in open or unresolved domains cannot prove coverage
            // of the rest of that column. Default rows can still cover it.
            _ => self.pattern_branch(owner, rows, OriginalPatternHead::Default, &[], tail, depth),
        }
    }

    fn pattern_branch(
        &mut self,
        owner: SemanticDefinitionId,
        rows: &[OriginalPatternRow<'_>],
        head: OriginalPatternHead<'_>,
        fields: &[SemanticTypeRecipe],
        tail: &[&SemanticTypeRecipe],
        depth: usize,
    ) -> R<bool> {
        let width = fields.len().saturating_add(tail.len());
        let mut work = width.saturating_add(rows.len().saturating_mul(width.saturating_add(1)));
        for row in rows {
            match (&head, row[0].map(|pattern| &pattern.kind)) {
                (
                    OriginalPatternHead::Variant(expected),
                    Some(
                        CorePatternKind::Variant { variant, .. }
                        | CorePatternKind::Constructor { variant, .. },
                    ),
                ) => work = work.saturating_add(expected.len().saturating_add(variant.len())),
                (
                    OriginalPatternHead::Record(labels),
                    Some(
                        CorePatternKind::Record { fields, .. }
                        | CorePatternKind::RecordConstructor { fields, .. },
                    ),
                ) => {
                    let names = fields.iter().fold(0usize, |work, (name, _)| {
                        work.saturating_add(name.len().saturating_add(1))
                    });
                    let expected = labels.iter().fold(0usize, |work, label| {
                        work.saturating_add(label.len().saturating_add(1))
                    });
                    work = work
                        .saturating_add(names.saturating_mul(labels.len()))
                        .saturating_add(expected.saturating_mul(fields.len()));
                }
                _ => {}
            }
        }
        self.pay(work, depth)?;
        let specialized = specialize_original_patterns(rows, head, fields.len());
        let next = fields
            .iter()
            .chain(tail.iter().copied())
            .collect::<Vec<_>>();
        self.covers_patterns(owner, &specialized, &next, depth + 1)
    }
}
