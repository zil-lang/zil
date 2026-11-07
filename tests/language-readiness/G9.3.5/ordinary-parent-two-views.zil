package Pair { manages first:Store<u64>; manages second:Store<u64> }
fn result()->u64 {
    let a:Store<u64>=store_new()
    let b:Store<u64>=store_new()
    store_reserve(a,1)
    store_reserve(b,1)
    store_push(a,18446744073709551610u64)
    store_push(b,2u64)
    let parent=Pair{first:give a,second:give b}
    let first=store_at_mut(parent.first,0)
    let second=store_at_mut(parent.second,0)
    *second=3u64
    return *first+*second
}
