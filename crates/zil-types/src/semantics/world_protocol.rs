//! Exact original/actual protocol head correspondence. Conditional facts come
//! from the complete original body pass; this bridge grants no root or phase.
use super::*;

pub(super) fn header(
    world: &mut World<'_, '_>,
    original: &SemanticDefinition,
    function: &TypedFunction,
) -> R<()> {
    let contracts = match &original.body {
        SemanticDefinitionBody::FunctionWithBody {
            protocol_contracts, ..
        } => protocol_contracts.as_slice(),
        _ => &[],
    };
    world.pay(
        contracts
            .len()
            .saturating_add(function.protocol_contracts.len()),
    )?;
    for contract in contracts.iter().chain(&function.protocol_contracts) {
        crate::registration::charge_contract(contract, |work| world.pay(work))?;
        if contract.parameter as usize >= function.parameters.len() {
            return Err(bad(
                "protocol contract parameter is not a real original/actual ABI position",
            ));
        }
    }
    if contracts != function.protocol_contracts {
        return Err(bad(
            "actual protocol table differs from its complete original declaration",
        ));
    }
    if !contracts.is_empty() {
        let ledger = world.ledger;
        let proof = ledger.original.borrow();
        let proof = proof
            .as_ref()
            .ok_or_else(|| bad("protocol bridge lacks original symbolic facts"))?;
        world.pay(proof.protocol_complete.len())?;
        if !proof.protocol_complete.contains(&original.id) {
            return Err(bad(
                "actual protocol declaration has no complete original conditional body proof",
            ));
        }
    }
    Ok(())
}
