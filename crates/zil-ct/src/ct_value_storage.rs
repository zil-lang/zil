//! Actual owned buffers of admitted CT values. Frame cells hold the inline
//! enum; their backing arrays are charged separately before allocation.
use super::*;
fn pay(budget: &mut Option<&mut CtExecutionBudget>, work: usize) -> CompileResult<()> {
    if let Some(budget) = budget {
        budget.reserve_analysis_work(work as u64)?;
    }
    Ok(())
}
fn vector<T>(v: &Vec<T>, copy: bool) -> u64 {
    (if copy { v.len() } else { v.capacity() }) as u64 * std::mem::size_of::<T>() as u64
}
fn text(s: &String, budget: &mut Option<&mut CtExecutionBudget>) -> CompileResult<u64> {
    pay(budget, s.len())?;
    Ok(if budget.is_some() {
        s.len()
    } else {
        s.capacity()
    } as u64)
}
fn constant(value: &Constant, budget: &mut Option<&mut CtExecutionBudget>) -> CompileResult<u64> {
    if let Constant::WideString(v) = value {
        pay(budget, v.len())?;
        Ok(vector(v, budget.is_some()))
    } else {
        Ok(0)
    }
}
pub(super) fn ty(value: &CtTy, mut budget: Option<&mut CtExecutionBudget>) -> CompileResult<u64> {
    pay(&mut budget, 1)?;
    Ok(match value {
        CtTy::Tuple(v) | CtTy::AnonymousSum(v) => {
            pay(&mut budget, v.len())?;
            let mut bytes = vector(v, budget.is_some());
            for item in v {
                bytes = bytes.saturating_add(ty(item, budget.as_deref_mut())?);
            }
            bytes
        }
        CtTy::Array { element, .. } | CtTy::List(element) => {
            (std::mem::size_of::<CtTy>() as u64).saturating_add(ty(element, budget)?)
        }
        _ => 0,
    })
}
pub(crate) fn graph(
    value: &CtTypeRef,
    mut budget: Option<&mut CtExecutionBudget>,
) -> CompileResult<u64> {
    pay(
        &mut budget,
        value
            .nodes
            .len()
            .saturating_add(value.ambient_classes.len()),
    )?;
    let copy = budget.is_some();
    let mut bytes = vector(&value.nodes, copy).saturating_add(vector(&value.ambient_classes, copy));
    for node in &value.nodes {
        bytes = bytes.saturating_add(match node {
            CtTypeNode::BorrowedStorage { life, .. } => {
                pay(
                    &mut budget,
                    life.slots
                        .len()
                        .saturating_add(life.retained.len())
                        .saturating_add(life.nominal_arguments.len()),
                )?;
                let mut n = vector(&life.slots, copy)
                    .saturating_add(vector(&life.retained, copy))
                    .saturating_add(vector(&life.nominal_arguments, copy));
                for row in &life.slots {
                    pay(&mut budget, row.path.len())?;
                    n = n.saturating_add(vector(&row.path, copy));
                }
                for row in &life.retained {
                    pay(&mut budget, row.path.len())?;
                    n = n.saturating_add(vector(&row.path, copy));
                }
                for row in &life.nominal_arguments {
                    pay(&mut budget, row.path.len())?;
                    n = n.saturating_add(vector(&row.path, copy));
                }
                n
            }
            CtTypeNode::Nominal { arguments, .. } => {
                pay(&mut budget, arguments.len())?;
                let mut n = vector(arguments, copy);
                for arg in arguments {
                    if let CtTypeArgument::Scalar(c) = arg {
                        n = n.saturating_add(constant(c, &mut budget)?);
                    }
                }
                n
            }
            CtTypeNode::Tuple(v) | CtTypeNode::AnonymousSum(v) => {
                pay(&mut budget, v.len())?;
                vector(v, copy)
            }
            CtTypeNode::BorrowedFunctionPointer {
                parameters, life, ..
            } => {
                pay(
                    &mut budget,
                    parameters
                        .len()
                        .saturating_add(life.bindings.len())
                        .saturating_add(life.static_bindings.len())
                        .saturating_add(life.classes.len()),
                )?;
                let mut n = vector(parameters, copy)
                    .saturating_add(vector(&life.bindings, copy))
                    .saturating_add(vector(&life.classes, copy))
                    .saturating_add(vector(&life.static_bindings, copy));
                for row in &life.bindings {
                    pay(&mut budget, row.position.path.len())?;
                    n = n.saturating_add(vector(&row.position.path, copy));
                }
                for position in &life.static_bindings {
                    pay(&mut budget, position.path.len())?;
                    n = n.saturating_add(vector(&position.path, copy));
                }
                for row in &life.classes {
                    pay(&mut budget, row.position.path.len())?;
                    n = n.saturating_add(vector(&row.position.path, copy));
                }
                n
            }
            CtTypeNode::FunctionPointer { parameters, .. }
            | CtTypeNode::ForeignCallback { parameters, .. } => {
                pay(&mut budget, parameters.len())?;
                vector(parameters, copy)
            }
            _ => 0,
        });
    }
    Ok(bytes)
}
pub(super) fn ident(
    value: &CtIdent,
    budget: &mut Option<&mut CtExecutionBudget>,
) -> CompileResult<u64> {
    pay(budget, 1)?;
    let mut bytes = text(&value.text, budget)?;
    if let Some(value) = &value.member_owner_type {
        bytes = bytes.saturating_add(graph(value, budget.as_deref_mut())?);
    }
    Ok(bytes)
}
fn token(value: &CtToken, budget: &mut Option<&mut CtExecutionBudget>) -> CompileResult<u64> {
    let mut bytes = text(&value.text, budget)?;
    if let Some(value) = &value.name {
        bytes = bytes.saturating_add(ident(value, budget)?);
    }
    Ok(bytes)
}
pub(super) fn field(
    value: &CtFieldInfo,
    budget: &mut Option<&mut CtExecutionBudget>,
) -> CompileResult<u64> {
    pay(budget, 1)?;
    Ok(graph(&value.owner_type, budget.as_deref_mut())?
        .saturating_add(graph(&value.ty, budget.as_deref_mut())?)
        .saturating_add(ident(&value.name, budget)?))
}
pub(super) fn aggregate(
    values: &Vec<CtValue>,
    mut budget: Option<&mut CtExecutionBudget>,
) -> CompileResult<u64> {
    pay(&mut budget, values.len())?;
    let mut bytes = vector(values, budget.is_some());
    for value in values {
        bytes = bytes.saturating_add(owned(value, budget.as_deref_mut())?);
    }
    Ok(bytes)
}
pub(super) fn owned(
    value: &CtValue,
    mut budget: Option<&mut CtExecutionBudget>,
) -> CompileResult<u64> {
    pay(&mut budget, 1)?;
    let copy = budget.is_some();
    Ok(match value {
        CtValue::Scalar(c) => constant(c, &mut budget)?,
        CtValue::Text(s) => text(s, &mut budget)?,
        CtValue::Type(t) => graph(t, budget)?,
        CtValue::Ident(i) => ident(i, &mut budget)?,
        CtValue::Syntax(t) => {
            pay(
                &mut budget,
                t.nodes
                    .len()
                    .saturating_add(t.origins.len())
                    .saturating_add(t.roots.len()),
            )?;
            let mut bytes = vector(&t.nodes, copy)
                .saturating_add(vector(&t.origins, copy))
                .saturating_add(vector(&t.roots, copy));
            for node in &t.nodes {
                bytes = bytes.saturating_add(match node {
                    CtSyntaxNode::Token(t) => token(t, &mut budget)?,
                    CtSyntaxNode::Group { children, .. } => {
                        pay(&mut budget, children.len())?;
                        vector(children, copy)
                    }
                    CtSyntaxNode::Hole { .. } => 0,
                });
            }
            bytes
        }
        CtValue::TokenTree(t) => {
            pay(
                &mut budget,
                t.nodes
                    .len()
                    .saturating_add(t.origins.len())
                    .saturating_add(t.roots.len()),
            )?;
            let mut bytes = vector(&t.nodes, copy)
                .saturating_add(vector(&t.origins, copy))
                .saturating_add(vector(&t.roots, copy));
            for node in &t.nodes {
                bytes = bytes.saturating_add(match node {
                    CtTokenNode::Token(t) => token(t, &mut budget)?,
                    CtTokenNode::Group { children, .. } => {
                        pay(&mut budget, children.len())?;
                        vector(children, copy)
                    }
                });
            }
            bytes
        }
        CtValue::TokenCursor(_) => 0,
        CtValue::ShapeInfo(s) => {
            pay(&mut budget, s.visible_fields.len())?;
            let mut bytes = vector(&s.visible_fields, copy);
            for value in &s.visible_fields {
                bytes = bytes.saturating_add(field(value, &mut budget)?);
            }
            bytes
        }
        CtValue::FieldInfo(f) => field(f, &mut budget)?,
        CtValue::ImplInfo(i) => graph(&i.for_type, budget)?,
        CtValue::LayoutInfo(l) => {
            pay(&mut budget, l.field_offsets.len())?;
            vector(&l.field_offsets, copy)
        }
        CtValue::Tuple(v)
        | CtValue::Array(v)
        | CtValue::List(v)
        | CtValue::Record { fields: v, .. }
        | CtValue::Variant { fields: v, .. } => aggregate(v, budget)?,
        CtValue::AnonymousSum {
            members, payload, ..
        } => {
            pay(&mut budget, members.len())?;
            let mut bytes =
                vector(members, copy).saturating_add(std::mem::size_of::<CtValue>() as u64);
            for member in members {
                bytes = bytes.saturating_add(ty(member, budget.as_deref_mut())?);
            }
            bytes.saturating_add(owned(payload, budget)?)
        }
    })
}
