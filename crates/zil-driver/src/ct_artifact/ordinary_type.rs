//! Translate canonical CT Type data back to the same checked ordinary shape
//! table used by the target layout backend. Generated types are absent here.
use super::{snapshot::ShapeSnapshot, source_index::SourceIndex};
use zil_base::{CompileResult, Diagnostic, Phase};
use zil_ir::{AggregateKind, Type, ct_artifact::*};

fn error(message: impl Into<String>) -> Vec<Diagnostic> {
    vec![Diagnostic::error(Phase::Ct, "E-CT-PHASE-DEPENDENCY", None, message)]
}

pub(super) fn checked_type(ty: &CtTypeRef, snapshot: &ShapeSnapshot, index: &SourceIndex, max_depth: usize) -> CompileResult<Type> {
    fn node(ty: &CtTypeRef, id: CtTypeNodeId, snapshot: &ShapeSnapshot, index: &SourceIndex, depth: usize, max_depth: usize) -> CompileResult<Type> {
        if depth > max_depth { return Err(error("CT type layout dependency is too deep")); }
        let actual = ty.nodes.get(id.0 as usize).ok_or_else(|| error("CT Type node is out of range"))?;
        Ok(match actual {
            CtTypeNode::Primitive(scalar) => match scalar {
                CtScalarTy::Unit => Type::Unit, CtScalarTy::Bool => Type::Bool, CtScalarTy::U8 => Type::U8,
                CtScalarTy::U16 => Type::U16, CtScalarTy::I32 => Type::I32, CtScalarTy::I64 => Type::I64,
            },
            CtTypeNode::Builtin(kind) => match kind {
                CtBuiltinType::ByteBuffer => Type::ByteBuffer,
                CtBuiltinType::ForeignHandle => Type::ForeignHandle,
                CtBuiltinType::CWideString => Type::CWideString,
            },
            CtTypeNode::Nominal { symbol, arguments } => {
                if !arguments.is_empty() { return Err(error("generic nominal layout is not complete in this shape batch")); }
                let span = index.span(symbol).ok_or_else(|| error("nominal layout symbol is not in frozen source"))?;
                let module = snapshot.core.modules.iter().find(|module| module.core.module_id == symbol.module)
                    .ok_or_else(|| error("nominal layout module is absent"))?;
                if let Some(aggregate) = module.core.aggregates.iter().find(|aggregate| aggregate.span == span) {
                    let name = format!("@module{}:{}", symbol.module.0, aggregate.name);
                    let fields = aggregate.fields.iter().map(|field| (field.name.clone(), field.ty.clone())).collect();
                    match aggregate.kind {
                        AggregateKind::Struct => Type::Record { name, fields },
                        AggregateKind::Package => Type::Package { name, fields },
                        AggregateKind::Resource => return Err(error("bare resource layout is not an ordinary value type")),
                    }
                } else if let Some(enumeration) = module.core.enums.iter().find(|enumeration| enumeration.span == span) {
                    Type::Enum { name: format!("@module{}:{}", symbol.module.0, enumeration.name), variants: enumeration.variants.clone() }
                } else { return Err(error("nominal layout shape has not completed")); }
            }
            CtTypeNode::Tuple(items) => Type::Tuple(items.iter().map(|id| node(ty, *id, snapshot, index, depth + 1, max_depth)).collect::<CompileResult<Vec<_>>>()?),
            CtTypeNode::Array { element, length } => Type::Array { element: Box::new(node(ty, *element, snapshot, index, depth + 1, max_depth)?), length: *length },
            CtTypeNode::Slice(element) => Type::Slice { element: Box::new(node(ty, *element, snapshot, index, depth + 1, max_depth)?) },
            CtTypeNode::Address { mutable, inner } => Type::Address { mutable: *mutable, inner: Box::new(node(ty, *inner, snapshot, index, depth + 1, max_depth)?) },
            CtTypeNode::Managed(inner) => Type::Managed(Box::new(node(ty, *inner, snapshot, index, depth + 1, max_depth)?)),
            CtTypeNode::Desc(inner) => Type::Desc(Box::new(node(ty, *inner, snapshot, index, depth + 1, max_depth)?)),
            CtTypeNode::OwnedStore(element) => Type::OwnedStore { element: Box::new(node(ty, *element, snapshot, index, depth + 1, max_depth)?) },
            CtTypeNode::FunctionPointer { parameters, result } => Type::FunctionPointer {
                parameters: parameters.iter().map(|id| node(ty, *id, snapshot, index, depth + 1, max_depth)).collect::<CompileResult<Vec<_>>>()?,
                result: Box::new(node(ty, *result, snapshot, index, depth + 1, max_depth)?),
            },
            CtTypeNode::ForeignCallback { parameters, result } => Type::ForeignCallback {
                parameters: parameters.iter().map(|id| node(ty, *id, snapshot, index, depth + 1, max_depth)).collect::<CompileResult<Vec<_>>>()?,
                result: Box::new(node(ty, *result, snapshot, index, depth + 1, max_depth)?),
            },
            CtTypeNode::ErrorUnion { error: e, result } => Type::ErrorUnion {
                error: Box::new(node(ty, *e, snapshot, index, depth + 1, max_depth)?),
                result: Box::new(node(ty, *result, snapshot, index, depth + 1, max_depth)?),
            },
            CtTypeNode::Opaque { .. } => return Err(error("opaque type layout is not visible")),
        })
    }
    node(ty, ty.root, snapshot, index, 0, max_depth)
}
