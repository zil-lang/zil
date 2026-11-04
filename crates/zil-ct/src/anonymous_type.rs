//! Complete, bounded constructor identity for anonymous CT sums.
//! Keys contain actual structure and nominal identities, never DAG IDs or hashes.
use crate::CtExecutionBudget;
use zil_base::{CompileResult, Diagnostic, Phase};
use zil_ir::{Constant, ct_artifact::*};

fn bad(message: &str) -> Vec<Diagnostic> {
    vec![Diagnostic::error(Phase::Ct, "E-CT-ARTIFACT", None, message)]
}
fn bound() -> Vec<Diagnostic> {
    vec![Diagnostic::limit(
        Phase::Ct,
        "CT anonymous type identity budget exhausted",
        None,
    )]
}
fn add(a: u64, b: u64) -> CompileResult<u64> {
    a.checked_add(b).ok_or_else(bound)
}
fn mul(a: u64, b: u64) -> CompileResult<u64> {
    a.checked_mul(b).ok_or_else(bound)
}

pub(crate) fn children(node: &CtTypeNode) -> Vec<CtTypeNodeId> {
    match node {
        CtTypeNode::BorrowedStorage { storage, .. } => vec![*storage],
        CtTypeNode::Nominal { arguments, .. } => arguments
            .iter()
            .filter_map(|a| match a {
                CtTypeArgument::Type(id) => Some(*id),
                _ => None,
            })
            .collect(),
        CtTypeNode::Tuple(v) | CtTypeNode::AnonymousSum(v) => v.clone(),
        CtTypeNode::FunctionPointer { parameters, result }
        | CtTypeNode::BorrowedFunctionPointer {
            parameters, result, ..
        }
        | CtTypeNode::ForeignCallback { parameters, result } => {
            parameters.iter().copied().chain([*result]).collect()
        }
        CtTypeNode::Array { element, .. }
        | CtTypeNode::Slice(element)
        | CtTypeNode::Managed(element)
        | CtTypeNode::Desc(element)
        | CtTypeNode::OwnedStore(element) => vec![*element],
        CtTypeNode::Address { inner, .. } => vec![*inner],
        CtTypeNode::ErrorUnion { error, result } => vec![*error, *result],
        _ => Vec::new(),
    }
}

pub(crate) fn remap(
    node: &CtTypeNode,
    mut map: impl FnMut(CtTypeNodeId) -> CompileResult<CtTypeNodeId>,
) -> CompileResult<CtTypeNode> {
    Ok(match node {
        CtTypeNode::BorrowedStorage { .. } => {
            return Err(vec![Diagnostic::unsupported(
                Phase::Ct,
                "stored-borrow CT remapping is not yet implemented",
                None,
            )]);
        }
        CtTypeNode::Primitive(s) => CtTypeNode::Primitive(*s),
        CtTypeNode::Builtin(s) => CtTypeNode::Builtin(*s),
        CtTypeNode::Opaque { identity } => CtTypeNode::Opaque {
            identity: identity.clone(),
        },
        CtTypeNode::Nominal { symbol, arguments } => CtTypeNode::Nominal {
            symbol: symbol.clone(),
            arguments: arguments
                .iter()
                .map(|a| match a {
                    CtTypeArgument::Type(id) => map(*id).map(CtTypeArgument::Type),
                    CtTypeArgument::Scalar(c) => Ok(CtTypeArgument::Scalar(c.clone())),
                    CtTypeArgument::Lifetime(_) => Err(vec![Diagnostic::unsupported(Phase::Ct, "CT4 lifetime owner relocation is not yet implemented", None)]),
                })
                .collect::<CompileResult<_>>()?,
        },
        CtTypeNode::Tuple(v) => CtTypeNode::Tuple(
            v.iter()
                .copied()
                .map(&mut map)
                .collect::<CompileResult<_>>()?,
        ),
        CtTypeNode::AnonymousSum(v) => CtTypeNode::AnonymousSum(
            v.iter()
                .copied()
                .map(&mut map)
                .collect::<CompileResult<_>>()?,
        ),
        CtTypeNode::Array { element, length } => CtTypeNode::Array {
            element: map(*element)?,
            length: *length,
        },
        CtTypeNode::BorrowedFunctionPointer {
            parameters,
            result,
            life,
        } => CtTypeNode::BorrowedFunctionPointer {
            parameters: parameters
                .iter()
                .copied()
                .map(&mut map)
                .collect::<CompileResult<_>>()?,
            result: map(*result)?,
            life: life.clone(),
        },
        CtTypeNode::FunctionPointer { parameters, result } => CtTypeNode::FunctionPointer {
            parameters: parameters
                .iter()
                .copied()
                .map(&mut map)
                .collect::<CompileResult<_>>()?,
            result: map(*result)?,
        },
        CtTypeNode::ForeignCallback { parameters, result } => CtTypeNode::ForeignCallback {
            parameters: parameters
                .iter()
                .copied()
                .map(&mut map)
                .collect::<CompileResult<_>>()?,
            result: map(*result)?,
        },
        CtTypeNode::Address { mutable, inner } => CtTypeNode::Address {
            mutable: *mutable,
            inner: map(*inner)?,
        },
        CtTypeNode::Slice(id) => CtTypeNode::Slice(map(*id)?),
        CtTypeNode::Managed(id) => CtTypeNode::Managed(map(*id)?),
        CtTypeNode::Desc(id) => CtTypeNode::Desc(map(*id)?),
        CtTypeNode::OwnedStore(id) => CtTypeNode::OwnedStore(map(*id)?),
        CtTypeNode::ErrorUnion { error, result } => CtTypeNode::ErrorUnion {
            error: map(*error)?,
            result: map(*result)?,
        },
    })
}

