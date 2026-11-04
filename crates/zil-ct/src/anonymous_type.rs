//! Complete, bounded constructor identity for anonymous CT sums.
//! Keys contain actual structure and nominal identities, never DAG IDs or hashes.
use crate::CtExecutionBudget;
use zil_base::{CompileResult, Diagnostic, Phase};
use zil_ir::{Constant, ct_artifact::*};

fn bad(message: &str) -> Vec<Diagnostic> {
    vec![Diagnostic::error(Phase::Ct, "E-CT-ARTIFACT", None, message)]
}
fn bound() -> Vec<Diagnostic> {
    vec![Diagnostic::limit(Phase::Ct, "CT anonymous type identity budget exhausted", None)]
}
fn add(a: u64, b: u64) -> CompileResult<u64> { a.checked_add(b).ok_or_else(bound) }
fn mul(a: u64, b: u64) -> CompileResult<u64> { a.checked_mul(b).ok_or_else(bound) }

pub(crate) fn children(node: &CtTypeNode) -> Vec<CtTypeNodeId> {
    match node {
        CtTypeNode::Nominal { arguments, .. } => arguments.iter().filter_map(|a| match a {
            CtTypeArgument::Type(id) => Some(*id), _ => None,
        }).collect(),
        CtTypeNode::Tuple(v) | CtTypeNode::AnonymousSum(v) => v.clone(),
        CtTypeNode::FunctionPointer { parameters, result }
        | CtTypeNode::ForeignCallback { parameters, result } => parameters.iter().copied().chain([*result]).collect(),
        CtTypeNode::Array { element, .. } | CtTypeNode::Slice(element)
        | CtTypeNode::Managed(element) | CtTypeNode::Desc(element)
        | CtTypeNode::OwnedStore(element) => vec![*element],
        CtTypeNode::Address { inner, .. } => vec![*inner],
        CtTypeNode::ErrorUnion { error, result } => vec![*error, *result],
        _ => Vec::new(),
    }
}

pub(crate) fn remap(node: &CtTypeNode, mut map: impl FnMut(CtTypeNodeId) -> CompileResult<CtTypeNodeId>) -> CompileResult<CtTypeNode> {
    Ok(match node {
        CtTypeNode::Primitive(s) => CtTypeNode::Primitive(*s),
        CtTypeNode::Builtin(s) => CtTypeNode::Builtin(*s),
        CtTypeNode::Opaque { identity } => CtTypeNode::Opaque { identity: identity.clone() },
        CtTypeNode::Nominal { symbol, arguments } => CtTypeNode::Nominal {
            symbol: symbol.clone(), arguments: arguments.iter().map(|a| match a {
                CtTypeArgument::Type(id) => map(*id).map(CtTypeArgument::Type),
                CtTypeArgument::Scalar(c) => Ok(CtTypeArgument::Scalar(c.clone())),
            }).collect::<CompileResult<_>>()?,
        },
        CtTypeNode::Tuple(v) => CtTypeNode::Tuple(v.iter().copied().map(&mut map).collect::<CompileResult<_>>()?),
        CtTypeNode::AnonymousSum(v) => CtTypeNode::AnonymousSum(v.iter().copied().map(&mut map).collect::<CompileResult<_>>()?),
        CtTypeNode::Array { element, length } => CtTypeNode::Array { element: map(*element)?, length: *length },
        CtTypeNode::FunctionPointer { parameters, result } => CtTypeNode::FunctionPointer {
            parameters: parameters.iter().copied().map(&mut map).collect::<CompileResult<_>>()?, result: map(*result)?,
        },
        CtTypeNode::ForeignCallback { parameters, result } => CtTypeNode::ForeignCallback {
            parameters: parameters.iter().copied().map(&mut map).collect::<CompileResult<_>>()?, result: map(*result)?,
        },
        CtTypeNode::Address { mutable, inner } => CtTypeNode::Address { mutable: *mutable, inner: map(*inner)? },
        CtTypeNode::Slice(id) => CtTypeNode::Slice(map(*id)?),
        CtTypeNode::Managed(id) => CtTypeNode::Managed(map(*id)?),
        CtTypeNode::Desc(id) => CtTypeNode::Desc(map(*id)?),
        CtTypeNode::OwnedStore(id) => CtTypeNode::OwnedStore(map(*id)?),
        CtTypeNode::ErrorUnion { error, result } => CtTypeNode::ErrorUnion { error: map(*error)?, result: map(*result)? },
    })
}

