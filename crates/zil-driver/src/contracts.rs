//! Publish address and finite callable contracts only after independent verification.
use zil_base::{CompileResult, Diagnostic, Fingerprint, Phase};
use zil_ir::{
    FunctionCallableContract, FunctionCallableReturn, FunctionResultOrigins, ScalarInterface,
    callable_contract_fingerprint, callable_return_contract_fingerprint,
};
use zil_verify::VerifiedModule;

pub(super) fn publish(
    verified: &VerifiedModule,
    interfaces: &mut [ScalarInterface],
) -> CompileResult<()> {
    for interface in interfaces {
        let mut bytes = b"ZIL-VERIFIED-CALL-CONTRACT-7\0".to_vec();
        bytes.extend_from_slice(&interface.signature_fingerprint.0);
        let mut results = Vec::with_capacity(interface.exports.len());
        let mut callables = Vec::new();
        let mut callable_returns = Vec::new();
        for export in &interface.exports {
            let fail = || {
                vec![Diagnostic::error(
                    Phase::Verify,
                    "E-INTERFACE-IDENTITY",
                    None,
                    "exported interface does not match its verified function",
                )]
            };
            let function = verified
                .module()
                .functions
                .get(export.function.0 as usize)
                .ok_or_else(fail)?;
            if function.environment.is_some()
                || verified.is_suspend(export.function)
                || function.return_type != export.result
                || function.reference_return_bound != export.reference_return_bound
                || function.result_mode != export.result_mode
                || function.parameters.len() != export.parameters.len()
                || function
                    .parameters
                    .iter()
                    .zip(&export.parameters)
                    .any(|(local, (_, ty))| &function.locals[local.0 as usize].ty != ty)
            {
                return Err(fail());
            }
            if let Some(contract) = verified.callable_contract(export.function) {
                bytes.push(1);
                bytes.extend_from_slice(&callable_contract_fingerprint(contract).0);
                callables.push(FunctionCallableContract {
                    name: export.name.clone(),
                    contract: contract.clone(),
                });
            } else {
                bytes.push(0);
            }
            if let Some(contract) = verified.callable_return_contract(export.function) {
                bytes.push(1);
                bytes.extend_from_slice(&callable_return_contract_fingerprint(contract).0);
                callable_returns.push(FunctionCallableReturn {
                    name: export.name.clone(),
                    contract: contract.clone(),
                });
            } else {
                bytes.push(0);
            }
            let parameters = verified.result_origins(export.function).ok_or_else(fail)?;
            let static_storage = verified
                .result_has_static_origin(export.function)
                .ok_or_else(fail)?;
            bytes.extend_from_slice(&(export.name.len() as u64).to_le_bytes());
            bytes.extend_from_slice(export.name.as_bytes());
            bytes.extend_from_slice(&(parameters.len() as u64).to_le_bytes());
            for parameter in &parameters {
                bytes.extend_from_slice(&parameter.to_le_bytes());
            }
            bytes.push(u8::from(static_storage));
            results.push(FunctionResultOrigins {
                name: export.name.clone(),
                parameters,
                static_storage,
            });
        }
        interface.result_origins = Some(results);
        interface.callable_contracts = Some(callables);
        interface.callable_returns = Some(callable_returns);
        interface.contract_fingerprint = Some(Fingerprint::of(&bytes));
    }
    Ok(())
}

/// JSON presentation of an optional verified relation; null remains distinct
/// from a verified empty export set.
pub(super) fn callable_json(interface: &ScalarInterface) -> String {
    use super::json_string;
    interface.callable_contracts.as_ref().map_or_else(|| "null".into(), |contracts| {
        format!("[{}]", contracts.iter().map(|entry| {
            let c = &entry.contract;
            let traces = traces_json(&c.traces);
            let roles = c.roles.iter().map(|role| {
                let life = role.life.as_ref().map_or_else(|| "null".into(), storage_life_json);
                format!("{{\"parameter\":{},\"closure\":{},\"kind\":{},\"life\":{},\"hold\":{:?},\"call\":{:?},\"writes\":{:?},\"cleanup\":{}}}",
                    role.parameter, role.closure.0, json_string(&format!("{:?}", role.kind)), life,
                    role.hold, role.call, role.writes, json_string(&format!("{:?}", role.cleanup)))
            }).collect::<Vec<_>>().join(",");
            format!("{{\"name\":{},\"roles\":[{}],\"traces\":{}}}", json_string(&entry.name), roles, traces)
        }).collect::<Vec<_>>().join(","))
    })
}

