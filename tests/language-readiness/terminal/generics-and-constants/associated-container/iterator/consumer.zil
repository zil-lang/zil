import iter
pub struct Snapshot<I:iter.LendingIterator>{pub value:Option<I.Item>}
pub fn capture<I:iter.LendingIterator>(cursor:&mut I)->Snapshot<I>{return Snapshot::<I>{value:iter.next_copy(cursor)}}
pub fn take<I:iter.LendingIterator>(snapshot:Snapshot<I>)->Option<I.Item>{return snapshot.value}
