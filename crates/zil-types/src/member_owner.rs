//! Member provenance is a nominal constraint, never a receiver conversion.
use super::*;
fn base(ty: &Type) -> &Type {
    match ty {
        Type::Address { inner, .. } => match inner.as_ref() {
            Type::Managed(payload) => payload,
            other => other,
        },
        Type::Managed(inner) | Type::Desc(inner) => inner,
        other => other,
    }
}
/// Compare the complete actual nominal instance, retaining the original type
/// and access mode for the subsequent ordinary projection implementation.
pub(super) fn check_member_owner(actual: &Type, owner: Option<&Type>, span: Span) -> R<()> {
    let Some(owner) = owner else {
        return Ok(());
    };
    if base(actual) == base(owner) {
        Ok(())
    } else {
        Err(error(
            "E-MEMBER-OWNER",
            span,
            "member provenance owner differs from the actual receiver nominal type or generic instance",
        ))
    }
}
impl Builder<'_, '_> {
    pub(super) fn member_owner_work(&mut self, owner: Option<&Type>, span: Span) -> R<()> {
        if let Some(owner) = owner {
            // Equality short-circuits at shape/length/name mismatches; its
            // traversed common prefix is bounded by the complete owner tree.
            let work = inference::type_snapshot_work(owner, span, self.limits)?;
            self.closures
                .snapshot(work.saturating_mul(2).saturating_add(2), span)?;
        }
        Ok(())
    }
    pub(super) fn member_owner(
        &mut self,
        actual: &Type,
        owner: Option<&Type>,
        span: Span,
    ) -> R<()> {
        self.member_owner_work(owner, span)?;
        check_member_owner(actual, owner, span)
    }
}
/// Symbolic graphs must infer the receiver from source uses and call sites;
/// owner provenance is never an extra bound or a unification constraint.
pub(super) fn concrete(ty: &Type, charge: &mut impl FnMut() -> R<()>) -> R<bool> {
    charge()?;
    Ok(match ty {
        Type::Named(_) | Type::Unsupported(_) => false,
        Type::Tuple(fields) => {
            for ty in fields {
                if !concrete(ty, charge)? {
                    return Ok(false);
                }
            }
            true
        }
        Type::Record { fields, .. } | Type::Package { fields, .. } => {
            for (_, ty) in fields {
                if !concrete(ty, charge)? {
                    return Ok(false);
                }
            }
            true
        }
        Type::Enum { variants, .. } => {
            for (_, fields) in variants {
                for ty in fields {
                    if !concrete(ty, charge)? {
                        return Ok(false);
                    }
                }
            }
            true
        }
        Type::Array { element, .. }
        | Type::Slice { element }
        | Type::OwnedStore { element }
        | Type::Address { inner: element, .. }
        | Type::Managed(element)
        | Type::Desc(element)
        | Type::Opaque {
            hidden: element, ..
        } => concrete(element, charge)?,
        Type::ErrorUnion { error, result } => concrete(error, charge)? && concrete(result, charge)?,
        Type::FunctionPointer { parameters, result }
            | Type::BorrowedFunctionPointer { parameters, result, .. }
        | Type::ForeignCallback { parameters, result } => {
            for ty in parameters {
                if !concrete(ty, charge)? {
                    return Ok(false);
                }
            }
            concrete(result, charge)?
        }
        _ => true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn record(name: &str) -> Type {
        Type::Record {
            name: name.into(),
            fields: vec![("x".into(), Type::I32)],
        }
    }
    #[test]
    fn compares_full_nominal_instances_without_changing_the_receiver() {
        let a = record("Point<i32>");
        let b = record("OtherPoint<i32>");
        let phantom = record("Point<i64>");
        assert!(check_member_owner(&a, Some(&a), Span::default()).is_ok());
        assert!(check_member_owner(&a, Some(&b), Span::default()).is_err());
        assert!(check_member_owner(&a, Some(&phantom), Span::default()).is_err());
        let managed = Type::Managed(Box::new(a.clone()));
        let desc = Type::Desc(Box::new(a.clone()));
        let address = Type::Address {
            mutable: false,
            inner: Box::new(managed.clone()),
        };
        for actual in [&managed, &desc, &address] {
            assert!(check_member_owner(actual, Some(&a), Span::default()).is_ok());
        }
        let unsupported = Type::Address {
            mutable: false,
            inner: Box::new(Type::Address {
                mutable: false,
                inner: Box::new(a.clone()),
            }),
        };
        assert!(check_member_owner(&unsupported, Some(&a), Span::default()).is_err());
    }
}
