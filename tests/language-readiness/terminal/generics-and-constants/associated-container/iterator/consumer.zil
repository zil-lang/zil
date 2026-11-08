import iter
pub struct Snapshot<T:Copy>{pub value:Option<T>}
pub fn capture<I:iter.LendingIterator>(cursor:&mut I)->Snapshot<I.Item> where I.Item:Copy {return Snapshot::<I.Item>{value:iter.next_copy(cursor)}}
pub fn take<T:Copy>(snapshot:Snapshot<T>)->Option<T>{return snapshot.value}
