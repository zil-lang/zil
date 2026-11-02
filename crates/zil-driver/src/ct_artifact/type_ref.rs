//! Reify only checked ordinary type shapes into CT data. Source spelling alone
//! never grants a nominal identity or a target layout.
use super::{snapshot::ShapeSnapshot, source_index::SourceIndex};
use zil_base::{CompileResult, Diagnostic, Phase};
use zil_ir::{Type, ct_artifact::*};

fn reject(message: impl Into<String>) -> Vec<Diagnostic> {
    vec![Diagnostic::error(Phase::Ct, "E-CT-TYPE-PHASE", None, message)]
}

struct Builder<'a> {
    snapshot: &'a ShapeSnapshot,
    index: &'a SourceIndex,
    nodes: Vec<CtTypeNode>,
    left: usize,
}
impl Builder<'_> {
    fn push(&mut self, node: CtTypeNode) -> CompileResult<CtTypeNodeId> {
        self.left = self.left.checked_sub(1).ok_or_else(|| reject("CT type view exceeds node budget"))?;
        let id = CtTypeNodeId(u32::try_from(self.nodes.len()).map_err(|_| reject("CT type node identity overflow"))?);
        self.nodes.push(node);
        Ok(id)
    }
    fn nominal(&self, name: &str) -> CompileResult<CtSymbolId> {
        for module in &self.snapshot.core.modules {
            let prefix = format!("@module{}:", module.core.module_id.0);
            for aggregate in &module.core.aggregates {
                if name == aggregate.name || name == format!("{prefix}{}", aggregate.name) {
                    // This instance shares its declaration span with the
                    // generic template. The concrete argument lineage is
                    // not recorded in CoreAggregate, so a bare nominal ID
                    // would collapse distinct Type values.
                    if aggregate.name.starts_with("$generic_") {
                        return Err(reject("instantiated generic type has no canonical argument lineage in this shape view"));
                    }
                    if let Some(symbol) = self.index.symbol_at(module.core.module_id, aggregate.span) { return Ok(symbol); }
                }
            }
            for enumeration in &module.core.enums {
                if (name == enumeration.name || name == format!("{prefix}{}", enumeration.name))
                    && let Some(symbol) = self.index.symbol_at(module.core.module_id, enumeration.span) {
                    return Ok(symbol);
                }
            }
        }
        Err(reject(format!("nominal type {name} has no checked pre-generation declaration")))
    }
    fn ty(&mut self, ty: &Type) -> CompileResult<CtTypeNodeId> {
        use CtScalarTy as S;
        match ty {
            Type::Unit => self.push(CtTypeNode::Primitive(S::Unit)),
            Type::Bool => self.push(CtTypeNode::Primitive(S::Bool)),
            Type::U8 => self.push(CtTypeNode::Primitive(S::U8)),
            Type::U16 => self.push(CtTypeNode::Primitive(S::U16)),
            Type::I32 => self.push(CtTypeNode::Primitive(S::I32)),
            Type::I64 => self.push(CtTypeNode::Primitive(S::I64)),
            Type::ByteBuffer => self.push(CtTypeNode::Builtin(CtBuiltinType::ByteBuffer)),
            Type::ForeignHandle => self.push(CtTypeNode::Builtin(CtBuiltinType::ForeignHandle)),
            Type::CWideString => self.push(CtTypeNode::Builtin(CtBuiltinType::CWideString)),
            Type::Record { name, .. } | Type::Package { name, .. } | Type::Enum { name, .. } => {
                let symbol = self.nominal(name)?;
                self.push(CtTypeNode::Nominal { symbol, arguments: Vec::new() })
            }
            Type::Tuple(items) => { let ids = items.iter().map(|ty| self.ty(ty)).collect::<CompileResult<Vec<_>>>()?; self.push(CtTypeNode::Tuple(ids)) }
            Type::Array { element, length } => { let element = self.ty(element)?; self.push(CtTypeNode::Array { element, length: *length }) }
            Type::Slice { element } => { let element = self.ty(element)?; self.push(CtTypeNode::Slice(element)) }
            Type::Address { mutable, inner } => { let inner = self.ty(inner)?; self.push(CtTypeNode::Address { mutable: *mutable, inner }) }
            Type::Managed(inner) => { let inner = self.ty(inner)?; self.push(CtTypeNode::Managed(inner)) }
            Type::Desc(inner) => { let inner = self.ty(inner)?; self.push(CtTypeNode::Desc(inner)) }
            Type::OwnedStore { element } => { let element = self.ty(element)?; self.push(CtTypeNode::OwnedStore(element)) }
            Type::FunctionPointer { parameters, result } => {
                let parameters = parameters.iter().map(|ty| self.ty(ty)).collect::<CompileResult<Vec<_>>>()?;
                let result = self.ty(result)?;
                self.push(CtTypeNode::FunctionPointer { parameters, result })
            }
            Type::ForeignCallback { parameters, result } => {
                let parameters = parameters.iter().map(|ty| self.ty(ty)).collect::<CompileResult<Vec<_>>>()?;
                let result = self.ty(result)?;
                self.push(CtTypeNode::ForeignCallback { parameters, result })
            }
            Type::ErrorUnion { error, result } => {
                let error = self.ty(error)?;
                let result = self.ty(result)?;
                self.push(CtTypeNode::ErrorUnion { error, result })
            }
            Type::Named(_) | Type::Opaque { .. } | Type::Closure(_) | Type::Unsupported(_) => Err(reject("type has no completed CT shape")),
        }
    }
}

pub(super) fn from_checked_type(ty: &Type, snapshot: &ShapeSnapshot, index: &SourceIndex, max_nodes: usize) -> CompileResult<CtTypeRef> {
    let mut builder = Builder { snapshot, index, nodes: Vec::new(), left: max_nodes };
    let root = builder.ty(ty)?;
    Ok(CtTypeRef { nodes: builder.nodes, root })
}