struct Canonical { ty: CtTypeRef, key: Vec<u8> }
struct Key<'a> { out: Vec<u8>, budget: &'a mut CtExecutionBudget }
impl Key<'_> {
    fn bytes(&mut self, bytes: &[u8]) -> CompileResult<()> {
        self.budget.reserve_analysis_work(bytes.len() as u64)?;
        self.budget.reserve_temporary_bytes(bytes.len() as u64)?;
        self.out.extend_from_slice(bytes); Ok(())
    }
    fn number(&mut self, value: u64) -> CompileResult<()> { self.bytes(&value.to_be_bytes()) }
    fn symbol(&mut self, s: &CtSymbolId) -> CompileResult<()> {
        self.number(u64::from(s.module.0))?; self.number(u64::from(s.declaration.0))?;
        self.bytes(&[u8::from(s.generation_path.is_some())])?;
        if let Some(fp) = s.generation_path { self.bytes(&fp.0)?; } Ok(())
    }
    fn scalar(&mut self, c: &Constant) -> CompileResult<()> {
        let (tag, bits) = match c {
            Constant::Unit => (0, 0), Constant::Bool(b) => (1, u64::from(*b)),
            Constant::U8(v) => (2, u64::from(*v)), Constant::U16(v) => (3, u64::from(*v)),
            Constant::I32(v) => (4, *v as u32 as u64), Constant::I64(v) => (5, *v as u64),
            Constant::U64(v) => (6, *v), Constant::Usize(v) => (7, *v),
            Constant::F32(v) => (8, u64::from(*v)), Constant::F64(v) => (9, *v),
            _ => return Err(bad("CT type argument is not a scalar")),
        };
        self.bytes(&[tag])?; self.number(bits)
    }
}

