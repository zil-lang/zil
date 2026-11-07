struct Cell { n:u64 }
package Pair { manages left:Store<u64>; manages right:Store<Cell> }
enum Fault { Bad }
fn fail()->Fault!u64 { return Fault.Bad() }
fn make_left(n:u64)->Store<u64> contract{result(create)} {
    let values:Store<u64>=store_new()
    store_reserve(values,1)
    store_push(values,n)
    return give values
}
fn make_right(n:u64)->Store<Cell> contract{result(create)} {
    let values:Store<Cell>=store_new()
    store_reserve(values,1)
    store_push(values,Cell{n:n})
    return give values
}
fn stage(give prepared:Store<Cell>,n:u64)->Store<Cell> contract{result(give)} {
    return give prepared
}
fn replace(target:&mut Pair)->Fault!u64 {
    target.right=stage(make_right(18446744073709551613u64),fail()?)
    return 0u64
}
fn result()->u64 {
    var pair=Pair{left:make_left(18446744073709551610u64),right:make_right(18446744073709551612u64)}
    match replace(&mut pair) {
        .ok(_)=>{return 1u64},
        .err(_)=>{
            if *store_at(pair.left,0)!=18446744073709551610u64 { return 2u64 }
            return store_at(pair.right,0).n
        }
    }
}
