package Parent { manages values:Store<u64> }
package Pair { first:Parent; second:Parent }
fn make(n:u64)->Parent {
    let values:Store<u64>=store_new()
    store_reserve(values,1)
    store_push(values,n)
    return Parent{values:give values}
}
fn result()->u64 {
    let first=make(18446744073709551610u64)
    let second=make(18446744073709551611u64)
    let pair=Pair{first:first,second:second}
    return *store_at(pair.second.values,0)
}
