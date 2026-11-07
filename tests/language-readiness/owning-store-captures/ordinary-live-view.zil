package Parent { manages values:Store<u64> }
fn make(n:u64)->Parent {
    let values:Store<u64>=store_new()
    store_reserve(values,1)
    store_push(values,n)
    return Parent{values:give values}
}
fn result()->u64 {
    var parent=make(18446744073709551610u64)
    let view=store_at(parent.values,0)
    parent=make(18446744073709551611u64)
    return *view
}
