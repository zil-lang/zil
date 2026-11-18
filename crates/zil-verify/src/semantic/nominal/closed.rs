//! A closed nominal body consisting of original scalar fields and whole type
//! formals has one physical result per ordered list of complete type actuals.
//! Requirements on those formals are checked anew in the calling environment.
//! This directory stores only the physical result of a completed inventory
//! search. A lifetime, origin, lexical parent or scope witness stays on the
//! ordinary nominal path.
use super::*;
use std::rc::Rc;

#[derive(Default)]
pub(in crate::semantic) struct ClosedNominals {
    rows: BTreeMap<(SemanticDefinitionId, bool, usize), Vec<Choice>>,
}

struct Choice {
    arguments: Vec<Rc<CheckedTypeData>>,
    result: Rc<CheckedTypeData>,
}

fn empty_domain(value: &CheckedType) -> bool {
    value.life == empty_life()
        && value.classes.is_empty()
        && value.origins.is_empty()
        && value.statics.is_empty()
}

impl ClosedNominals {
    pub(super) fn get(
        &self,
        key: (SemanticDefinitionId, bool, usize),
        actuals: &[CheckedActual],
        b: &mut Budget,
    ) -> Check<Option<CheckedType>> {
        b.spend(
            self.rows
                .len()
                .checked_ilog2()
                .map_or(1, |n| n as usize + 2),
        )?;
        let Some(rows) = self.rows.get(&key) else {
            return Ok(None);
        };
        'choices: for row in rows {
            b.spend(1)?;
            if row.arguments.len() != actuals.len() {
                continue;
            }
            for (known, actual) in row.arguments.iter().zip(actuals) {
                b.spend(1)?;
                let CheckedActual::Type(actual) = actual else {
                    continue 'choices;
                };
                if !empty_domain(actual) {
                    continue 'choices;
                }
                if !Rc::ptr_eq(known, &actual.type_cell) {
                    dynamic_metadata::ty(&known.ty, b)?;
                    dynamic_metadata::ty(&actual.ty, b)?;
                    if known.ty != actual.ty {
                        continue 'choices;
                    }
                }
            }
            return Ok(Some(CheckedType {
                type_cell: row.result.clone(),
                life: empty_life(),
                classes: vec![],
                origins: vec![],
                statics: vec![],
            }));
        }
        Ok(None)
    }

    pub(super) fn remember(
        &mut self,
        key: (SemanticDefinitionId, bool, usize),
        definition: &SemanticDefinition,
        actuals: &[CheckedActual],
        body: &SemanticNominalBody,
        inventory: &[&LifeNominalInstance],
        result: &CheckedType,
        b: &mut Budget,
    ) -> Check<()> {
        if inventory.is_empty()
            || definition.parent.is_some()
            || !definition.environment.is_empty()
            || !definition.implicit_lifetimes.is_empty()
            || !empty_domain(result)
            || !type_storage::closed_storage(&result.ty, b)?
        {
            return Ok(());
        }
        // These recipes read only this invocation's checked type arguments.
        // Complex child constructors can introduce their own obligations.
        let field = |ty: &SemanticTypeRecipe| {
            matches!(ty, SemanticTypeRecipe::Atom(t) if t.is_b0_scalar())
                || matches!(
                    ty,
                    SemanticTypeRecipe::Formal(SemanticFormalRef {
                        scope: SemanticScopeRef::Ancestor(0),
                        ..
                    })
                )
        };
        match body {
            SemanticNominalBody::Product { fields } => {
                b.spend(fields.len())?;
                if !fields.iter().all(|f| field(&f.ty)) {
                    return Ok(());
                }
            }
            SemanticNominalBody::Enum { variants } => {
                b.spend(variants.len())?;
                for v in variants {
                    b.spend(v.fields.len())?;
                    if !v.fields.iter().all(field) {
                        return Ok(());
                    }
                }
            }
        }
        // Every alternative must be independent of caller equations, including
        // alternatives that failed to match on the first successful lookup.
        b.spend(inventory.len())?;
        for row in inventory {
            if !type_storage::closed_storage(&row.ty, b)? {
                return Ok(());
            }
            b.spend(row.arguments.len())?;
            for a in &row.arguments {
                let LifeNominalActual::Type(ty) = a else {
                    return Ok(());
                };
                if !type_storage::closed_storage(ty, b)? {
                    return Ok(());
                }
            }
        }
        b.spend(actuals.len())?;
        let mut arguments = Vec::with_capacity(actuals.len());
        for a in actuals {
            let CheckedActual::Type(t) = a else {
                return Ok(());
            };
            if !empty_domain(t) || !type_storage::closed_storage(&t.ty, b)? {
                return Ok(());
            }
            arguments.push(t.type_cell.clone());
        }
        b.spend(
            self.rows
                .len()
                .checked_ilog2()
                .map_or(2, |n| n as usize + 3),
        )?;
        self.rows.entry(key).or_default().push(Choice {
            arguments,
            result: result.type_cell.clone(),
        });
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn physical_choice_preserves_mixed_order_declaration_depth_and_empty_domain() {
        let limits = Limits::default();
        let (module, _) = zil_ir::wire::decode_module_candidate(
            include_bytes!("../fixtures/ordinary-identity-complete-head.wire"),
            &limits,
        )
        .unwrap();
        let mut definition = module.semantic.as_ref().unwrap().definitions[0].clone();
        definition.parent = None;
        definition.environment.clear();
        definition.implicit_lifetimes.clear();
        let a = |ty| CheckedType {
            type_cell: CheckedTypeData::new(ty),
            life: empty_life(),
            origins: vec![],
            classes: vec![],
            statics: vec![],
        };
        let args = [
            CheckedActual::Type(a(Type::I32)),
            CheckedActual::Type(a(Type::U32)),
        ];
        let body = SemanticNominalBody::Product {
            fields: vec![SemanticFieldRecipe {
                name: "right".into(),
                ty: SemanticTypeRecipe::Formal(SemanticFormalRef {
                    scope: SemanticScopeRef::Ancestor(0),
                    ordinal: 1,
                }),
            }],
        };
        let result = a(Type::Record {
            name: "Pair".into(),
            fields: vec![("right".into(), Type::U32)],
        });
        let inventory = LifeNominalInstance {
            declaration: LifeDeclarationId {
                module: zil_base::ModuleId(0),
                declaration: zil_base::NodeId(15),
                compiler: false,
            },
            ty: result.ty.clone(),
            arguments: vec![
                LifeNominalActual::Type(Type::I32),
                LifeNominalActual::Type(Type::U32),
            ],
        };
        let mut directory = ClosedNominals::default();
        let key = (definition.id, false, 1);
        let mut b = Budget::new(&limits);
        directory
            .remember(
                key,
                &definition,
                &args,
                &body,
                &[&inventory],
                &result,
                &mut b,
            )
            .unwrap();
        assert_eq!(directory.get(key, &args, &mut b).unwrap(), Some(result));
        assert!(
            directory
                .get(key, &[args[1].clone(), args[0].clone()], &mut b)
                .unwrap()
                .is_none()
        );
        assert!(
            directory
                .get((definition.id, true, 1), &args, &mut b)
                .unwrap()
                .is_none()
        );
        assert!(
            directory
                .get((definition.id, false, 2), &args, &mut b)
                .unwrap()
                .is_none()
        );
        let mut different = definition.id;
        different.index += 1;
        assert!(
            directory
                .get((different, false, 1), &args, &mut b)
                .unwrap()
                .is_none()
        );
        let mut qualified = args.clone();
        let CheckedActual::Type(t) = &mut qualified[0] else {
            unreachable!()
        };
        t.life.binder_count = 1;
        assert!(directory.get(key, &qualified, &mut b).unwrap().is_none());
    }
}
