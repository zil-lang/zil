//! Shared native allocator declarations used by Task and registration.
//! Earlier machine consumers may already have emitted these exact ABIs.
use super::*;

pub(super) fn calloc_and_free(
    layouts: &LayoutContext<'_>,
    lir: &LirView<'_>,
    existing: &m::Code,
    limits: &zil_base::Limits,
    meter: &tasks::Meter,
) -> CompileResult<m::Code> {
    let specs = [
        ("calloc", m::Ty::Ptr, vec![m::Ty::Int(64), m::Ty::Int(64)]),
        ("free", m::Ty::Void, vec![m::Ty::Ptr]),
    ];
    meter.spend(
        existing
            .0
            .len()
            .saturating_add(lir.functions.len().saturating_mul(specs.len())),
    )?;
    let mut declarations = [None, None];
    for event in &existing.0 {
        let m::Event::Declare(function) = event else {
            continue;
        };
        let index = match function.name.as_str() {
            "calloc" => 0,
            "free" => 1,
            _ => continue,
        };
        if declarations[index].replace(function).is_some() {
            return invalid(
                "E-CODEGEN-CFG",
                "duplicate native allocator declaration",
                None,
            );
        }
    }

    let mut code = m::Code::new();
    for ((name, result, parameters), previous) in specs.into_iter().zip(declarations) {
        if let Some(function) = lir.functions.iter().find(|function| {
            function
                .foreign
                .as_ref()
                .is_some_and(|foreign| foreign.symbol == name)
        }) {
            meter.spend(function.parameters.len().saturating_add(1))?;
            let before = meter.remaining();
            let mut left = before;
            for local in &function.parameters {
                copy_aggregates::reserve_enum_metadata(
                    &function.locals[local.0 as usize].ty,
                    &mut left,
                    0,
                    limits,
                )?;
            }
            copy_aggregates::reserve_enum_metadata(&function.return_type, &mut left, 0, limits)?;
            meter.spend(before - left)?;
            let actual = function
                .parameters
                .iter()
                .map(|local| layouts.value_type(&function.locals[local.0 as usize].ty))
                .collect::<CompileResult<Vec<_>>>()?;
            if actual != parameters || layouts.value_type(&function.return_type)? != result {
                return invalid(
                    "E-CODEGEN-FOREIGN",
                    format!("{name} conflicts with the native allocator ABI"),
                    Some(function.span),
                );
            }
            continue;
        }
        let expected = m::Function {
            name: name.into(),
            linkage: m::Linkage::External,
            signature: m::Signature::new(
                result,
                parameters.into_iter().map(m::Param::unnamed).collect(),
            ),
            attrs: vec![],
            blocks: None,
        };
        if let Some(previous) = previous {
            if previous != &expected {
                return invalid(
                    "E-CODEGEN-FOREIGN",
                    format!("{name} changes its native allocator declaration"),
                    None,
                );
            }
        } else {
            code.push(m::Event::Declare(expected));
        }
    }
    Ok(code)
}
