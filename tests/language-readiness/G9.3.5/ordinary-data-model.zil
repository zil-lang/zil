import std.store

struct Cell { n:u64; tag:i32 }
package Pair { manages numbers:Store<u64>; manages records:Store<Cell>; stamp:i32 }
package Root { child:Pair; serial:i32 }

fn numbers(n:u64)->Store<u64> contract{result(create)} {
    let values:Store<u64>=store.new()
    if !store.reserve(values,1) { return give values }
    store.push(values,n)
    return give values
}
fn records(n:u64)->Store<Cell> contract{result(create)} {
    let values:Store<Cell>=store.new()
    if !store.reserve(values,1) { return give values }
    store.push(values,Cell{n:n,tag:7})
    return give values
}
fn make(n:u64)->Root {
    return Root{child:Pair{numbers:numbers(n),records:records(n+1u64),stamp:9},serial:11}
}
fn relay(value:Root)->Root { return value }
fn checked(value:Root)->Root {
    var root=value
    var pair=root.child
    let numbers=give pair.numbers
    pair.numbers=give numbers
    root.child=pair
    return root
}
fn replace(target:&mut Root,n:u64) { *target=make(n) }
fn result()->u64 {
    var root=checked(relay(make(18446744073709551610u64)))
    let pair=root.child
    let before=store_at(pair.numbers,0)
    if *before!=18446744073709551610u64 { return 1u64 }
    root.child=pair
    replace(&mut root,18446744073709551612u64)
    let serial=root.serial
    let pair=root.child
    let scalar=store_at(pair.numbers,0)
    let record=store_at(pair.records,0)
    if pair.stamp!=9 || serial!=11 || record.tag!=7 || record.n!=18446744073709551613u64 { return 2u64 }
    return *scalar
}
