//! Associated names retain their actual lexical receiver and declaring trait.
//! Looking up a source member does not discharge its bound or choose a provider.
use super::*;
impl Builder<'_, '_, '_, '_> {
    fn projection_member(
        &mut self,
        receiver: &SemanticTypeRecipe,
        bound: &s::TypeRef,
        member: &str,
        path: &mut Vec<u32>,
        depth: usize,
    ) -> R<Option<SemanticTypeRecipe>> {
        self.budget.node(bound.span, depth)?;
        let mut bound = bound;
        while let s::TypeKind::Group(inner) = &bound.kind {
            bound = inner;
            self.budget.node(bound.span, depth)?;
        }
        let s::TypeKind::Named { path: names, .. } = &bound.kind else {
            return Err(unsupported(
                bound.span,
                "associated receiver bound requires its complete original trait application",
            ));
        };
        if self.builtin_trait(bound)?.is_some()
            || matches!(names.as_slice(), [name] if matches!(name.as_str(), "Fn" | "FnMut" | "FnOnce"))
        {
            return Ok(None);
        }
        let row = self.named(names, bound.span)?;
        let inventory = self.inventory;
        let s::ItemKind::Trait(declaration) = &inventory.declarations[row].item.kind else {
            return Err(error(
                bound.span,
                "associated member bound does not name an original trait",
            ));
        };
        let mut selected = None;
        for (ordinal, item) in declaration.members.iter().enumerate() {
            self.budget.node(item.span, depth)?;
            if let s::ItemKind::TypeAlias(alias) = &item.kind {
                self.budget.text(&alias.name, item.span)?;
                self.budget.text(member, item.span)?;
                if alias.name == member {
                    if !alias.generics.is_empty() {
                        return Err(unsupported(
                            item.span,
                            "associated type member actuals must remain in their original generic scope",
                        ));
                    }
                    if selected.replace(ordinal).is_some() {
                        return Err(error(
                            item.span,
                            "original associated type member is ambiguous",
                        ));
                    }
                }
            }
        }
        let Some(member) = selected else {
            return Ok(None);
        };
        let application = self.application(bound, path, depth + 1)?;
        prepay_recipe(receiver, self.budget, bound.span, depth + 1)?;
        Ok(Some(SemanticTypeRecipe::AssociatedType {
            receiver: Box::new(receiver.clone()),
            application: Box::new(application),
            member: u32::try_from(member)
                .map_err(|_| error(bound.span, "associated member ordinal overflow"))?,
        }))
    }
    pub(super) fn projected_type(
        &mut self,
        names: &[String],
        arguments: &[s::GenericArg],
        path: &mut Vec<u32>,
        depth: usize,
        span: Span,
    ) -> R<Option<SemanticTypeRecipe>> {
        let [base, member] = names else {
            return Ok(None);
        };
        let formal = if base == "Self" {
            None
        } else {
            self.formal(base, 0, span)?
        };
        if formal.is_none() && base != "Self" {
            return Ok(None);
        }
        if !arguments.is_empty() {
            return Err(unsupported(
                span,
                "associated type application requires its complete original member-argument declaration",
            ));
        }
        let inventory = self.inventory;
        let saved_owner = self.owner;
        let saved_offset = self.lexical_offset;
        let saved_invocations = std::mem::take(&mut self.invocations);
        let implicit_before = self.implicit.len();
        let output = (|| {
            let mut candidates = Vec::new();
            if let Some(reference) = formal {
                let SemanticScopeRef::Ancestor(distance) = reference.scope;
                let steps = distance.checked_sub(saved_offset).ok_or_else(|| {
                    error(
                        span,
                        "associated formal precedes the actual lexical definition",
                    )
                })?;
                let mut owner = saved_owner;
                for _ in 0..steps {
                    owner = inventory.declarations[owner]
                        .parent
                        .ok_or_else(|| error(span, "associated formal lexical owner is absent"))?;
                }
                let s::GenericParamKind::Type(bounds) =
                    &inventory.declarations[owner].parameters()[reference.ordinal as usize].kind
                else {
                    return Err(error(span, "associated receiver formal is not a Type"));
                };
                let receiver = SemanticTypeRecipe::Formal(reference);
                self.owner = owner;
                self.lexical_offset = distance;
                for bound in bounds {
                    if let Some(candidate) =
                        self.projection_member(&receiver, bound, member, path, depth + 1)?
                    {
                        self.push(&mut candidates, candidate, bound.span)?;
                    }
                }
                let clauses = match &inventory.declarations[owner].item.kind {
                    s::ItemKind::Function(value)
                    | s::ItemKind::Extern {
                        function: value, ..
                    } => value.where_clause.as_slice(),
                    s::ItemKind::Impl(value) => value.where_clause.as_slice(),
                    _ => &[],
                };
                for clause in clauses {
                    self.budget.node(clause.span, depth)?;
                    if let s::BoundSubject::Type(subject) = &clause.subject
                        && matches!(&subject.kind, s::TypeKind::Named { path, args } if path.as_slice() == [base.as_str()] && args.is_empty())
                    {
                        for bound in &clause.bounds {
                            if let Some(candidate) =
                                self.projection_member(&receiver, bound, member, path, depth + 1)?
                            {
                                self.push(&mut candidates, candidate, bound.span)?;
                            }
                        }
                    }
                }
            } else {
                let mut current = Some(saved_owner);
                let mut distance = saved_offset;
                let mut found = None;
                while let Some(owner) = current {
                    let row = &inventory.declarations[owner];
                    self.budget.node(row.item.span, depth)?;
                    if let s::ItemKind::Impl(value) = &row.item.kind {
                        if let Some(bound) = &value.trait_type {
                            found = Some((owner, distance, bound));
                            break;
                        }
                    }
                    if let s::ItemKind::Trait(value) = &row.item.kind {
                        let receiver = SemanticTypeRecipe::SelfType {
                            scope: SemanticScopeRef::Ancestor(distance),
                        };
                        let mut member_ordinal = None;
                        for (ordinal, item) in value.members.iter().enumerate() {
                            self.budget.node(item.span, depth)?;
                            if matches!(&item.kind, s::ItemKind::TypeAlias(alias) if alias.name == *member)
                            {
                                if member_ordinal.replace(ordinal).is_some() {
                                    return Err(error(
                                        span,
                                        "original Self associated member is ambiguous",
                                    ));
                                }
                            }
                        }
                        if let Some(ordinal) = member_ordinal {
                            let mut actuals = Vec::new();
                            for (ordinal, parameter) in value.generics.iter().enumerate() {
                                let reference = SemanticFormalRef {
                                    scope: SemanticScopeRef::Ancestor(distance),
                                    ordinal: ordinal as u32,
                                };
                                let actual = match &parameter.kind {
                                    s::GenericParamKind::Type(_) => SemanticActualRecipe::Type(
                                        SemanticTypeRecipe::Formal(reference),
                                    ),
                                    s::GenericParamKind::Lifetime => SemanticActualRecipe::Life(
                                        SemanticLifeRecipe::Formal(reference),
                                    ),
                                    s::GenericParamKind::Const(carrier) => {
                                        self.owner = owner;
                                        self.lexical_offset = distance;
                                        SemanticActualRecipe::Const {
                                            carrier: self.ty(carrier, path, depth + 1, false)?,
                                            value: SemanticConstRecipe::Formal(reference),
                                        }
                                    }
                                };
                                self.push(&mut actuals, actual, parameter.span)?;
                            }
                            return Ok(Some(SemanticTypeRecipe::AssociatedType {
                                receiver: Box::new(receiver),
                                application: Box::new(SemanticTraitRecipe {
                                    declaration: LifeDeclarationId {
                                        module: row.module,
                                        declaration: row.declaration,
                                        compiler: false,
                                    },
                                    arguments: actuals,
                                }),
                                member: ordinal as u32,
                            }));
                        }
                        break;
                    }
                    current = row.parent;
                    distance = distance
                        .checked_add(1)
                        .ok_or_else(|| error(span, "Self associated lexical depth overflow"))?;
                }
                if let Some((owner, distance, bound)) = found {
                    let receiver = SemanticTypeRecipe::SelfType {
                        scope: SemanticScopeRef::Ancestor(distance),
                    };
                    self.owner = owner;
                    self.lexical_offset = distance;
                    if let Some(candidate) =
                        self.projection_member(&receiver, bound, member, path, depth + 1)?
                    {
                        self.push(&mut candidates, candidate, bound.span)?;
                    }
                }
            }
            if self.implicit.len() != implicit_before {
                return Err(unsupported(
                    span,
                    "associated bound elision needs its original owner occurrence relation",
                ));
            }
            let mut candidates = candidates.into_iter();
            let Some(candidate) = candidates.next() else {
                return Err(unsupported(
                    span,
                    "associated type has no unique actual declaring trait in its original bounds",
                ));
            };
            for other in candidates {
                prepay_recipe(&candidate, self.budget, span, depth)?;
                prepay_recipe(&other, self.budget, span, depth)?;
                if other != candidate {
                    return Err(error(
                        span,
                        "associated type is ambiguous across original trait applications",
                    ));
                }
            }
            Ok(Some(candidate))
        })();
        self.owner = saved_owner;
        self.lexical_offset = saved_offset;
        self.invocations = saved_invocations;
        output
    }
}
