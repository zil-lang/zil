//! Translate canonical CT Type data back to the same checked ordinary shape
//! table used by the target layout backend. Generated types are absent here.
use super::{snapshot::ShapeSnapshot, source_index::SourceIndex};
use zil_base::{CompileResult, Diagnostic, Phase};
use zil_ir::{AggregateKind, Type, ct_artifact::*};

fn error(message: impl Into<String>) -> Vec<Diagnostic> {
    vec![Diagnostic::error(
        Phase::Ct,
        "E-CT-PHASE-DEPENDENCY",
        None,
        message,
    )]
}

pub(super) fn checked_type(
    ty: &CtTypeRef,
    snapshot: &ShapeSnapshot,
    index: &SourceIndex,
    max_depth: usize,
    budget: &mut zil_ct::CtExecutionBudget,
) -> CompileResult<Type> {
    fn node(
        ty: &CtTypeRef,
        id: CtTypeNodeId,
        snapshot: &ShapeSnapshot,
        index: &SourceIndex,
        depth: usize,
        max_depth: usize,
        budget: &mut zil_ct::CtExecutionBudget,
    ) -> CompileResult<Type> {
        if depth > max_depth {
            return Err(error("CT type layout dependency is too deep"));
        }
        let actual = ty
            .nodes
            .get(id.0 as usize)
            .ok_or_else(|| error("CT Type node is out of range"))?;
        Ok(match actual {
            CtTypeNode::Primitive(scalar) => match scalar {
                CtScalarTy::Unit => Type::Unit,
                CtScalarTy::Bool => Type::Bool,
                CtScalarTy::U8 => Type::U8,
                CtScalarTy::U16 => Type::U16,
                CtScalarTy::I32 => Type::I32,
                CtScalarTy::I64 => Type::I64,
                CtScalarTy::F32 => Type::F32,
                CtScalarTy::F64 => Type::F64,
                CtScalarTy::U64 => Type::U64,
                CtScalarTy::Usize => Type::Usize,
            },
            CtTypeNode::Builtin(kind) => match kind {
                CtBuiltinType::ByteBuffer => Type::ByteBuffer,
                CtBuiltinType::ForeignHandle => Type::ForeignHandle,
                CtBuiltinType::CWideString => Type::CWideString,
            },
            CtTypeNode::Nominal { symbol, arguments } => {
                if !arguments.is_empty() {
                    return Err(error(
                        "generic nominal layout is not complete in this shape batch",
                    ));
                }
                let span = index
                    .span(symbol)
                    .ok_or_else(|| error("nominal layout symbol is not in frozen source"))?;
                let module = snapshot
                    .core
                    .modules
                    .iter()
                    .find(|module| module.core.module_id == symbol.module)
                    .ok_or_else(|| error("nominal layout module is absent"))?;
                if let Some(aggregate) = module
                    .core
                    .aggregates
                    .iter()
                    .find(|aggregate| aggregate.span == span)
                {
                    let name = format!("@module{}:{}", symbol.module.0, aggregate.name);
                    let fields = aggregate
                        .fields
                        .iter()
                        .map(|field| (field.name.clone(), field.ty.clone()))
                        .collect();
                    match aggregate.kind {
                        AggregateKind::Struct => Type::Record { name, fields },
                        AggregateKind::Package => Type::Package { name, fields },
                        AggregateKind::Resource => {
                            return Err(error(
                                "bare resource layout is not an ordinary value type",
                            ));
                        }
                    }
                } else if let Some(enumeration) = module
                    .core
                    .enums
                    .iter()
                    .find(|enumeration| enumeration.span == span)
                {
                    Type::Enum {
                        name: format!("@module{}:{}", symbol.module.0, enumeration.name),
                        variants: enumeration.variants.clone(),
                    }
                } else {
                    return Err(error("nominal layout shape has not completed"));
                }
            }
            CtTypeNode::AnonymousSum(items) => {
                let mut members = Vec::new();
                for id in items {
                    let member = node(ty, *id, snapshot, index, depth + 1, max_depth, budget)?;
                    if let Type::Enum { name, variants } = member {
                        if name == "@anonymous-sum:v1" {
                            for (_, fields) in variants { members.extend(fields); }
                        } else { members.push(Type::Enum { name, variants }); }
                    } else { members.push(member); }
                }
                anonymous_sum(members, budget, max_depth)?
            }
            CtTypeNode::Tuple(items) => Type::Tuple(
                items
                    .iter()
                    .map(|id| node(ty, *id, snapshot, index, depth + 1, max_depth, budget))
                    .collect::<CompileResult<Vec<_>>>()?,
            ),
            CtTypeNode::Array { element, length } => Type::Array {
                element: Box::new(node(ty, *element, snapshot, index, depth + 1, max_depth, budget)?),
                length: *length,
            },
            CtTypeNode::Slice(element) => Type::Slice {
                element: Box::new(node(ty, *element, snapshot, index, depth + 1, max_depth, budget)?),
            },
            CtTypeNode::Address { mutable, inner } => Type::Address {
                mutable: *mutable,
                inner: Box::new(node(ty, *inner, snapshot, index, depth + 1, max_depth, budget)?),
            },
            CtTypeNode::Managed(inner) => Type::Managed(Box::new(node(
                ty,
                *inner,
                snapshot,
                index,
                depth + 1,
                max_depth,
                budget,
            )?)),
            CtTypeNode::Desc(inner) => Type::Desc(Box::new(node(
                ty,
                *inner,
                snapshot,
                index,
                depth + 1,
                max_depth,
                budget,
            )?)),
            CtTypeNode::OwnedStore(element) => Type::OwnedStore {
                element: Box::new(node(ty, *element, snapshot, index, depth + 1, max_depth, budget)?),
            },
            CtTypeNode::FunctionPointer { parameters, result } => Type::FunctionPointer {
                parameters: parameters
                    .iter()
                    .map(|id| node(ty, *id, snapshot, index, depth + 1, max_depth, budget))
                    .collect::<CompileResult<Vec<_>>>()?,
                result: Box::new(node(ty, *result, snapshot, index, depth + 1, max_depth, budget)?),
            },
            CtTypeNode::ForeignCallback { parameters, result } => Type::ForeignCallback {
                parameters: parameters
                    .iter()
                    .map(|id| node(ty, *id, snapshot, index, depth + 1, max_depth, budget))
                    .collect::<CompileResult<Vec<_>>>()?,
                result: Box::new(node(ty, *result, snapshot, index, depth + 1, max_depth, budget)?),
            },
            CtTypeNode::ErrorUnion { error: e, result } => Type::ErrorUnion {
                error: Box::new(node(ty, *e, snapshot, index, depth + 1, max_depth, budget)?),
                result: Box::new(node(ty, *result, snapshot, index, depth + 1, max_depth, budget)?),
            },
            CtTypeNode::Opaque { .. } => return Err(error("opaque type layout is not visible")),
        })
    }
    node(ty, ty.root, snapshot, index, 0, max_depth, budget)
}

