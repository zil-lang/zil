//! Analyze's actual owning frame positions. These paths name responsibility
//! carriers; they never turn a metadata selector into an address or a loan.
use super::*;
use zil_ir::{ContractProjection, PlaceProjection};

pub(super) type Path = Vec<ContractProjection>;
pub(super) type Shape = BTreeMap<Path, bool>;
pub(super) struct Catalog {
    pub(super) values: Vec<Shape>,
    pub(super) locals: Vec<Shape>,
    pub(super) captures: Vec<Shape>,
}
fn shape(module: &TypedModule, ty: &Type, b: &mut Budget) -> CompileResult<Shape> {
    let mut result = Shape::new();
    let mut pending = vec![(ty, Path::new(), 0usize)];
    while let Some((ty, path, depth)) = pending.pop() {
        fee(b, path.len().saturating_add(1))?;
        if depth > b.partial_depth {
            return Err(bad("Task owning carrier graph exceeds finite depth"));
        }
        match ty {
            Type::Managed(payload) if matches!(payload.as_ref(), Type::Task { .. }) => {
                let Type::Task { retained_life, .. } = payload.as_ref() else {
                    unreachable!()
                };
                fee(b, retained_life.ports.len())?;
                if !retained_life.ports.is_empty() {
                    result.insert(
                        path,
                        retained_life
                            .ports
                            .iter()
                            .any(|port| port.mode == zil_ir::StoredBorrowMode::ExclusiveAddress),
                    );
                }
            }
            Type::Closure(id) => {
                let definition = module
                    .closures
                    .get(id.0 as usize)
                    .ok_or_else(|| bad("Task carrier closure definition absent"))?;
                fee(
                    b,
                    definition
                        .captures
                        .len()
                        .saturating_mul(path.len().saturating_add(2)),
                )?;
                for (index, child) in definition.captures.iter().enumerate() {
                    let mut selected = path.clone();
                    selected.push(ContractProjection::Capture(index as u32));
                    pending.push((child, selected, depth + 1));
                }
            }
            Type::BorrowedStorage { storage, .. } => pending.push((storage, path, depth + 1)),
            Type::Tuple(fields) => {
                fee(b, fields.len().saturating_mul(path.len().saturating_add(2)))?;
                for (index, child) in fields.iter().enumerate() {
                    let mut selected = path.clone();
                    selected.push(ContractProjection::Place(PlaceProjection::Field(
                        index as u32,
                    )));
                    pending.push((child, selected, depth + 1));
                }
            }
            Type::Package { fields, .. } | Type::Record { fields, .. } => {
                fee(b, fields.len().saturating_mul(path.len().saturating_add(2)))?;
                for (index, (_, child)) in fields.iter().enumerate() {
                    let mut selected = path.clone();
                    selected.push(ContractProjection::Place(PlaceProjection::Field(
                        index as u32,
                    )));
                    pending.push((child, selected, depth + 1));
                }
            }
            Type::ErrorUnion { error, result } => {
                fee(b, path.len().saturating_add(2).saturating_mul(2))?;
                for (arm, child) in [(true, error.as_ref()), (false, result.as_ref())] {
                    let mut selected = path.clone();
                    selected.push(ContractProjection::Place(PlaceProjection::ErrorArm(arm)));
                    pending.push((child, selected, depth + 1));
                }
            }
            Type::Enum { variants, .. } => {
                fee(b, variants.len())?;
                for (variant, (_, fields)) in variants.iter().enumerate() {
                    fee(b, fields.len().saturating_mul(path.len().saturating_add(2)))?;
                    for (index, child) in fields.iter().enumerate() {
                        let mut selected = path.clone();
                        selected.push(ContractProjection::Place(PlaceProjection::Variant(
                            zil_ir::VariantProjection {
                                variant: variant as u32,
                                index: index as u32,
                            },
                        )));
                        pending.push((child, selected, depth + 1));
                    }
                }
            }
            // Addresses/Desc do not own their pointee frame. Its existing
            // actual target and parent loans remain in the stored solver.
            _ => {}
        }
    }
    Ok(result)
}
impl Catalog {
    pub(super) fn new(
        module: &TypedModule,
        f: &TypedFunction,
        b: &mut Budget,
    ) -> CompileResult<Self> {
        fee(b, f.values.len().saturating_add(f.locals.len()))?;
        let mut values = Vec::with_capacity(f.values.len());
        for ty in &f.values {
            values.push(shape(module, ty, b)?);
        }
        let mut locals = Vec::with_capacity(f.locals.len());
        for local in &f.locals {
            locals.push(shape(module, &local.ty, b)?);
        }
        let mut captures = Vec::new();
        if let Some(id) = f.environment {
            let definition = module
                .closures
                .get(id.0 as usize)
                .ok_or_else(|| bad("Task frame entry lacks actual captures"))?;
            fee(b, definition.captures.len())?;
            for ty in &definition.captures {
                captures.push(shape(module, ty, b)?);
            }
        }
        Ok(Self {
            values,
            locals,
            captures,
        })
    }
}