struct Canonical {
    ty: CtTypeRef,
    key: Vec<u8>,
    depth: u32,
}
struct Key<'a> {
    out: Vec<u8>,
    budget: &'a mut CtExecutionBudget,
}
impl Key<'_> {
    fn bytes(&mut self, bytes: &[u8]) -> CompileResult<()> {
        self.budget.reserve_analysis_work(bytes.len() as u64)?;
        self.budget.reserve_temporary_bytes(bytes.len() as u64)?;
        self.out.extend_from_slice(bytes);
        Ok(())
    }
    fn number(&mut self, value: u64) -> CompileResult<()> {
        self.bytes(&value.to_be_bytes())
    }
    fn symbol(&mut self, s: &CtSymbolId) -> CompileResult<()> {
        self.number(u64::from(s.module.0))?;
        self.number(u64::from(s.declaration.0))?;
        self.bytes(&[u8::from(s.generation_path.is_some())])?;
        if let Some(fp) = s.generation_path {
            self.bytes(&fp.0)?;
        }
        Ok(())
    }
    fn life(&mut self, life: &zil_ir::FunctionLifeContract) -> CompileResult<()> {
        self.number(u64::from(life.binder_count))?;
        self.number(life.parameter_lives.len() as u64)?;
        for label in &life.parameter_lives {
            self.bytes(&[u8::from(label.is_some())])?;
            if let Some(label) = label {
                self.number(u64::from(*label))?;
            }
        }
        self.bytes(&[u8::from(life.result_life.is_some())])?;
        if let Some(label) = life.result_life {
            self.number(u64::from(label))?;
        }
        Ok(())
    }
    fn scalar(&mut self, c: &Constant) -> CompileResult<()> {
        let (tag, bits) = match c {
            Constant::Unit => (0, 0),
            Constant::Bool(b) => (1, u64::from(*b)),
            Constant::U8(v) => (2, u64::from(*v)),
            Constant::U16(v) => (3, u64::from(*v)),
            Constant::I32(v) => (4, *v as u32 as u64),
            Constant::I64(v) => (5, *v as u64),
            Constant::U64(v) => (6, *v),
            Constant::Usize(v) => (7, *v),
            Constant::F32(v) => (8, u64::from(*v)),
            Constant::F64(v) => (9, *v),
            _ => return Err(bad("CT type argument is not a scalar")),
        };
        self.bytes(&[tag])?;
        self.number(bits)
    }
}

/// Rebuild reachable constructors without incidental DAG sharing or numbering.
/// Temporary expansion and every full-key comparison are charged before copying.
/// This is a data transformation; it grants no nominal visibility or shape access.
pub fn canonicalize_anonymous_type(
    reference: &CtTypeRef,
    budget: &mut CtExecutionBudget,
) -> CompileResult<CtTypeRef> {
    canonical_type_key_mode(reference, budget, false).map(|(ty, _)| ty)
}

