import std.store
struct Cell { n:u64; stamp:i32 }
package Pair { manages left:Store<u64>; manages right:Store<Cell> }
fn result()->u64 {
    let left:Store<u64>=store.new()
    if !store.reserve(left,1) { return 101u64 }
    store.push(left,18446744073709551610u64)
    let right:Store<Cell>=store.new()
    if !store.reserve(right,1) { return *store_at(left,0) }
    store.push(right,Cell{n:18446744073709551611u64,stamp:7})
    let pair=Pair{left:give left,right:give right}
    if !store.reserve(pair.left,4) {
        if store.len(pair.left)!=1 || store.len(pair.right)!=1 { return 102u64 }
        if store_at(pair.right,0).stamp!=7 { return 103u64 }
        return *store_at(pair.left,0)
    }
    if store_at(pair.right,0).n!=18446744073709551611u64 { return 104u64 }
    return *store_at(pair.left,0)+1u64
}