fn source_position_json(position: &zil_ir::StoragePosition) -> String {
    use zil_ir::{ContractProjection, PlaceProjection, StorageBase};
    let base = match position.base {
        StorageBase::Parameter(i) => format!("{{\"parameter\":{i}}}"),
        StorageBase::Capture(i) => format!("{{\"capture\":{i}}}"),
        StorageBase::Result => "\"result\"".into(),
    };
    let path = position
        .path
        .iter()
        .map(|step| match step {
            ContractProjection::Capture(i) => format!("{{\"capture\":{i}}}"),
            ContractProjection::RetainedPort(i) => format!("{{\"retained_port\":{i}}}"),
            ContractProjection::RetainedClass(i) => format!("{{\"retained_class\":{i}}}"),
            ContractProjection::Place(PlaceProjection::Field(i)) => format!("{{\"field\":{i}}}"),
            ContractProjection::Place(PlaceProjection::Variant(p)) => {
                format!("{{\"variant\":{},\"index\":{}}}", p.variant, p.index)
            }
            ContractProjection::Place(PlaceProjection::ErrorArm(error)) => {
                format!("{{\"error_arm\":{error}}}")
            }
        })
        .collect::<Vec<_>>()
        .join(",");
    format!("{{\"base\":{base},\"path\":[{path}]}}")
}

pub(super) fn callable_return_json(interface: &ScalarInterface) -> String {
    use super::json_string;
    interface.callable_returns.as_ref().map_or_else(|| "null".into(), |contracts| {
        format!("[{}]", contracts.iter().map(|entry| {
            let c = &entry.contract;
            let sources=c.capture_sources.as_ref().map_or_else(||"null".into(),|rows|format!("[{}]",rows.iter().map(|row|format!("{{\"position\":{},\"sources\":[{}]}}",source_position_json(&row.position),row.sources.iter().map(source_position_json).collect::<Vec<_>>().join(","))).collect::<Vec<_>>().join(",")));
            let source=match c.source {zil_ir::CallableReturnSource::Fresh=>"{\"fresh\":true}".into(),zil_ir::CallableReturnSource::Parameter(i)=>format!("{{\"parameter\":{i}}}")};
            format!("{{\"name\":{},\"closure\":{},\"kind\":{},\"source\":{},\"capture_sources\":{},\"hold\":{:?},\"call\":{:?},\"writes\":{:?},\"cleanup\":{:?},\"lending_captures\":{:?}}}",
                json_string(&entry.name), c.closure.0, json_string(&format!("{:?}", c.kind)), source,sources,
                c.hold, c.call, c.writes, c.cleanup, c.lending_captures)
        }).collect::<Vec<_>>().join(","))
    })
}

fn traces_json(traces: &zil_ir::CallableTraces) -> String {
    use super::json_string;
    use zil_ir::{CallableFlowNext, CallableTraces};
    let calls = |calls: &[zil_ir::CallEvent]| {
        calls
            .iter()
            .map(|event| {
                format!(
                    "{{\"parameter\":{},\"kind\":{}}}",
                    event.parameter,
                    json_string(&format!("{:?}", event.kind))
                )
            })
            .collect::<Vec<_>>()
            .join(",")
    };
    match traces {
        CallableTraces::Finite(traces) => format!(
            "{{\"kind\":\"Finite\",\"paths\":[{}]}}",
            traces
                .iter()
                .map(|trace| format!(
                    "{{\"calls\":[{}],\"exit\":{}}}",
                    calls(&trace.calls),
                    json_string(&format!("{:?}", trace.exit))
                ))
                .collect::<Vec<_>>()
                .join(",")
        ),
        CallableTraces::Flow(flow) => format!(
            "{{\"kind\":\"Flow\",\"entry\":{},\"nodes\":[{}]}}",
            flow.entry,
            flow.nodes
                .iter()
                .map(|node| {
                    let next = match &node.next {
                        CallableFlowNext::Edges(edges) => {
                            format!("{{\"kind\":\"Edges\",\"targets\":{edges:?}}}")
                        }
                        CallableFlowNext::Exit(exit) => format!(
                            "{{\"kind\":\"Exit\",\"exit\":{}}}",
                            json_string(&format!("{exit:?}"))
                        ),
                    };
                    format!("{{\"calls\":[{}],\"next\":{}}}", calls(&node.calls), next)
                })
                .collect::<Vec<_>>()
                .join(",")
        ),
    }
}

