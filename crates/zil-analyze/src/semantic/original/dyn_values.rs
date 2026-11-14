//! Reify the original complete Dyn key. This is an identity query only: the
//! table, entries, implementation and actual loan effects need other proofs.
use super::*;
use crate::semantic::value::{Argument, Class, Value};
fn associated_type_work(row: &DynAssociatedType, b: &mut Budget) -> CompileResult<()> {
    crate::dyn_metadata::recipe(&row.member.application, b)?;
    crate::dyn_metadata::ty(&row.ty, b)?;
    crate::dyn_metadata::shape(&row.life, b)?;
    b.spend(row.classes.len().saturating_add(1), WORK)
}
fn associated_constant_work(row: &DynAssociatedConstant, b: &mut Budget) -> CompileResult<()> {
    crate::dyn_metadata::recipe(&row.member.application, b)?;
    crate::dyn_metadata::ty(&row.ty, b)?;
    b.spend(
        match &row.value {
            Constant::WideString(units) => units.len().saturating_add(1),
            _ => 1,
        },
        WORK,
    )
}
impl Checked {
    fn dyn_fixed(
        &self,
        class: &Class,
        domain: &[Class],
        b: &mut Budget,
    ) -> CompileResult<StorageBinder> {
        let mut selected = None;
        for (index, image) in domain.iter().enumerate() {
            b.spend(1, WORK)?;
            if image == class {
                if selected.replace(index).is_some() {
                    return Err(unknown(
                        "original Dyn class needs its intrinsic origin selector before equal images can be transported",
                    ));
                }
            }
        }
        Ok(StorageBinder::Fixed(
            u32::try_from(selected.ok_or_else(|| {
                fail("original Dyn actual class is outside its complete Context")
            })?)
            .map_err(|_| fail("original Dyn Context class ordinal overflows"))?,
        ))
    }
    fn dyn_actuals(
        &self,
        terms: &[Term],
        domain: &[Class],
        context: &SemanticInstance,
        program: &SemanticModule,
        inventory: &Inventory<'_>,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Vec<DynGenericActual>> {
        let mut result = Vec::new();
        for term in terms {
            let actual = match self.argument(*term, context, program, inventory, b, depth + 1)? {
                Argument::Type(value) => {
                    if value.classes.is_empty() {
                        DynGenericActual::Type(value.ty)
                    } else {
                        let mut classes = Vec::new();
                        for class in &value.classes {
                            let fixed = self.dyn_fixed(class, domain, b)?;
                            Solver::push(&mut classes, fixed, b)?;
                        }
                        DynGenericActual::QualifiedType(DynTypeValue {
                            ty: value.ty,
                            life: value.life,
                            classes,
                        })
                    }
                }
                Argument::Life(class) => DynGenericActual::Life(self.dyn_fixed(&class, domain, b)?),
                Argument::Const { carrier, value } => {
                    crate::dyn_metadata::ty(&carrier, b)?;
                    if value.ty() != carrier {
                        return Err(fail("original Dyn constant changes its complete carrier"));
                    }
                    DynGenericActual::Const(value)
                }
            };
            Solver::push(&mut result, actual, b)?;
        }
        Ok(result)
    }
    pub(super) fn dyn_value(
        &self,
        ty: &dyn_types::DynType,
        context: &SemanticInstance,
        program: &SemanticModule,
        inventory: &Inventory<'_>,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Value> {
        b.spend(ty.work(), WORK)?;
        if depth > b.partial_depth {
            return Err(unknown(
                "original Dyn value exceeds its finite complete scope depth",
            ));
        }
        let (retained, classes) =
            self.dyn_context_value(ty, context, program, inventory, b, depth + 1)?;
        if retained.binder_count as usize != classes.len() {
            return Err(fail(
                "original Dyn Context omits an intrinsic class occurrence",
            ));
        }
        let application = DynTraitApplication {
            declaration: ty.declaration,
            actuals: self.dyn_actuals(
                &ty.arguments,
                &classes,
                context,
                program,
                inventory,
                b,
                depth + 1,
            )?,
        };
        let expected_context = match &ty.context {
            dyn_types::Context::Closed => Some(None),
            dyn_types::Context::Type(term) => Some(Some(
                self.value(*term, context, program, inventory, b, depth + 1)?
                    .ty,
            )),
            dyn_types::Context::Shape(_) => None,
        };
        let mut associated_types = Vec::new();
        let mut associated_constants = Vec::new();
        for row in &ty.associated {
            let member = DynMember {
                application: DynTraitApplication {
                    declaration: row.declaration,
                    actuals: self.dyn_actuals(
                        &row.arguments,
                        &classes,
                        context,
                        program,
                        inventory,
                        b,
                        depth + 1,
                    )?,
                },
                ordinal: row.member,
            };
            match &row.value {
                dyn_types::AssociatedValue::Type(term) => {
                    let value = self.value(*term, context, program, inventory, b, depth + 1)?;
                    let mut mapped = Vec::new();
                    for class in &value.classes {
                        let fixed = self.dyn_fixed(class, &classes, b)?;
                        Solver::push(&mut mapped, fixed, b)?;
                    }
                    Solver::push(
                        &mut associated_types,
                        DynAssociatedType {
                            member,
                            ty: value.ty,
                            life: value.life,
                            classes: mapped,
                        },
                        b,
                    )?;
                }
                dyn_types::AssociatedValue::Const { carrier, value } => {
                    let carrier =
                        self.value(*carrier, context, program, inventory, b, depth + 1)?;
                    let Argument::Const {
                        carrier: actual,
                        value,
                    } = self.argument(*value, context, program, inventory, b, depth + 1)?
                    else {
                        return Err(fail(
                            "original Dyn associated constant has another actual kind",
                        ));
                    };
                    if !carrier.classes.is_empty() || carrier.ty != actual {
                        return Err(fail(
                            "original Dyn associated constant changes its complete carrier",
                        ));
                    }
                    Solver::push(
                        &mut associated_constants,
                        DynAssociatedConstant {
                            member,
                            ty: actual,
                            value,
                        },
                        b,
                    )?;
                }
            }
        }
        let mut selected = None;
        for interface in &inventory.module.dyn_interfaces {
            crate::dyn_metadata::recipe(&interface.application, b)?;
            crate::dyn_metadata::retained_shape(&interface.retained_life, b)?;
            if let Some(context) = &interface.context {
                crate::dyn_metadata::ty(context, b)?;
            }
            for row in &interface.associated_types {
                associated_type_work(row, b)?;
            }
            for row in &interface.associated_constants {
                associated_constant_work(row, b)?;
            }
            crate::dyn_metadata::recipe(&application, b)?;
            crate::dyn_metadata::retained_shape(&retained, b)?;
            for row in &associated_types {
                associated_type_work(row, b)?;
            }
            for row in &associated_constants {
                associated_constant_work(row, b)?;
            }
            if interface.application != application
                || interface.retained_life != retained
                || interface.associated_types != associated_types
                || interface.associated_constants != associated_constants
                || expected_context
                    .as_ref()
                    .is_some_and(|expected| interface.context.as_ref() != expected.as_ref())
            {
                continue;
            }
            if selected.replace(interface.id).is_some() {
                return Err(unknown(
                    "original Dyn type has multiple complete actual interface keys and needs its exact occurrence relocation",
                ));
            }
        }
        let interface = selected.ok_or_else(|| {
            fail("original Dyn type has no exact actual application, Context and associated key")
        })?;
        Ok(Value {
            ty: if ty.owned {
                Type::OwnedDyn { interface }
            } else {
                Type::Dyn { interface }
            },
            life: StorageLifeShape {
                binder_count: retained.binder_count,
                slots: Vec::new(),
                retained: Vec::new(),
                nominal_arguments: Vec::new(),
            },
            classes,
        })
    }
}
