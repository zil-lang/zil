//! Compare immutable physical types within an established bounds proof.
//! Every visited pair follows the same constructor and ordered member. A
//! positive comparison transfers only structural equality of the full type.
use super::*;

pub(super) fn compare(
    left: &Type,
    right: &Type,
    b: &mut Budget,
    mut charge_comparison: impl FnMut(&Type, &mut Budget) -> Check<()>,
) -> Check<bool> {
    fn same_name(left: &str, right: &str, b: &mut Budget) -> Check<bool> {
        if left.len() != right.len() {
            return Ok(false);
        }
        b.spend(left.len())?;
        Ok(left == right)
    }

    let mut next = Some((left, right));
    let mut pending = Vec::new();
    while let Some((left, right)) = next.take().or_else(|| pending.pop()) {
        b.spend(1)?;
        if std::ptr::eq(left, right) {
            continue;
        }
        if std::mem::discriminant(left) != std::mem::discriminant(right) {
            return Ok(false);
        }
        match (left, right) {
            (
                Type::Address {
                    mutable: a,
                    inner: x,
                },
                Type::Address {
                    mutable: b,
                    inner: y,
                },
            ) => {
                if a != b {
                    return Ok(false);
                }
                next = Some((x.as_ref(), y.as_ref()));
            }
            (Type::Managed(x), Type::Managed(y))
            | (Type::Desc(x), Type::Desc(y))
            | (Type::OwnedStore { element: x }, Type::OwnedStore { element: y })
            | (Type::Slice { element: x }, Type::Slice { element: y }) => {
                next = Some((x.as_ref(), y.as_ref()));
            }
            (Type::Record { name: a, fields: x }, Type::Record { name: c, fields: y })
            | (Type::Package { name: a, fields: x }, Type::Package { name: c, fields: y }) => {
                if x.len() != y.len() || !same_name(a, c, b)? {
                    return Ok(false);
                }
                b.spend(x.len().saturating_mul(2))?;
                for ((name, field), (other, actual)) in x.iter().zip(y) {
                    if !same_name(name, other, b)? {
                        return Ok(false);
                    }
                    pending.push((field, actual));
                }
            }
            (
                Type::Enum {
                    name: a,
                    variants: x,
                },
                Type::Enum {
                    name: c,
                    variants: y,
                },
            ) => {
                if x.len() != y.len() || !same_name(a, c, b)? {
                    return Ok(false);
                }
                b.spend(x.len())?;
                for ((name, fields), (other, actual)) in x.iter().zip(y) {
                    if fields.len() != actual.len() || !same_name(name, other, b)? {
                        return Ok(false);
                    }
                    b.spend(fields.len().saturating_mul(2))?;
                    pending.extend(fields.iter().zip(actual));
                }
            }
            (
                Type::Array {
                    element: x,
                    length: a,
                },
                Type::Array {
                    element: y,
                    length: c,
                },
            ) => {
                if a != c {
                    return Ok(false);
                }
                next = Some((x.as_ref(), y.as_ref()));
            }
            (Type::Tuple(x), Type::Tuple(y)) => {
                if x.len() != y.len() {
                    return Ok(false);
                }
                b.spend(x.len().saturating_mul(2))?;
                pending.extend(x.iter().zip(y));
            }
            (
                Type::ErrorUnion {
                    error: a,
                    result: x,
                },
                Type::ErrorUnion {
                    error: c,
                    result: y,
                },
            ) => {
                b.spend(1)?;
                next = Some((a.as_ref(), c.as_ref()));
                pending.push((x.as_ref(), y.as_ref()));
            }
            (
                Type::Opaque {
                    identity: a,
                    hidden: x,
                },
                Type::Opaque {
                    identity: c,
                    hidden: y,
                },
            ) => {
                if a != c {
                    return Ok(false);
                }
                next = Some((x.as_ref(), y.as_ref()));
            }
            (Type::Named(a), Type::Named(c)) | (Type::Unsupported(a), Type::Unsupported(c)) => {
                if !same_name(a, c, b)? {
                    return Ok(false);
                }
            }
            _ if left.is_b0_scalar() => {}
            _ => {
                charge_comparison(left, b)?;
                if left != right {
                    return Ok(false);
                }
            }
        }
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ordered_nominals_and_referents_match_derived_equality() {
        fn package(name: &str, fields: &[(&str, Type)]) -> Type {
            Type::Package {
                name: name.to_owned(),
                fields: fields
                    .iter()
                    .map(|(n, t)| ((*n).to_owned(), t.clone()))
                    .collect(),
            }
        }

        let closure = Type::Closure(ClosureId(1));
        let fields = [("left", closure.clone()), ("right", Type::I32)];
        let mut types = vec![
            package("A", &fields),
            package("B", &fields),
            package("A", &[fields[1].clone(), fields[0].clone()]),
            package(
                "A",
                &[("left", Type::Closure(ClosureId(2))), fields[1].clone()],
            ),
        ];
        for inner in types.clone() {
            for mutable in [false, true] {
                let reference = Type::Address {
                    mutable,
                    inner: Box::new(inner.clone()),
                };
                types.push(Type::Tuple(vec![reference.clone(), Type::I32]));
                types.push(Type::Enum {
                    name: "Result".into(),
                    variants: vec![
                        ("ok".into(), vec![reference]),
                        ("err".into(), vec![Type::U8]),
                    ],
                });
            }
        }
        let independent = types.clone();
        for left in &types {
            for right in &independent {
                let mut budget = Budget::new(&Limits::default());
                assert_eq!(
                    compare(left, right, &mut budget, dynamic_metadata::ty).unwrap(),
                    left == right
                );
            }
        }
        let limits = Limits {
            max_analysis_steps: 0,
            ..Limits::default()
        };
        let error = compare(
            &types[0],
            &independent[0],
            &mut Budget::new(&limits),
            dynamic_metadata::ty,
        )
        .unwrap_err();
        assert_eq!(error.category, zil_base::ResultCategory::ResourceLimit);
    }
}
