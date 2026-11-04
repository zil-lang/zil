use super::*;
use crate::place_replacements::fixture;
#[test]
fn prepared_pin_allows_shared_reads_and_disjoint_fields_but_blocks_parent_mutation() {
    let mut module = fixture::module();
    let Type::Address { inner, .. } = &module.functions[0].values[0] else {
        unreachable!()
    };
    let inner = inner.clone();
    module.functions[0].values.push(Type::Address {
        mutable: false,
        inner,
    });
    let f = &module.functions[0];
    let limits = Limits::default();
    let mut budget = Budget::new(&limits);
    let definitions = crate::place_replacements::Definitions::new(&module, f, &mut budget).unwrap();
    let active = BTreeSet::from([ReservationId(71)]);
    let values = vec![Roots::from([Root::Parameter(0)]), Roots::new()];
    for (kind, allowed) in [
        (
            InstructionKind::ReborrowAddress {
                address: ValueId(0),
                mutable: false,
            },
            true,
        ),
        (
            InstructionKind::BorrowPlaceAddress {
                address: ValueId(0),
                path: vec![zil_ir::PlaceProjection::Field(1)],
                mutable: true,
            },
            true,
        ),
        (
            InstructionKind::BorrowPlaceAddress {
                address: ValueId(0),
                path: vec![zil_ir::PlaceProjection::Field(0)],
                mutable: true,
            },
            false,
        ),
        (
            InstructionKind::Call {
                function: FunctionId(0),
                arguments: vec![ValueId(0), ValueId(1)],
            },
            false,
        ),
        (
            InstructionKind::WriteAddress {
                address: ValueId(0),
                value: ValueId(1),
            },
            false,
        ),
    ] {
        let result = matches!(kind, InstructionKind::ReborrowAddress { .. }).then_some(ValueId(2));
        let i = fixture::instruction(result, kind);
        let result = transition(
            &module,
            f,
            &i,
            &values,
            &State::new(),
            &active,
            &definitions,
            &mut budget,
        );
        assert_eq!(result.is_ok(), allowed, "{:?}: {result:?}", i.kind);
    }
}
