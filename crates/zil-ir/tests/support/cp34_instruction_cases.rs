//! One raw identity case per frozen InstructionKind variant; not accepted IR.
use zil_ir::*;
pub fn instructions(flag: bool) -> Vec<InstructionKind> {
    vec![
        InstructionKind::OwnedStoreNew { element: Type::I32 },
        InstructionKind::OwnedStoreReserve {
            store: ValueId(65),
            capacity: ValueId(68),
        },
        InstructionKind::OwnedStorePush {
            store: ValueId(129),
            value: ValueId(132),
        },
        InstructionKind::OwnedStorePop {
            store: ValueId(193),
        },
        InstructionKind::OwnedStoreLen {
            store: ValueId(257),
        },
        InstructionKind::OwnedStoreCapacity {
            store: ValueId(321),
        },
        InstructionKind::OwnedStoreBorrow {
            store: ValueId(385),
            index: ValueId(388),
        },
        InstructionKind::OwnedStoreBorrowMut {
            store: ValueId(449),
            index: ValueId(452),
        },
        InstructionKind::ByteBufferNew,
        InstructionKind::ByteBufferReserve {
            buffer: ValueId(577),
            capacity: ValueId(580),
        },
        InstructionKind::ByteBufferPush {
            buffer: ValueId(641),
            value: ValueId(644),
        },
        InstructionKind::ByteBufferTruncate {
            buffer: ValueId(705),
            length: ValueId(708),
        },
        InstructionKind::ByteBufferLen {
            buffer: ValueId(769),
        },
        InstructionKind::ByteBufferCapacity {
            buffer: ValueId(833),
        },
        InstructionKind::ByteBufferBorrow {
            buffer: ValueId(897),
            mutable: flag,
        },
        InstructionKind::ScopeEnter {
            scope: ScopeId(961),
        },
        InstructionKind::ScopeExit {
            scope: ScopeId(1025),
            kind: CleanupExitKind::Return,
        },
        InstructionKind::MakeErrorUnion {
            is_error: flag,
            value: ValueId(1092),
        },
        InstructionKind::ErrorTag {
            value: ValueId(1153),
        },
        InstructionKind::ReadErrorUnion {
            value: ValueId(1217),
            is_error: flag,
        },
        InstructionKind::TakeErrorUnion {
            value: ValueId(1281),
            is_error: flag,
        },
        InstructionKind::DropErrorUnion {
            local: LocalId(1345),
        },
        InstructionKind::MakeManaged {
            fields: vec![ValueId(1409), ValueId(1411)],
        },
        InstructionKind::MakePackage {
            fields: vec![ValueId(1473), ValueId(1475)],
        },
        InstructionKind::MakeOwnedArray {
            elements: vec![ValueId(1537), ValueId(1539)],
        },
        InstructionKind::ProjectOwnedArrayField {
            local: LocalId(1601),
            index: 1604,
            field: 1607,
        },
        InstructionKind::TakeOwnedArrayElement {
            local: LocalId(1665),
            index: 1668,
        },
        InstructionKind::DropOwnedArrayElement {
            local: LocalId(1729),
            index: 1732,
        },
        InstructionKind::StoreOwnedArrayElement {
            local: LocalId(1793),
            index: 1796,
            value: ValueId(1799),
        },
        InstructionKind::DropOwnedArray {
            local: LocalId(1857),
        },
        InstructionKind::BorrowOwnedArrayElement {
            local: LocalId(1921),
            index: 1924,
            mutable: flag,
        },
        InstructionKind::BorrowPackageField {
            local: LocalId(1985),
            index: 1988,
            mutable: flag,
        },
        InstructionKind::ReadPackageField {
            local: LocalId(2049),
            index: 2052,
        },
        InstructionKind::DescribePackageField {
            local: LocalId(2113),
            index: 2116,
        },
        InstructionKind::DropPackage {
            local: LocalId(2177),
        },
        InstructionKind::TakePackageField {
            local: LocalId(2241),
            index: 2244,
        },
        InstructionKind::DropPackageField {
            local: LocalId(2305),
            index: 2308,
        },
        InstructionKind::StorePackageField {
            local: LocalId(2369),
            index: 2372,
            value: ValueId(2375),
        },
        InstructionKind::BindPackageCapture {
            local: LocalId(2433),
            index: 2436,
        },
        InstructionKind::EndPackageCapture {
            local: LocalId(2497),
            index: 2500,
        },
        InstructionKind::DescribeLocal {
            local: LocalId(2561),
        },
        InstructionKind::ReadDesc {
            desc: ValueId(2625),
            index: 2628,
        },
        InstructionKind::WriteDesc {
            desc: ValueId(2689),
            index: 2692,
            value: ValueId(2695),
        },
        InstructionKind::DropManaged {
            local: LocalId(2753),
        },
        InstructionKind::Constant(Constant::I32(-41)),
        InstructionKind::ConvertInteger {
            value: ValueId(2881),
            target: Type::I32,
        },
        InstructionKind::Load(LocalId(2945)),
        InstructionKind::TakeLocal {
            local: LocalId(3009),
        },
        InstructionKind::StorageLive(LocalId(3073)),
        InstructionKind::StorageDead(LocalId(3137)),
        InstructionKind::BorrowLocal {
            local: LocalId(3201),
        },
        InstructionKind::BorrowLocalMut {
            local: LocalId(3265),
        },
        InstructionKind::ReborrowAddress {
            address: ValueId(3329),
            mutable: flag,
        },
        InstructionKind::ReadAddress {
            address: ValueId(3393),
        },
        InstructionKind::WriteAddress {
            address: ValueId(3457),
            value: ValueId(3460),
        },
        InstructionKind::AddressField {
            address: ValueId(3521),
            index: 3524,
        },
        InstructionKind::AddressIndex {
            address: ValueId(3585),
            index: ValueId(3588),
        },
        InstructionKind::AddressSlice {
            address: ValueId(3649),
            start: flag.then_some(ValueId(3652)),
            end: flag.then_some(ValueId(3655)),
        },
        InstructionKind::SliceLength {
            address: ValueId(3713),
        },
        InstructionKind::Store {
            local: LocalId(3777),
            value: ValueId(3780),
        },
        InstructionKind::Unary {
            op: UnaryOp::Negate,
            operand: ValueId(3844),
        },
        InstructionKind::Binary {
            op: BinaryOp::Add,
            left: ValueId(3908),
            right: ValueId(3911),
        },
        InstructionKind::Call {
            function: FunctionId(3969),
            arguments: vec![ValueId(3972), ValueId(3974)],
        },
        InstructionKind::UnsafeEnter { region: 4033 },
        InstructionKind::UnsafeExit { region: 4097 },
        InstructionKind::ForeignCall {
            function: FunctionId(4161),
            arguments: vec![ValueId(4164), ValueId(4166)],
            region: 4167,
        },
        InstructionKind::FunctionRef {
            function: FunctionId(4225),
        },
        InstructionKind::ForeignFunctionRef {
            function: FunctionId(4289),
        },
        InstructionKind::CallIndirect {
            callee: ValueId(4353),
            arguments: vec![ValueId(4356), ValueId(4358)],
        },
        InstructionKind::MakeAggregate {
            fields: vec![ValueId(4417), ValueId(4419)],
        },
        InstructionKind::MakeVariant {
            variant: 4481,
            fields: vec![ValueId(4484), ValueId(4486)],
        },
        InstructionKind::VariantTag {
            value: ValueId(4545),
        },
        InstructionKind::ReadVariant {
            value: ValueId(4609),
            variant: 4612,
            index: 4615,
        },
        InstructionKind::ExtractIndex {
            aggregate: ValueId(4673),
            index: ValueId(4676),
        },
        InstructionKind::UpdateIndex {
            aggregate: ValueId(4737),
            index: ValueId(4740),
            value: ValueId(4743),
        },
        InstructionKind::ExtractField {
            aggregate: ValueId(4801),
            index: 4804,
        },
        InstructionKind::UpdateField {
            aggregate: ValueId(4865),
            index: 4868,
            value: ValueId(4871),
        },
        InstructionKind::MakeClosure {
            closure: ClosureId(4929),
            captures: vec![ValueId(4932), ValueId(4934)],
        },
        InstructionKind::ReadCapture { index: 4993 },
        InstructionKind::DescribeCapture { index: 5057 },
        InstructionKind::TakeCapture { index: 5121 },
        InstructionKind::BorrowCapture {
            index: 5185,
            mutable: flag,
        },
        InstructionKind::CallClosureShared {
            callee: LocalId(5249),
            arguments: vec![ValueId(5252), ValueId(5254)],
        },
        InstructionKind::CallClosureBorrowed {
            callee: ValueId(5313),
            kind: CallableKind::FnMut,
            arguments: vec![ValueId(5319), ValueId(5321)],
        },
        InstructionKind::CallClosureOnce {
            callee: ValueId(5377),
            arguments: vec![ValueId(5380), ValueId(5382)],
        },
        InstructionKind::DropClosure {
            local: LocalId(5441),
        },
        InstructionKind::WriteCapture {
            index: 5505,
            value: ValueId(5508),
        },
        InstructionKind::CallClosureMut {
            callee: LocalId(5569),
            arguments: vec![ValueId(5572), ValueId(5574)],
        },
        InstructionKind::CallClosure {
            callee: ValueId(5633),
            arguments: vec![ValueId(5636), ValueId(5638)],
        },
        InstructionKind::Unsupported("unsupported-值".into()),
    ]
}
