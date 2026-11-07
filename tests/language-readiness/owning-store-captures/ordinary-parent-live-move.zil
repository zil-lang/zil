package Parent { manages values:Store<u64> }
fn make()->Parent {
    let values:Store<u64>=store_new()
    store_reserve(values,1)
    store_push(values,18446744073709551610u64)
    return Parent{values:give values}
}
fn relay(value:Parent)->Parent { return value }
fn result()->u64 {
    let parent=make()
    let view=store_at(parent.values,0)
    let transferred=relay(parent)
    return *view+u64(store_len(transferred.values))
}