/// Rebuild reachable constructors without incidental DAG sharing or numbering.
/// Temporary expansion and every full-key comparison are charged before copying.
/// This is a data transformation; it grants no nominal visibility or shape access.
pub fn canonicalize_anonymous_type(reference: &CtTypeRef, budget: &mut CtExecutionBudget) -> CompileResult<CtTypeRef> {
    if reference.root.0 as usize >= reference.nodes.len() { return Err(bad("CT type root is outside its graph")); }
    let count = reference.nodes.len() as u64;
    budget.reserve_analysis_work(count)?;
    let scratch = mul(count, 96)?;
    budget.reserve_temporary_bytes(scratch)?;
    let mut canonical: Vec<Canonical> = Vec::new();
    let mut charged = scratch;
    let result = (|| {
        for (index, node) in reference.nodes.iter().enumerate() {
            let edge_count = match node {
                CtTypeNode::Nominal { arguments, .. } => arguments.len(),
                CtTypeNode::Tuple(v) | CtTypeNode::AnonymousSum(v) => v.len(),
                CtTypeNode::FunctionPointer { parameters, .. } | CtTypeNode::ForeignCallback { parameters, .. } => parameters.len().saturating_add(1),
                _ => 2,
            } as u64;
            budget.reserve_analysis_work(edge_count)?;
            let edge_bytes = mul(edge_count, 16)?;
            budget.reserve_temporary_bytes(edge_bytes)?; charged = add(charged, edge_bytes)?;
            let mut ids = children(node);
            if ids.iter().any(|id| id.0 as usize >= index) { return Err(bad("CT type child is not earlier in the graph")); }
            if matches!(node, CtTypeNode::AnonymousSum(_)) {
                if ids.len() < 2 { return Err(bad("anonymous sum needs at least two members")); }
                let mut flat = Vec::new();
                while let Some(id) = ids.pop() {
                    budget.reserve_analysis_work(1)?;
                    if let CtTypeNode::AnonymousSum(nested) = &reference.nodes[id.0 as usize] {
                        let bytes = mul(nested.len() as u64, 8)?;
                        budget.reserve_temporary_bytes(bytes)?; charged = add(charged, bytes)?;
                        budget.reserve_analysis_work(nested.len() as u64)?;
                        ids.extend(nested.iter().copied());
                    } else {
                        budget.reserve_temporary_bytes(8)?; charged = add(charged, 8)?; flat.push(id);
                    }
                }
                let compare = flat.iter().try_fold(0u64, |n, id| add(n, canonical[id.0 as usize].key.len() as u64))?;
                budget.reserve_analysis_work(mul(compare, add(flat.len() as u64, 1)?)?)?;
                flat.sort_by(|a, b| canonical[a.0 as usize].key.cmp(&canonical[b.0 as usize].key));
                if flat.windows(2).any(|v| canonical[v[0].0 as usize].key == canonical[v[1].0 as usize].key) {
                    return Err(bad("anonymous sum contains duplicate complete member types"));
                }
                ids = flat;
            }
            let output_nodes = ids.iter().try_fold(1u64, |n, id| add(n, canonical[id.0 as usize].ty.nodes.len() as u64))?;
            let bytes = mul(output_nodes, 128)?;
            budget.reserve_analysis_work(output_nodes)?;
            budget.reserve_generated_nodes(output_nodes)?;
            budget.reserve_temporary_bytes(bytes)?; charged = add(charged, bytes)?;
            let mut nodes = Vec::new();
            let mut mapped = Vec::new();
            for id in &ids {
                let child = &canonical[id.0 as usize].ty;
                let offset = u32::try_from(nodes.len()).map_err(|_| bound())?;
                for child_node in &child.nodes {
                    nodes.push(remap(child_node, |old| old.0.checked_add(offset).map(CtTypeNodeId).ok_or_else(bound))?);
                }
                mapped.push(CtTypeNodeId(child.root.0.checked_add(offset).ok_or_else(bound)?));
            }
            let mut key = Key { out: Vec::new(), budget };
            let tag = match node { CtTypeNode::Primitive(_) => 0, CtTypeNode::Builtin(_) => 1, CtTypeNode::Nominal { .. } => 2,
                CtTypeNode::Tuple(_) => 3, CtTypeNode::Array { .. } => 4, CtTypeNode::FunctionPointer { .. } => 5,
                CtTypeNode::Address { .. } => 6, CtTypeNode::Slice(_) => 7, CtTypeNode::Managed(_) => 8,
                CtTypeNode::Desc(_) => 9, CtTypeNode::OwnedStore(_) => 10, CtTypeNode::ForeignCallback { .. } => 11,
                CtTypeNode::Opaque { .. } => 12, CtTypeNode::ErrorUnion { .. } => 13, CtTypeNode::AnonymousSum(_) => 15 };
            key.bytes(&[tag])?;
            match node {
                CtTypeNode::Primitive(s) => key.bytes(&[*s as u8])?, CtTypeNode::Builtin(b) => key.bytes(&[*b as u8])?,
                CtTypeNode::Nominal { symbol, arguments } => {
                    key.symbol(symbol)?; key.number(arguments.len() as u64)?;
                    let mut child = ids.iter();
                    for arg in arguments { match arg {
                        CtTypeArgument::Type(_) => { key.bytes(&[0])?; let c = &canonical[child.next().expect("type argument").0 as usize].key;
                            key.number(c.len() as u64)?; key.bytes(c)?; },
                        CtTypeArgument::Scalar(c) => { key.bytes(&[1])?; key.scalar(c)?; },
                    }}
                }
                CtTypeNode::Opaque { identity } => key.symbol(identity)?,
                CtTypeNode::Array { length, .. } => key.number(u64::from(*length))?,
                CtTypeNode::Address { mutable, .. } => key.bytes(&[u8::from(*mutable)])?, _ => {},
            }
            if !matches!(node, CtTypeNode::Nominal { .. }) {
                key.number(ids.len() as u64)?;
                for id in &ids { let c = &canonical[id.0 as usize].key; key.number(c.len() as u64)?; key.bytes(c)?; }
            }
            let key = key.out; charged = add(charged, key.len() as u64)?;
            let final_node = if matches!(node, CtTypeNode::AnonymousSum(_)) { CtTypeNode::AnonymousSum(mapped) }
                else { let mut next = mapped.into_iter(); remap(node, |_| Ok(next.next().expect("mapped child")))? };
            let root = CtTypeNodeId(u32::try_from(nodes.len()).map_err(|_| bound())?); nodes.push(final_node);
            canonical.push(Canonical { ty: CtTypeRef { nodes, root }, key });
        }
        let result = canonical.swap_remove(reference.root.0 as usize);
        let output_bytes = add(16, mul(result.ty.nodes.len() as u64, 64)?)?;
        // Keep precisely the VM's returned Type-value charge after scratch dies.
        budget.reserve_temporary_bytes(output_bytes)?;
        Ok(result.ty)
    })();
    drop(canonical);
    budget.release_temporary_bytes(charged);
    result
}