fn storage_life_json(life: &zil_ir::StorageLifeContract) -> String {
    use zil_ir::{
        ContractProjection, LifeTypeProjection, PlaceProjection, StorageBase, StorageBinder,
    };
    fn base(base: StorageBase) -> String {
        match base {
            StorageBase::Parameter(n) => format!("{{\"parameter\":{n}}}"),
            StorageBase::Capture(n) => format!("{{\"capture\":{n}}}"),
            StorageBase::Result => "\"result\"".into(),
        }
    }
    fn binder(binder: StorageBinder) -> String {
        match binder {
            StorageBinder::Fixed(n) => format!("{{\"fixed\":{n}}}"),
            StorageBinder::Invocation(n) => format!("{{\"invocation\":{n}}}"),
        }
    }
    fn place(place: PlaceProjection) -> String {
        match place {
            PlaceProjection::Field(n) => format!("{{\"field\":{n}}}"),
            PlaceProjection::Variant(p) => {
                format!("{{\"variant\":{},\"index\":{}}}", p.variant, p.index)
            }
            PlaceProjection::ErrorArm(error) => format!("{{\"error_arm\":{error}}}"),
        }
    }
    let bindings = life
        .bindings
        .iter()
        .map(|row| {
            let path = row
                .position
                .path
                .iter()
                .map(|step| match *step {
                    ContractProjection::Place(p) => place(p),
                    ContractProjection::RetainedPort(n) => format!("{{\"retained_port\":{n}}}"),
                    ContractProjection::RetainedClass(n) => format!("{{\"retained_class\":{n}}}"),
                    ContractProjection::Capture(n) => format!("{{\"capture\":{n}}}"),
                })
                .collect::<Vec<_>>()
                .join(",");
            format!(
                "{{\"base\":{},\"path\":[{}],\"binder\":{}}}",
                base(row.position.base),
                path,
                binder(row.binder)
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    let classes = life
        .classes
        .iter()
        .map(|row| {
            let path = row
                .position
                .path
                .iter()
                .map(|step| match *step {
                    LifeTypeProjection::Place(p) => place(p),
                    LifeTypeProjection::TypeArgument(n) => format!("{{\"type_argument\":{n}}}"),
                    LifeTypeProjection::Port(n) => format!("{{\"port\":{n}}}"),
                    LifeTypeProjection::AssociatedType(n) => format!("{{\"associated_type\":{n}}}"),
                    LifeTypeProjection::Parameter(n) => format!("{{\"parameter\":{n}}}"),
                    LifeTypeProjection::Pointee => "\"pointee\"".into(),
                    LifeTypeProjection::Element => "\"element\"".into(),
                    LifeTypeProjection::Result => "\"result\"".into(),
                })
                .collect::<Vec<_>>()
                .join(",");
            format!(
                "{{\"base\":{},\"path\":[{}],\"class\":{},\"binder\":{}}}",
                base(row.position.base),
                path,
                row.position.class,
                binder(row.binder)
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    format!(
        "{{\"fixed_binder_count\":{},\"invocation_binder_count\":{},\"bindings\":[{}],\"classes\":[{}]}}",
        life.fixed_binder_count, life.invocation_binder_count, bindings, classes
    )
}
