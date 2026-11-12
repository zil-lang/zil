//! Relate complete query keys in two lexical domains before Context mapping.
//! The result is a partial substitution, not an inverse Context map and not a
//! renaming of any intrinsic nominal or opaque class directory.
use super::*;

#[derive(Clone, Debug)]
pub(super) struct Relation {
    rows: Vec<DynLifeEmbedding>,
}
impl Relation {
    fn pair(&mut self, from: StorageBinder, to: StorageBinder, b: &mut Budget) -> Check<bool> {
        b.spend(self.rows.len().saturating_add(1))?;
        if let Some(previous) = self.rows.iter().find(|row| row.from == from) {
            return Ok(previous.to == to);
        }
        self.rows.push(DynLifeEmbedding { from, to });
        Ok(true)
    }
    fn value(&mut self, from: &DynTypeValue, to: &DynTypeValue, b: &mut Budget) -> Check<bool> {
        dynamic_metadata::recipe(from, b)?;
        dynamic_metadata::recipe(to, b)?;
        if from.ty != to.ty || from.life != to.life || from.classes.len() != to.classes.len() {
            return Ok(false);
        }
        for (from, to) in from.classes.iter().zip(&to.classes) {
            if !self.pair(*from, *to, b)? {
                return Ok(false);
            }
        }
        Ok(true)
    }
    fn actual(
        &mut self,
        module: &TypedModule,
        from: &DynGenericActual,
        to: &DynGenericActual,
        b: &mut Budget,
    ) -> Check<bool> {
        dynamic_metadata::recipe(from, b)?;
        dynamic_metadata::recipe(to, b)?;
        Ok(match (from, to) {
            (DynGenericActual::Life(from), DynGenericActual::Life(to)) => {
                self.pair(*from, *to, b)?
            }
            (DynGenericActual::QualifiedType(from), DynGenericActual::QualifiedType(to)) => {
                self.value(from, to, b)?
            }
            (DynGenericActual::Type(from), DynGenericActual::Type(to)) => {
                values::closed(module, from, b)?;
                values::closed(module, to, b)?;
                from == to
            }
            (DynGenericActual::Type(from), DynGenericActual::QualifiedType(to))
            | (DynGenericActual::QualifiedType(to), DynGenericActual::Type(from)) => {
                values::closed(module, from, b)?;
                from == &to.ty && to.life.binder_count == 0 && to.classes.is_empty()
            }
            (DynGenericActual::Const(from), DynGenericActual::Const(to)) => from == to,
            _ => false,
        })
    }
    pub(super) fn class(&self, binder: StorageBinder, b: &mut Budget) -> Check<StorageBinder> {
        b.spend(self.rows.len())?;
        self.rows.iter().find(|row| row.from == binder).map(|row|row.to)
            .ok_or_else(|| bad("associated RHS uses a lexical class not anchored by the complete actual query key"))
    }
    pub(super) fn mapped(&self, value: &DynTypeValue, b: &mut Budget) -> Check<DynTypeValue> {
        dynamic_metadata::recipe(value, b)?;
        let mut result = value.clone();
        for class in &mut result.classes {
            *class = self.class(*class, b)?;
        }
        Ok(result)
    }
    /// Compare only after establishing the original complete key relation.
    /// Several lexical classes may have the same Context image on either side.
    pub(super) fn context(
        &self,
        from: &[DynLifeEmbedding],
        to: &[DynLifeEmbedding],
        b: &mut Budget,
    ) -> Check<()> {
        for pair in &self.rows {
            b.spend(from.len().saturating_add(to.len()).saturating_add(1))?;
            let left = from
                .iter()
                .find(|row| row.from == pair.from)
                .map(|row| row.to)
                .ok_or_else(|| bad("query source lexical class has no outer Context image"))?;
            let right = to
                .iter()
                .find(|row| row.from == pair.to)
                .map(|row| row.to)
                .ok_or_else(|| bad("query target lexical class has no outer Context image"))?;
            if left != right {
                return Err(bad(
                    "complete local query substitution disagrees with its actual Context composition",
                ));
            }
        }
        Ok(())
    }
}

pub(super) struct Key<'a> {
    pub receiver: &'a DynTypeValue,
    pub application: &'a DynTraitApplication,
    pub scope: DynInstantiationScope,
}
/// Mismatch is a different query, never permission to try a same-layout type.
/// Every matching candidate must subsequently prove the original implementation
/// and requirements; a matching key alone is not an implementation witness.
pub(super) fn relate(
    module: &TypedModule,
    from: Key<'_>,
    to: Key<'_>,
    b: &mut Budget,
) -> Check<Option<Relation>> {
    scopes::bounded(from.scope, b)?;
    scopes::bounded(to.scope, b)?;
    values::check(module, from.receiver, from.scope, b)?;
    values::check(module, to.receiver, to.scope, b)?;
    scopes::actuals(module, &from.application.actuals, from.scope, b)?;
    scopes::actuals(module, &to.application.actuals, to.scope, b)?;
    if from.application.declaration != to.application.declaration
        || from.application.actuals.len() != to.application.actuals.len()
    {
        return Ok(None);
    }
    let mut relation = Relation { rows: vec![] };
    if !relation.value(from.receiver, to.receiver, b)? {
        return Ok(None);
    }
    for (from, to) in from.application.actuals.iter().zip(&to.application.actuals) {
        if !relation.actual(module, from, to, b)? {
            return Ok(None);
        }
    }
    Ok(Some(relation))
}

