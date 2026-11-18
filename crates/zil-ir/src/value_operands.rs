//! Operand enumeration for the raw Typed instruction schema.
use crate::{InstructionKind, LocalId, ValueId};

impl InstructionKind {
    /// Actual SSA operands in instruction order. This describes raw IR only.
    pub fn value_operands(&self) -> impl Iterator<Item = ValueId> + '_ {
        let (a, b, rest): (Option<ValueId>, Option<ValueId>, &[ValueId]) = match self {
            Self::MutexCreate { value } => (Some(*value), None, &[]),
            Self::MutexLock { mutex } => (Some(*mutex), None, &[]),
            Self::MutexValue { guard } | Self::MutexUnlock { guard } => (Some(*guard), None, &[]),
            Self::DropMutexGuard { .. } => (None, None, &[]),
            Self::AsyncRegistrationStart { callback, .. } => (Some(*callback), None, &[]),
            Self::AsyncRegistrationValid { registration }
            | Self::AsyncRegistrationRequestClose { registration }
            | Self::AsyncRegistrationPollClose { registration }
            | Self::AsyncRegistrationFinishDestroy { registration }
            | Self::AsyncRegistrationFinishDestroyChecked { registration } => {
                (Some(*registration), None, &[])
            }
            Self::MakeDynBorrow { source, .. } | Self::MakeDynOwned { source, .. } => {
                (Some(*source), None, &[])
            }
            Self::BorrowDynOwned { .. } | Self::DropDynOwned { .. } => (None, None, &[]),
            Self::CallDynSlot {
                callee, arguments, ..
            } => (Some(*callee), None, arguments),
            Self::LoadStoredBorrow { storage, .. } => (Some(*storage), None, &[]),
            Self::StoreStoredBorrow { storage, value, .. } => (Some(*storage), Some(*value), &[]),
            Self::ReservePlaceLocal { .. } | Self::AbortPlaceReservation { .. } => {
                (None, None, &[])
            }
            Self::ReservePlaceAddress { address, .. } => (Some(*address), None, &[]),
            Self::CommitPlaceReplace { value, .. } => (Some(*value), None, &[]),
            Self::RegistrationStart { callback, .. } => (Some(*callback), None, &[]),
            Self::RegistrationValid { registration } | Self::RegistrationClose { registration } => {
                (Some(*registration), None, &[])
            }
            Self::TaskStart { argument, .. } => (Some(*argument), None, &[]),
            Self::TaskCreate { arguments, .. } | Self::TaskCreateOwned { arguments, .. } => {
                (None, None, arguments)
            }
            Self::TaskValid { task }
            | Self::TaskPoll { task }
            | Self::TaskRequestCancel { task }
            | Self::TaskPollClose { task }
            | Self::TaskFinishDestroy { task } => (Some(*task), None, &[]),
            Self::TaskYield => (None, None, &[]),
            Self::MakeOwnedTuple { fields } => (None, None, fields),
            Self::BorrowPlaceLocal { .. }
            | Self::PlaceTagLocal { .. }
            | Self::UnpackTuple { .. }
            | Self::DropTuple { .. }
            | Self::TakeTupleField { .. }
            | Self::TakeVariantField { .. } => (None, None, &[]),

            Self::PlaceTagAddress { address, .. }
            | Self::BorrowPlaceAddress { address, .. }
            | Self::VariantTagAddress { address, .. }
            | Self::BorrowVariantAddress { address, .. } => (Some(*address), None, &[]),
            Self::WithScopedThread { argument, body, .. } => (Some(*argument), Some(*body), &[]),
            Self::WithSyncCell { initial, body, .. } => (Some(*initial), Some(*body), &[]),
            Self::SyncI32 { address, value, .. } | Self::SyncInteger { address, value, .. } => {
                (Some(*address), *value, &[])
            }
            Self::SyncI32CompareExchange {
                address,
                expected,
                desired,
                ..
            }
            | Self::SyncIntegerCompareExchange {
                address,
                expected,
                desired,
                ..
            } => (
                Some(*address),
                Some(*expected),
                std::slice::from_ref(desired),
            ),
            Self::WithForeignCallback { callback, body, .. } => (Some(*callback), Some(*body), &[]),
            Self::OwnedStoreReserve { store, capacity } => (Some(*store), Some(*capacity), &[]),
            Self::OwnedStorePush { store, value } => (Some(*store), Some(*value), &[]),
            Self::OwnedStoreBorrow { store, index }
            | Self::OwnedStoreBorrowMut { store, index } => (Some(*store), Some(*index), &[]),
            Self::OwnedStorePop { store }
            | Self::OwnedStoreLen { store }
            | Self::OwnedStoreCapacity { store } => (Some(*store), None, &[]),
            Self::DescribeAddress { address: value }
            | Self::Store { value, .. }
            | Self::ConvertInteger { value, .. }
            | Self::ConvertNumber { value, .. }
            | Self::StorePackageField { value, .. }
            | Self::StoreOwnedArrayElement { value, .. }
            | Self::WriteCapture { value, .. } => (Some(*value), None, &[]),
            Self::Call { arguments, .. }
            | Self::SuspendCall { arguments, .. }
            | Self::ForeignCall { arguments, .. }
            | Self::CallClosureMut { arguments, .. }
            | Self::CallClosureShared { arguments, .. } => (None, None, arguments),
            Self::CallClosure { callee, arguments }
            | Self::CallClosureOnce { callee, arguments }
            | Self::CallClosureBorrowed {
                callee, arguments, ..
            }
            | Self::CallIndirect { callee, arguments } => (Some(*callee), None, arguments),
            Self::MakeClosure { captures, .. } => (None, None, captures),
            Self::MakeManaged { fields }
            | Self::MakePackage { fields }
            | Self::MakeAggregate { fields }
            | Self::MakeVariant { fields, .. } => (None, None, fields),
            Self::MakeOwnedArray { elements } => (None, None, elements),
            Self::ReadDesc { desc, .. } => (Some(*desc), None, &[]),
            Self::ByteBufferLen { buffer }
            | Self::ByteBufferCapacity { buffer }
            | Self::ByteBufferBorrow { buffer, .. } => (Some(*buffer), None, &[]),
            Self::ByteBufferReserve { buffer, capacity } => (Some(*buffer), Some(*capacity), &[]),
            Self::ByteBufferTruncate { buffer, length } => (Some(*buffer), Some(*length), &[]),
            Self::ByteBufferPush { buffer, value } => (Some(*buffer), Some(*value), &[]),
            Self::WriteDesc { desc, value, .. } => (Some(*desc), Some(*value), &[]),
            Self::Unary { operand, .. } => (Some(*operand), None, &[]),
            Self::Binary { left, right, .. } => (Some(*left), Some(*right), &[]),
            Self::ReadAddress { address }
            | Self::SliceLength { address }
            | Self::ReborrowAddress { address, .. }
            | Self::AddressField { address, .. } => (Some(*address), None, &[]),
            Self::WriteAddress { address, value } => (Some(*address), Some(*value), &[]),
            Self::AddressIndex { address, index } => (Some(*address), Some(*index), &[]),
            Self::AddressSlice {
                address,
                start,
                end,
            } => (Some(*address), *start, end.as_slice()),
            Self::ExtractField { aggregate, .. } => (Some(*aggregate), None, &[]),
            Self::ExtractIndex { aggregate, index } => (Some(*aggregate), Some(*index), &[]),
            Self::UpdateField {
                aggregate, value, ..
            } => (Some(*aggregate), Some(*value), &[]),
            Self::UpdateIndex {
                aggregate,
                index,
                value,
            } => (Some(*aggregate), Some(*index), std::slice::from_ref(value)),
            Self::MakeErrorUnion { value, .. }
            | Self::ErrorTag { value }
            | Self::ReadErrorUnion { value, .. }
            | Self::TakeErrorUnion { value, .. }
            | Self::VariantTag { value }
            | Self::ReadVariant { value, .. } => (Some(*value), None, &[]),
            Self::StaticUtf8 { .. }
            | Self::ByteBufferNew
            | Self::RegisterDefer { .. }
            | Self::VariantTagLocal { .. }
            | Self::ReadVariantLocal { .. }
            | Self::BorrowVariantLocal { .. }
            | Self::BorrowFieldLocal { .. }
            | Self::UnpackVariant { .. }
            | Self::DropEnum { .. }
            | Self::OwnedStoreNew { .. }
            | Self::Constant(_)
            | Self::Load(_)
            | Self::TakeLocal { .. }
            | Self::BorrowLocal { .. }
            | Self::BorrowLocalMut { .. }
            | Self::DescribeLocal { .. }
            | Self::ReadPackageField { .. }
            | Self::DescribePackageField { .. }
            | Self::TakePackageField { .. }
            | Self::DropPackageField { .. }
            | Self::ProjectOwnedArrayField { .. }
            | Self::TakeOwnedArrayElement { .. }
            | Self::DropOwnedArrayElement { .. }
            | Self::DropOwnedArray { .. }
            | Self::BorrowOwnedArrayElement { .. }
            | Self::BorrowPackageField { .. }
            | Self::BorrowCapture { .. }
            | Self::DescribeCapture { .. }
            | Self::TakeCapture { .. }
            | Self::ReadCapture { .. }
            | Self::BindPackageCapture { .. }
            | Self::EndPackageCapture { .. }
            | Self::DropManaged { .. }
            | Self::DropPackage { .. }
            | Self::DropErrorUnion { .. }
            | Self::DropClosure { .. }
            | Self::StorageLive(_)
            | Self::StorageDead(_)
            | Self::UnsafeEnter { .. }
            | Self::UnsafeExit { .. }
            | Self::ScopeEnter { .. }
            | Self::ScopeExit { .. }
            | Self::FunctionRef { .. }
            | Self::ForeignFunctionRef { .. }
            | Self::Unsupported(_) => (None, None, &[]),
        };
        a.into_iter().chain(b).chain(rest.iter().copied())
    }

    /// Actual storage-local operands, including projection destinations.
    pub fn local_operands(&self) -> impl Iterator<Item = LocalId> + '_ {
        let (head, rest): (Option<LocalId>, &[LocalId]) = match self {
            Self::BorrowDynOwned { local, .. }
            | Self::DropDynOwned { local, .. }
            | Self::RegisterDefer { local, .. }
            | Self::DropErrorUnion { local, .. }
            | Self::ProjectOwnedArrayField { local, .. }
            | Self::TakeOwnedArrayElement { local, .. }
            | Self::DropOwnedArrayElement { local, .. }
            | Self::StoreOwnedArrayElement { local, .. }
            | Self::DropOwnedArray { local, .. }
            | Self::BorrowOwnedArrayElement { local, .. }
            | Self::BorrowPackageField { local, .. }
            | Self::ReadPackageField { local, .. }
            | Self::DescribePackageField { local, .. }
            | Self::DropPackage { local, .. }
            | Self::TakePackageField { local, .. }
            | Self::DropPackageField { local, .. }
            | Self::StorePackageField { local, .. }
            | Self::BindPackageCapture { local, .. }
            | Self::EndPackageCapture { local, .. }
            | Self::DescribeLocal { local, .. }
            | Self::DropManaged { local, .. }
            | Self::Load(local)
            | Self::TakeLocal { local, .. }
            | Self::StorageLive(local)
            | Self::StorageDead(local)
            | Self::BorrowLocal { local, .. }
            | Self::BorrowLocalMut { local, .. }
            | Self::Store { local, .. }
            | Self::DropMutexGuard { local, .. }
            | Self::DropTuple { local, .. }
            | Self::TakeTupleField { local, .. }
            | Self::TakeVariantField { local, .. }
            | Self::BorrowPlaceLocal { local, .. }
            | Self::PlaceTagLocal { local, .. }
            | Self::ReservePlaceLocal { local, .. }
            | Self::BorrowFieldLocal { local, .. }
            | Self::VariantTagLocal { local, .. }
            | Self::ReadVariantLocal { local, .. }
            | Self::BorrowVariantLocal { local, .. }
            | Self::DropEnum { local, .. }
            | Self::CallClosureShared { callee: local, .. }
            | Self::DropClosure { local, .. }
            | Self::CallClosureMut { callee: local, .. } => (Some(*local), &[]),
            Self::UnpackTuple { local, fields, .. } | Self::UnpackVariant { local, fields, .. } => {
                (Some(*local), fields)
            }
            Self::MakeDynBorrow { .. }
            | Self::MakeDynOwned { .. }
            | Self::CallDynSlot { .. }
            | Self::LoadStoredBorrow { .. }
            | Self::StoreStoredBorrow { .. }
            | Self::OwnedStoreNew { .. }
            | Self::OwnedStoreReserve { .. }
            | Self::OwnedStorePush { .. }
            | Self::OwnedStorePop { .. }
            | Self::OwnedStoreLen { .. }
            | Self::OwnedStoreCapacity { .. }
            | Self::OwnedStoreBorrow { .. }
            | Self::OwnedStoreBorrowMut { .. }
            | Self::ByteBufferNew
            | Self::ByteBufferReserve { .. }
            | Self::ByteBufferPush { .. }
            | Self::ByteBufferTruncate { .. }
            | Self::ByteBufferLen { .. }
            | Self::ByteBufferCapacity { .. }
            | Self::ByteBufferBorrow { .. }
            | Self::ScopeEnter { .. }
            | Self::ScopeExit { .. }
            | Self::MakeErrorUnion { .. }
            | Self::ErrorTag { .. }
            | Self::ReadErrorUnion { .. }
            | Self::TakeErrorUnion { .. }
            | Self::MakeManaged { .. }
            | Self::MakePackage { .. }
            | Self::MakeOwnedArray { .. }
            | Self::DescribeAddress { .. }
            | Self::ReadDesc { .. }
            | Self::WriteDesc { .. }
            | Self::StaticUtf8 { .. }
            | Self::Constant(_)
            | Self::ConvertInteger { .. }
            | Self::ConvertNumber { .. }
            | Self::ReborrowAddress { .. }
            | Self::ReadAddress { .. }
            | Self::WriteAddress { .. }
            | Self::AddressField { .. }
            | Self::AddressIndex { .. }
            | Self::AddressSlice { .. }
            | Self::SliceLength { .. }
            | Self::Unary { .. }
            | Self::Binary { .. }
            | Self::Call { .. }
            | Self::SuspendCall { .. }
            | Self::UnsafeEnter { .. }
            | Self::UnsafeExit { .. }
            | Self::ForeignCall { .. }
            | Self::WithForeignCallback { .. }
            | Self::WithScopedThread { .. }
            | Self::WithSyncCell { .. }
            | Self::SyncI32 { .. }
            | Self::SyncI32CompareExchange { .. }
            | Self::SyncInteger { .. }
            | Self::SyncIntegerCompareExchange { .. }
            | Self::MutexCreate { .. }
            | Self::MutexLock { .. }
            | Self::MutexValue { .. }
            | Self::MutexUnlock { .. }
            | Self::TaskStart { .. }
            | Self::TaskCreate { .. }
            | Self::TaskCreateOwned { .. }
            | Self::TaskYield
            | Self::TaskValid { .. }
            | Self::TaskPoll { .. }
            | Self::TaskRequestCancel { .. }
            | Self::TaskPollClose { .. }
            | Self::TaskFinishDestroy { .. }
            | Self::RegistrationStart { .. }
            | Self::RegistrationValid { .. }
            | Self::RegistrationClose { .. }
            | Self::AsyncRegistrationStart { .. }
            | Self::AsyncRegistrationValid { .. }
            | Self::AsyncRegistrationRequestClose { .. }
            | Self::AsyncRegistrationPollClose { .. }
            | Self::AsyncRegistrationFinishDestroy { .. }
            | Self::AsyncRegistrationFinishDestroyChecked { .. }
            | Self::FunctionRef { .. }
            | Self::ForeignFunctionRef { .. }
            | Self::CallIndirect { .. }
            | Self::MakeAggregate { .. }
            | Self::MakeVariant { .. }
            | Self::VariantTag { .. }
            | Self::ReadVariant { .. }
            | Self::MakeOwnedTuple { .. }
            | Self::BorrowPlaceAddress { .. }
            | Self::PlaceTagAddress { .. }
            | Self::ReservePlaceAddress { .. }
            | Self::CommitPlaceReplace { .. }
            | Self::AbortPlaceReservation { .. }
            | Self::VariantTagAddress { .. }
            | Self::BorrowVariantAddress { .. }
            | Self::ExtractIndex { .. }
            | Self::UpdateIndex { .. }
            | Self::ExtractField { .. }
            | Self::UpdateField { .. }
            | Self::MakeClosure { .. }
            | Self::ReadCapture { .. }
            | Self::DescribeCapture { .. }
            | Self::TakeCapture { .. }
            | Self::BorrowCapture { .. }
            | Self::CallClosureBorrowed { .. }
            | Self::CallClosureOnce { .. }
            | Self::WriteCapture { .. }
            | Self::CallClosure { .. }
            | Self::Unsupported(_) => (None, &[]),
        };
        head.into_iter().chain(rest.iter().copied())
    }
}
