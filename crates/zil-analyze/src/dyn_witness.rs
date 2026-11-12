//! Original declaration recipes and actual implementation witnesses. This
//! module owns no runtime target/loan fact and never consumes a candidate plan.
use crate::{bad, budget::Budget};
use zil_base::CompileResult;
use zil_ir::*;
mod declarations;
mod values;
const WORK: &str = "dynamic original declaration witness";
fn fail(message: &str) -> Vec<zil_base::Diagnostic> {
    bad(format!("dynamic declaration proof: {message}"))
}
fn limit(message: &str) -> Vec<zil_base::Diagnostic> {
    vec![zil_base::Diagnostic::limit(
        zil_base::Phase::Analyze,
        message,
        None,
    )]
}

pub(super) fn declaration(
    module: &TypedModule,
    declaration: &DynDeclaration,
    b: &mut Budget,
) -> CompileResult<()> {
    declarations::validate(module, declaration, b)
}

pub(super) fn receiver(
    module: &TypedModule,
    table: &DynVtable,
    b: &mut Budget,
) -> CompileResult<()> {
    let life = table
        .self_life
        .as_ref()
        .ok_or_else(|| fail("provider's complete actual Self domain is Unknown"))?;
    b.spend(module.dyn_interfaces.len(), WORK)?;
    let mut matches = module
        .dyn_interfaces
        .iter()
        .filter(|i| i.id == table.interface);
    let interface = matches
        .next()
        .ok_or_else(|| fail("actual provider interface is absent"))?;
    if matches.next().is_some() {
        return Err(fail("actual provider interface is ambiguous"));
    }
    crate::dyn_metadata::ty(&table.self_type, b)?;
    crate::dyn_metadata::recipe_table(table, b)?;
    b.spend(table.self_classes.len(), WORK)?;
    let value = DynTypeValue {
        ty: table.self_type.clone(),
        life: life.clone(),
        classes: table.self_classes.clone(),
    };
    values::check(
        module,
        &value,
        interface.retained_life.binder_count,
        None,
        b,
    )
}
