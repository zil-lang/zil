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
        let mut bytes = b"ZIL-VERIFIED-CALL-CONTRACT-3\0".to_vec();
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
                || function.return_type != export.result
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
            bytes.extend_from_slice(&(export.name.len() as u64).to_le_bytes());
            bytes.extend_from_slice(export.name.as_bytes());
            bytes.extend_from_slice(&(parameters.len() as u64).to_le_bytes());
            for parameter in &parameters {
                bytes.extend_from_slice(&parameter.to_le_bytes());
            }
            results.push(FunctionResultOrigins {
                name: export.name.clone(),
                parameters,
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
            let traces = c.traces.iter().map(|trace| format!(
                "{{\"calls\":[{}],\"exit\":{}}}",
                trace.calls.iter().map(|mode| json_string(&format!("{mode:?}"))).collect::<Vec<_>>().join(","),
                json_string(&format!("{:?}", trace.exit))
            )).collect::<Vec<_>>().join(",");
            format!("{{\"name\":{},\"parameter\":{},\"closure\":{},\"kind\":{},\"hold\":{:?},\"call\":{:?},\"writes\":{:?},\"cleanup\":{},\"traces\":[{}]}}",
                json_string(&entry.name), c.parameter, c.closure.0, json_string(&format!("{:?}", c.kind)),
                c.hold, c.call, c.writes, json_string(&format!("{:?}", c.cleanup)), traces)
        }).collect::<Vec<_>>().join(","))
    })
}

pub(super) fn callable_return_json(interface: &ScalarInterface) -> String {
    use super::json_string;
    interface.callable_returns.as_ref().map_or_else(|| "null".into(), |contracts| {
        format!("[{}]", contracts.iter().map(|entry| {
            let c = &entry.contract;
            format!("{{\"name\":{},\"closure\":{},\"kind\":{},\"hold\":{:?},\"call\":{:?},\"writes\":{:?},\"cleanup\":{:?}}}",
                json_string(&entry.name), c.closure.0, json_string(&format!("{:?}", c.kind)),
                c.hold, c.call, c.writes, c.cleanup)
        }).collect::<Vec<_>>().join(","))
    })
}
