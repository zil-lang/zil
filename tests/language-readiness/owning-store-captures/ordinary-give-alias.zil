package Parent { manages values:Store<u64> }
fn make()->Parent {
    let values:Store<u64>=store_new()
    store_reserve(values,1)
    store_push(values,18446744073709551610u64)
    return Parent{values:give values}
}
fn result()->u64 {
    var parent=make()
    let alias=parent.values
    parent.values=give alias
    return *store_at(parent.values,0)
}