// This bridge sorts complete resolved runtime identities again: constructor
// graph order is deliberately not an ordinary enum tag order.
pub(super) fn anonymous_sum(mut members: Vec<Type>, budget: &mut zil_ct::CtExecutionBudget, max_depth: usize) -> CompileResult<Type> {
    let mut work = 1u64;
    for member in &members {
        let mut pending = vec![(member, 0usize)];
        while let Some((ty, depth)) = pending.pop() {
            if depth > max_depth { return Err(error("anonymous member identity exceeds depth")); }
            budget.reserve_analysis_work(1)?;
            work = work.saturating_add(1);
            match ty {
                Type::Tuple(v) => pending.extend(v.iter().map(|t| (t, depth + 1))),
                Type::Record { name, fields } | Type::Package { name, fields } => {
                    work = work.saturating_add(name.len() as u64);
                    for (name, t) in fields { work = work.saturating_add(name.len() as u64); pending.push((t, depth + 1)); }
                }
                Type::Enum { name, variants } => {
                    work = work.saturating_add(name.len() as u64);
                    for (name, fields) in variants { work = work.saturating_add(name.len() as u64); pending.extend(fields.iter().map(|t| (t, depth + 1))); }
                }
                Type::Array { element, .. } | Type::Slice { element } | Type::OwnedStore { element }
                | Type::Address { inner: element, .. } | Type::Managed(element) | Type::Desc(element) => pending.push((element, depth + 1)),
                Type::FunctionPointer { parameters, result } | Type::ForeignCallback { parameters, result } => {
                    pending.extend(parameters.iter().map(|t| (t, depth + 1))); pending.push((result, depth + 1));
                }
                Type::ErrorUnion { error, result } => { pending.push((error, depth + 1)); pending.push((result, depth + 1)); },
                Type::Named(_) | Type::Unsupported(_) | Type::Opaque { .. } | Type::Closure(_) => return Err(error("anonymous sum member is not complete")),
                _ => {},
            }
        }
    }
    budget.reserve_analysis_work(work.saturating_mul(members.len().saturating_add(2) as u64))?;
    if members.len() < 2 { return Err(error("anonymous sum needs two resolved members")); }
    members.sort();
    if members.windows(2).any(|v| v[0] == v[1]) { return Err(error("anonymous sum repeats a resolved member")); }
    Ok(Type::Enum { name: "@anonymous-sum:v1".into(), variants: members.into_iter().enumerate().map(|(i, t)| (format!("Member{i}"), vec![t])).collect() })
}