/// Full structural key and normalized graph. Both outputs remain charged until
/// their caller drops them; this supplies data identity, never source access.
pub(crate) fn canonical_type_key(
    reference: &CtTypeRef,
    budget: &mut CtExecutionBudget,
) -> CompileResult<(CtTypeRef, Vec<u8>)> {
    canonical_type_key_mode(reference, budget, true)
}
fn canonical_type_key_mode(
    reference: &CtTypeRef,
    budget: &mut CtExecutionBudget,
    keep_key: bool,
) -> CompileResult<(CtTypeRef, Vec<u8>)> {
    if reference.root.0 as usize >= reference.nodes.len() {
        return Err(bad("CT type root is outside its graph"));
    }
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
                CtTypeNode::FunctionPointer { parameters, .. }
                | CtTypeNode::BorrowedFunctionPointer { parameters, .. }
                | CtTypeNode::ForeignCallback { parameters, .. } => {
                    parameters.len().saturating_add(1)
                }
                _ => 2,
            } as u64;
            budget.reserve_analysis_work(edge_count)?;
            if let CtTypeNode::Nominal { arguments, .. } = node
                && arguments.iter().any(|a| {
                    matches!(
                        a,
                        CtTypeArgument::Scalar(
                            Constant::NullHandle
                                | Constant::NullWideString
                                | Constant::WideString(_)
                        )
                    )
                })
            {
                return Err(bad("CT type argument is not a pure scalar"));
            }
            let life_bytes = if let CtTypeNode::BorrowedFunctionPointer { life, .. } = node {
                let slots = life.parameter_lives.len() as u64;
                budget.reserve_analysis_work(add(slots, 3)?)?;
                add(mul(slots, std::mem::size_of::<Option<u32>>() as u64)?, 32)?
            } else {
                0
            };
            let edge_bytes = add(mul(edge_count, 64)?, life_bytes)?;
            budget.reserve_temporary_bytes(edge_bytes)?;
            charged = add(charged, edge_bytes)?;
            let mut ids = children(node);
            if ids.iter().any(|id| id.0 as usize >= index) {
                return Err(bad("CT type child is not earlier in the graph"));
            }
            if matches!(node, CtTypeNode::AnonymousSum(_)) {
                if ids.len() < 2 {
                    return Err(bad("anonymous sum needs at least two members"));
                }
                let mut flat = Vec::new();
                while let Some(id) = ids.pop() {
                    budget.reserve_analysis_work(1)?;
                    if let CtTypeNode::AnonymousSum(nested) = &reference.nodes[id.0 as usize] {
                        let bytes = mul(nested.len() as u64, 8)?;
                        budget.reserve_temporary_bytes(bytes)?;
                        charged = add(charged, bytes)?;
                        budget.reserve_analysis_work(nested.len() as u64)?;
                        ids.extend(nested.iter().copied());
                    } else {
                        budget.reserve_temporary_bytes(8)?;
                        charged = add(charged, 8)?;
                        flat.push(id);
                    }
                }
                let compare = flat.iter().try_fold(0u64, |n, id| {
                    add(n, canonical[id.0 as usize].key.len() as u64)
                })?;
                budget.reserve_analysis_work(mul(compare, add(flat.len() as u64, 1)?)?)?;
                flat.sort_by(|a, b| {
                    canonical[a.0 as usize]
                        .key
                        .cmp(&canonical[b.0 as usize].key)
                });
                if flat
                    .windows(2)
                    .any(|v| canonical[v[0].0 as usize].key == canonical[v[1].0 as usize].key)
                {
                    return Err(bad(
                        "anonymous sum contains duplicate complete member types",
                    ));
                }
                ids = flat;
            }
            let output_nodes = ids.iter().try_fold(1u64, |n, id| {
                add(n, canonical[id.0 as usize].ty.nodes.len() as u64)
            })?;
            let child_key_bytes = ids.iter().try_fold(0u64, |n, id| {
                add(n, canonical[id.0 as usize].key.len() as u64)
            })?;
            let bytes = add(mul(output_nodes, 128)?, mul(child_key_bytes, 8)?)?;
            budget.reserve_analysis_work(add(output_nodes, child_key_bytes)?)?;
            let depth = ids
                .iter()
                .map(|id| canonical[id.0 as usize].depth)
                .max()
                .unwrap_or(0)
                .saturating_add(1);
            if depth > budget.remaining().max_call_depth {
                return Err(bound());
            }
            budget.reserve_generated_nodes(output_nodes)?;
            budget.reserve_temporary_bytes(bytes)?;
            charged = add(charged, bytes)?;
            let mut nodes = Vec::new();
            let mut mapped = Vec::new();
            for id in &ids {
                let child = &canonical[id.0 as usize].ty;
                let offset = u32::try_from(nodes.len()).map_err(|_| bound())?;
                for child_node in &child.nodes {
                    nodes.push(remap(child_node, |old| {
                        old.0
                            .checked_add(offset)
                            .map(CtTypeNodeId)
                            .ok_or_else(bound)
                    })?);
                }
                mapped.push(CtTypeNodeId(
                    child.root.0.checked_add(offset).ok_or_else(bound)?,
                ));
            }
            let mut key = Key {
                out: Vec::new(),
                budget,
            };
            let tag = match node {
                CtTypeNode::BorrowedStorage { .. } => {
                    return Err(vec![Diagnostic::unsupported(
                        Phase::Ct,
                        "stored-borrow CT identity is not yet implemented",
                        None,
                    )]);
                }
                CtTypeNode::Primitive(_) => 0,
                CtTypeNode::Builtin(_) => 1,
                CtTypeNode::Nominal { .. } => 2,
                CtTypeNode::Tuple(_) => 3,
                CtTypeNode::Array { .. } => 4,
                CtTypeNode::FunctionPointer { .. } => 5,
                CtTypeNode::Address { .. } => 6,
                CtTypeNode::Slice(_) => 7,
                CtTypeNode::Managed(_) => 8,
                CtTypeNode::Desc(_) => 9,
                CtTypeNode::OwnedStore(_) => 10,
                CtTypeNode::ForeignCallback { .. } => 11,
                CtTypeNode::Opaque { .. } => 12,
                CtTypeNode::ErrorUnion { .. } => 13,
                CtTypeNode::BorrowedFunctionPointer { .. } => 14,
                CtTypeNode::AnonymousSum(_) => 15,
            };
            key.bytes(&[tag])?;
            match node {
                CtTypeNode::Primitive(s) => key.bytes(&[*s as u8])?,
                CtTypeNode::Builtin(b) => key.bytes(&[*b as u8])?,
                CtTypeNode::Nominal { symbol, arguments } => {
                    key.symbol(symbol)?;
                    key.number(arguments.len() as u64)?;
                    let mut child = ids.iter();
                    for arg in arguments {
                        match arg {
                            CtTypeArgument::Type(_) => {
                                key.bytes(&[0])?;
                                let c =
                                    &canonical[child.next().expect("type argument").0 as usize].key;
                                key.number(c.len() as u64)?;
                                key.bytes(c)?;
                            }
                            CtTypeArgument::Lifetime(_) => return Err(vec![Diagnostic::unsupported(Phase::Ct, "CT4 lifetime identity is not yet implemented", None)]),
                            CtTypeArgument::Scalar(c) => {
                                key.bytes(&[1])?;
                                key.scalar(c)?;
                            }
                        }
                    }
                }
                CtTypeNode::BorrowedFunctionPointer { life, .. } => key.life(life)?,
                CtTypeNode::Opaque { identity } => key.symbol(identity)?,
                CtTypeNode::Array { length, .. } => key.number(u64::from(*length))?,
                CtTypeNode::Address { mutable, .. } => key.bytes(&[u8::from(*mutable)])?,
                _ => {}
            }
            if !matches!(node, CtTypeNode::Nominal { .. }) {
                key.number(ids.len() as u64)?;
                for id in &ids {
                    let c = &canonical[id.0 as usize].key;
                    key.number(c.len() as u64)?;
                    key.bytes(c)?;
                }
            }
            let key = key.out;
            charged = add(charged, key.len() as u64)?;
            let final_node = if matches!(node, CtTypeNode::AnonymousSum(_)) {
                CtTypeNode::AnonymousSum(mapped)
            } else {
                let mut next = mapped.into_iter();
                remap(node, |_| Ok(next.next().expect("mapped child")))?
            };
            let root = CtTypeNodeId(u32::try_from(nodes.len()).map_err(|_| bound())?);
            nodes.push(final_node);
            canonical.push(Canonical {
                ty: CtTypeRef { nodes, root },
                key,
                depth,
            });
        }
        let result = canonical.swap_remove(reference.root.0 as usize);
        let output_bytes = add(16, mul(result.ty.nodes.len() as u64, 64)?)?;
        // Keep precisely the VM's returned Type-value charge after scratch dies.
        budget.reserve_temporary_bytes(add(
            output_bytes,
            if keep_key { result.key.len() as u64 } else { 0 },
        )?)?;
        let key = if keep_key { result.key } else { Vec::new() };
        Ok((result.ty, key))
    })();
    drop(canonical);
    budget.release_temporary_bytes(charged);
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use zil_base::{ModuleId, NodeId};
    fn budget() -> CtExecutionBudget {
        CtExecutionBudget::new(CtDeclaredBudget {
            max_steps: 1000,
            max_call_depth: 64,
            max_live_cells: 1000,
            max_temporary_bytes: 1_000_000,
            max_generated_nodes: 100_000,
            max_instances: 1000,
            max_analysis_work: 1_000_000,
        })
    }
    fn graph(nodes: Vec<CtTypeNode>) -> CtTypeRef {
        CtTypeRef {
            root: CtTypeNodeId(nodes.len() as u32 - 1),
            nodes,
        }
    }
    #[test]
    fn anonymous_type_identity_ignores_graph_numbering_sharing_and_member_order() {
        let a = graph(vec![
            CtTypeNode::Primitive(CtScalarTy::I32),
            CtTypeNode::Primitive(CtScalarTy::Bool),
            CtTypeNode::Tuple(vec![CtTypeNodeId(0), CtTypeNodeId(0)]),
            CtTypeNode::AnonymousSum(vec![CtTypeNodeId(2), CtTypeNodeId(1)]),
        ]);
        let b = graph(vec![
            CtTypeNode::Primitive(CtScalarTy::Bool),
            CtTypeNode::Primitive(CtScalarTy::I32),
            CtTypeNode::Primitive(CtScalarTy::I32),
            CtTypeNode::Tuple(vec![CtTypeNodeId(1), CtTypeNodeId(2)]),
            CtTypeNode::AnonymousSum(vec![CtTypeNodeId(0), CtTypeNodeId(3)]),
        ]);
        let a = canonicalize_anonymous_type(&a, &mut budget()).unwrap();
        let b = canonicalize_anonymous_type(&b, &mut budget()).unwrap();
        assert_eq!(a, b);
        assert_eq!(canonicalize_anonymous_type(&a, &mut budget()).unwrap(), a);
    }
    #[test]
    fn anonymous_type_identity_keeps_nominal_phantom_arguments_and_rejects_duplicates() {
        let symbol = CtSymbolId {
            module: ModuleId(4),
            declaration: NodeId(7),
            generation_path: None,
        };
        let mut nodes = vec![
            CtTypeNode::Primitive(CtScalarTy::I32),
            CtTypeNode::Primitive(CtScalarTy::Bool),
        ];
        for id in [0, 1] {
            nodes.push(CtTypeNode::Nominal {
                symbol: symbol.clone(),
                arguments: vec![CtTypeArgument::Type(CtTypeNodeId(id))],
            });
        }
        nodes.push(CtTypeNode::AnonymousSum(vec![
            CtTypeNodeId(2),
            CtTypeNodeId(3),
        ]));
        assert!(canonicalize_anonymous_type(&graph(nodes.clone()), &mut budget()).is_ok());
        nodes[3] = nodes[2].clone();
        assert!(canonicalize_anonymous_type(&graph(nodes), &mut budget()).is_err());
        let nested = graph(vec![
            CtTypeNode::Primitive(CtScalarTy::I32),
            CtTypeNode::Primitive(CtScalarTy::Bool),
            CtTypeNode::AnonymousSum(vec![CtTypeNodeId(0), CtTypeNodeId(1)]),
            CtTypeNode::AnonymousSum(vec![CtTypeNodeId(2), CtTypeNodeId(0)]),
        ]);
        assert!(canonicalize_anonymous_type(&nested, &mut budget()).is_err());
    }
    #[test]
    fn anonymous_type_expansion_and_exact_work_are_bounded_before_copying() {
        let mut nodes = vec![CtTypeNode::Primitive(CtScalarTy::I32)];
        for id in 0..24 {
            nodes.push(CtTypeNode::Tuple(vec![CtTypeNodeId(id), CtTypeNodeId(id)]));
        }
        let errors = canonicalize_anonymous_type(&graph(nodes), &mut budget()).unwrap_err();
        assert_eq!(errors[0].category, zil_base::ResultCategory::ResourceLimit);
        let simple = graph(vec![
            CtTypeNode::Primitive(CtScalarTy::I32),
            CtTypeNode::Primitive(CtScalarTy::Bool),
            CtTypeNode::AnonymousSum(vec![CtTypeNodeId(0), CtTypeNodeId(1)]),
        ]);
        let mut first = budget();
        canonicalize_anonymous_type(&simple, &mut first).unwrap();
        let exact = first.usage().analysis_work;
        for (work, good) in [(exact, true), (exact - 1, false), (exact, true)] {
            let initial = budget();
            let mut declared = initial.remaining();
            declared.max_analysis_work = work;
            let mut meter = CtExecutionBudget::new(declared);
            assert_eq!(
                canonicalize_anonymous_type(&simple, &mut meter).is_ok(),
                good
            );
        }
    }
    #[test]
    fn anonymous_type_borrowed_code_keeps_complete_life_and_signature_identity() {
        use zil_ir::FunctionLifeContract;
        let one = FunctionLifeContract {
            binder_count: 1,
            parameter_lives: vec![Some(0), Some(0)],
            result_life: Some(0),
        };
        let two = FunctionLifeContract {
            binder_count: 2,
            parameter_lives: vec![Some(0), Some(1)],
            result_life: Some(0),
        };
        let input = graph(vec![
            CtTypeNode::Primitive(CtScalarTy::I32),
            CtTypeNode::Address {
                mutable: true,
                inner: CtTypeNodeId(0),
            },
            CtTypeNode::BorrowedFunctionPointer {
                parameters: vec![CtTypeNodeId(1), CtTypeNodeId(1)],
                result: CtTypeNodeId(1),
                life: one.clone(),
            },
            CtTypeNode::BorrowedFunctionPointer {
                parameters: vec![CtTypeNodeId(1), CtTypeNodeId(1)],
                result: CtTypeNodeId(1),
                life: two.clone(),
            },
            CtTypeNode::AnonymousSum(vec![CtTypeNodeId(3), CtTypeNodeId(2)]),
        ]);
        let mut initial = budget();
        let canonical = canonicalize_anonymous_type(&input, &mut initial).unwrap();
        let exact = initial.usage().analysis_work;
        for (work, good) in [(exact, true), (exact - 1, false), (exact, true)] {
            let mut declared = budget().remaining();
            declared.max_analysis_work = work;
            let mut meter = CtExecutionBudget::new(declared);
            assert_eq!(
                canonicalize_anonymous_type(&input, &mut meter).is_ok(),
                good
            );
        }
        let CtTypeNode::AnonymousSum(members) = &canonical.nodes[canonical.root.0 as usize] else {
            unreachable!()
        };
        assert_eq!(members.len(), 2);
        for member in members {
            let CtTypeNode::BorrowedFunctionPointer {
                parameters,
                result,
                life,
            } = &canonical.nodes[member.0 as usize]
            else {
                panic!("borrowed code degraded")
            };
            assert!(life == &one || life == &two);
            assert_eq!(parameters.len(), 2);
            for id in parameters.iter().chain([result]) {
                let CtTypeNode::Address { mutable, inner } = &canonical.nodes[id.0 as usize] else {
                    panic!("signature child lost")
                };
                assert!(*mutable);
                assert_eq!(
                    canonical.nodes[inner.0 as usize],
                    CtTypeNode::Primitive(CtScalarTy::I32)
                );
            }
        }
        let mut equal = input.clone();
        let CtTypeNode::BorrowedFunctionPointer { life, .. } = &mut equal.nodes[3] else {
            unreachable!()
        };
        *life = one;
        assert!(canonicalize_anonymous_type(&equal, &mut budget()).is_err());
    }
}