#[cfg(test)]
mod tests {
    use super::*;
    use zil_base::{ModuleId, NodeId};
    fn value(ty: Type) -> DynTypeValue {
        DynTypeValue {
            ty,
            life: StorageLifeShape {
                binder_count: 0,
                slots: vec![],
                retained: vec![],
                nominal_arguments: vec![],
            },
            classes: vec![],
        }
    }
    fn app(actuals: Vec<DynGenericActual>) -> DynTraitApplication {
        DynTraitApplication {
            declaration: LifeDeclarationId {
                module: ModuleId(0),
                declaration: NodeId(8),
                compiler: false,
            },
            actuals,
        }
    }
    #[test]
    fn full_lexical_query_key_precedes_context_and_keeps_unused_formals_unanchored() {
        let (module, _) = crate::tests::baseline();
        let limits = Limits::default();
        let receiver = value(Type::I32);
        let left = app(vec![
            DynGenericActual::Life(StorageBinder::Fixed(0)),
            DynGenericActual::Life(StorageBinder::Fixed(1)),
        ]);
        let right = app(vec![DynGenericActual::Life(StorageBinder::Fixed(0)); 2]);
        let from = DynInstantiationScope {
            fixed_binder_count: 3,
            invocation_binder_count: 0,
        };
        let to = DynInstantiationScope {
            fixed_binder_count: 1,
            invocation_binder_count: 0,
        };
        let relation = relate(
            &module,
            Key {
                receiver: &receiver,
                application: &left,
                scope: from,
            },
            Key {
                receiver: &receiver,
                application: &right,
                scope: to,
            },
            &mut Budget::new(&limits),
        )
        .unwrap()
        .unwrap();
        assert_eq!(
            relation
                .class(StorageBinder::Fixed(0), &mut Budget::new(&limits))
                .unwrap(),
            StorageBinder::Fixed(0)
        );
        assert_eq!(
            relation
                .class(StorageBinder::Fixed(1), &mut Budget::new(&limits))
                .unwrap(),
            StorageBinder::Fixed(0)
        );
        assert!(
            relation
                .class(StorageBinder::Fixed(2), &mut Budget::new(&limits))
                .is_err()
        );
        let left_context = (0..3)
            .map(|i| DynLifeEmbedding {
                from: StorageBinder::Fixed(i),
                to: StorageBinder::Fixed(0),
            })
            .collect::<Vec<_>>();
        let right_context = [DynLifeEmbedding {
            from: StorageBinder::Fixed(0),
            to: StorageBinder::Fixed(0),
        }];
        relation
            .context(&left_context, &right_context, &mut Budget::new(&limits))
            .unwrap();
        let mut other = left_context.clone();
        other[1].to = StorageBinder::Fixed(1);
        assert!(
            relation
                .context(&other, &right_context, &mut Budget::new(&limits))
                .is_err()
        );
        assert!(
            relate(
                &module,
                Key {
                    receiver: &receiver,
                    application: &right,
                    scope: to
                },
                Key {
                    receiver: &receiver,
                    application: &left,
                    scope: from
                },
                &mut Budget::new(&limits)
            )
            .unwrap()
            .is_none()
        );
    }
    #[test]
    fn matching_context_values_cannot_replace_the_full_intrinsic_receiver_type() {
        let (module, _) = crate::tests::baseline();
        let limits = Limits::default();
        let left = value(Type::I32);
        let right = value(Type::U32);
        let app = app(vec![]);
        let scope = DynInstantiationScope {
            fixed_binder_count: 0,
            invocation_binder_count: 0,
        };
        assert!(
            relate(
                &module,
                Key {
                    receiver: &left,
                    application: &app,
                    scope
                },
                Key {
                    receiver: &right,
                    application: &app,
                    scope
                },
                &mut Budget::new(&limits)
            )
            .unwrap()
            .is_none()
        );
        let relation = relate(
            &module,
            Key {
                receiver: &left,
                application: &app,
                scope,
            },
            Key {
                receiver: &left,
                application: &app,
                scope,
            },
            &mut Budget::new(&limits),
        )
        .unwrap()
        .unwrap();
        assert_eq!(
            relation.mapped(&left, &mut Budget::new(&limits)).unwrap(),
            left
        );
    }
}
