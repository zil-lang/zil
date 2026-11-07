package Parent { manages values:Store<u64> }
fn result()->u64 {
    let values:Store<u64>=store_new()
    store_reserve(values,2)
    store_push(values,18446744073709551610u64)
    store_push(values,3u64)
    let parent=Parent{values:give values}
    let first=store_at_mut(parent.values,0)
    let second=store_at_mut(parent.values,1)
    *second=4u64
    return *first+*second
}
