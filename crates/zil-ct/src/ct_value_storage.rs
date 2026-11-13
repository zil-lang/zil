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
// Complete allocation accounting for descriptor side tables. The generated
// visitor supplies every finite record/variant, boxed payload and actual Vec /
// String capacity. Type references are edges, not recursive permission proofs.
fn catalogue_storage(
    catalogue: &CtTypeCatalogue,
    budget: &mut Option<&mut CtExecutionBudget>,
) -> CompileResult<u64> {
    use zil_ir::ct_artifact::{
        CtDynamicMetadata, CtDynamicTypeId, CtDynamicTypeNode, CtDynamicVisitor,
    };
    struct Meter<'a, 'b> {
        budget: &'b mut Option<&'a mut CtExecutionBudget>,
        copy: bool,
        bytes: u64,
        max_depth: usize,
    }
    impl Meter<'_, '_> {
        fn allocation(&mut self, count: usize, bytes: usize) -> CompileResult<()> {
            self.bytes = self
                .bytes
                .checked_add(
                    count
                        .checked_mul(bytes)
                        .and_then(|n| u64::try_from(n).ok())
                        .ok_or_else(|| limit("CT descriptor allocation overflow"))?,
                )
                .ok_or_else(|| limit("CT descriptor allocation overflow"))?;
            Ok(())
        }
    }
    impl CtDynamicVisitor for Meter<'_, '_> {
        type Error = Vec<Diagnostic>;
        fn node(&mut self, depth: usize) -> CompileResult<()> {
            if depth >= self.max_depth {
                return Err(limit("CT descriptor storage depth exceeded"));
            }
            pay(self.budget, 1)
        }
        fn rows(&mut self, count: usize, element_bytes: usize, depth: usize) -> CompileResult<()> {
            self.node(depth)?;
            pay(self.budget, count)?;
            self.allocation(count, element_bytes)
        }
        fn vector_storage(
            &mut self,
            count: usize,
            capacity: usize,
            bytes: usize,
            depth: usize,
        ) -> CompileResult<()> {
            self.node(depth)?;
            pay(self.budget, count)?;
            self.allocation(if self.copy { count } else { capacity }, bytes)
        }
        fn string_storage(
            &mut self,
            value: &str,
            capacity: usize,
            depth: usize,
        ) -> CompileResult<()> {
            self.node(depth)?;
            pay(self.budget, value.len())?;
            self.allocation(if self.copy { value.len() } else { capacity }, 1)
        }
        fn type_node(
            &mut self,
            _: usize,
            node: &CtDynamicTypeNode,
            depth: usize,
        ) -> CompileResult<()> {
            node.visit_ct_dynamic(self, depth)
        }
        fn type_reference(&mut self, _: CtDynamicTypeId, depth: usize) -> CompileResult<()> {
            self.node(depth)
        }
        fn type_handle(&mut self, value: &CtTypeRef, depth: usize) -> CompileResult<()> {
            value.visit_ct_dynamic(self, depth)
        }
        fn constant(&mut self, value: &Constant, depth: usize) -> CompileResult<()> {
            self.node(depth)?;
            if let Constant::WideString(value) = value {
                self.vector_storage(
                    value.len(),
                    value.capacity(),
                    std::mem::size_of::<u16>(),
                    depth + 1,
                )?;
            }
            Ok(())
        }
        fn text(&mut self, value: &str, depth: usize) -> CompileResult<()> {
            self.node(depth)?;
            pay(self.budget, value.len())?;
            self.allocation(value.len(), 1)
        }
    }
    let copy = budget.is_some();
    let max_depth = budget
        .as_ref()
        .map_or(128, |budget| budget.remaining().max_call_depth as usize)
        .min(128);
    let mut meter = Meter {
        budget,
        copy,
        bytes: 0,
        max_depth,
    };
    catalogue.visit_ct_dynamic(&mut meter, 0)?;
    Ok(meter.bytes)
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
    if let Some(catalogue) = &value.scoped_catalogue {
        bytes = bytes
            .checked_add(catalogue_storage(catalogue, &mut budget)?)
            .ok_or_else(|| limit("CT type descriptor storage overflow"))?;
    }
    for node in &value.nodes {
        bytes = bytes.saturating_add(match node {
            CtTypeNode::ScopedDyn { classes, .. } | CtTypeNode::ScopedTask { classes, .. } => {
                pay(&mut budget, classes.len())?;
                vector(classes, copy)
            }
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

#[cfg(test)]
mod catalogue_tests {
    use super::*;
    use zil_base::{Fingerprint, ResultCategory};
    use zil_ir::ct_artifact::{CtDynamicTypeId, CtDynamicTypeNode, CtStageIdentity};

    fn input() -> CtTypeRef {
        let mut name = String::with_capacity(64);
        name.push_str("abc");
        let mut types = Vec::with_capacity(4);
        types.push(CtDynamicTypeNode::Named(name));
        types.push(CtDynamicTypeNode::Managed(Box::new(CtDynamicTypeId(0))));
        CtTypeRef {
            scoped_catalogue: Some(CtTypeCatalogue {
                stage: CtStageIdentity {
                    source_fingerprint: Fingerprint::of(b"original"),
                    epoch: 7,
                    committed_generation_fingerprint: Fingerprint::of(b"committed"),
                    declaration_inventory_fingerprint: Fingerprint::of(b"declarations"),
                },
                types,
                interfaces: Vec::with_capacity(3),
            }),
            nodes: vec![CtTypeNode::Primitive(CtScalarTy::I32)],
            root: CtTypeNodeId(0),
            ambient_classes: Vec::new(),
        }
    }
    fn allowance(work: u64) -> CtDeclaredBudget {
        CtDeclaredBudget {
            max_steps: 100_000,
            max_call_depth: 128,
            max_live_cells: 100_000,
            max_temporary_bytes: 1_000_000,
            max_generated_nodes: 100_000,
            max_instances: 100_000,
            max_analysis_work: work,
        }
    }
    #[test]
    fn complete_untrusted_catalogues_have_actual_owned_and_copy_buffer_costs() {
        let value = input();
        let table = value.scoped_catalogue.as_ref().unwrap();
        let CtDynamicTypeNode::Named(name) = &table.types[0] else {
            unreachable!()
        };
        let expected_owned = value.nodes.capacity() * std::mem::size_of::<CtTypeNode>()
            + table.types.capacity() * std::mem::size_of::<CtDynamicTypeNode>()
            + table.interfaces.capacity() * std::mem::size_of::<CtDynInterface>()
            + name.capacity()
            + std::mem::size_of::<CtDynamicTypeId>();
        assert_eq!(graph(&value, None).unwrap(), expected_owned as u64);
        let expected_copy = value.nodes.len() * std::mem::size_of::<CtTypeNode>()
            + table.types.len() * std::mem::size_of::<CtDynamicTypeNode>()
            + name.len()
            + std::mem::size_of::<CtDynamicTypeId>();
        let mut budget = CtExecutionBudget::new(allowance(100_000));
        assert_eq!(
            graph(&value, Some(&mut budget)).unwrap(),
            expected_copy as u64
        );
        // The ordinary clone retains even this otherwise unused descriptor
        // table, with the exact allocation length of each cloned buffer.
        let copied = value.clone();
        assert_eq!(copied, value);
        assert_eq!(graph(&copied, None).unwrap(), expected_copy as u64);
    }
    #[test]
    fn catalogue_copy_work_is_bounded_before_clone_and_fresh_after_failure() {
        let value = input();
        let mut measured = CtExecutionBudget::new(allowance(100_000));
        let bytes = graph(&value, Some(&mut measured)).unwrap();
        let exact = measured.usage().analysis_work;
        assert!(exact > value.nodes.len() as u64);
        let mut below = CtExecutionBudget::new(allowance(exact - 1));
        assert!(
            graph(&value, Some(&mut below))
                .unwrap_err()
                .iter()
                .all(|error| error.category == ResultCategory::ResourceLimit)
        );
        for _ in 0..2 {
            let mut fresh = CtExecutionBudget::new(allowance(exact));
            assert_eq!(graph(&value, Some(&mut fresh)).unwrap(), bytes);
            assert_eq!(fresh.remaining().max_analysis_work, 0);
        }
    }
}
