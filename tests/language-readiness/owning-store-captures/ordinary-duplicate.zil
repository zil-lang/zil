package Parent { manages values:Store<u64> }
package Pair { first:Parent; second:Parent }
fn make()->Parent {
    let values:Store<u64>=store_new()
    store_reserve(values,1)
    store_push(values,18446744073709551610u64)
    return Parent{values:give values}
}
fn result()->u64 {
    let parent=make()
    let duplicated=Pair{first:parent,second:parent}
    return *store_at(duplicated.first.values,0)
}
