struct Cell { n:u64 }
package Parent { manages data:Store<Cell> }
fn make(n:u64)->Parent {
    let data:Store<Cell>=store_new()
    store_reserve(data,1)
    store_push(data,Cell{n:n})
    return Parent{data:give data}
}
fn read()->impl Fn()->u64 {
    let parent=make(18446744073709551610u64)
    return move()=>store_at(parent.data,0).n
}
fn change()->impl FnMut()->u64 {
    let parent=make(18446744073709551611u64)
    return move()=>{let item=store_at_mut(parent.data,0);item.n+=1u64;return item.n}
}
fn relay(value:Parent)->Parent { return value }
fn consume()->impl FnOnce()->u64 {
    let parent=make(18446744073709551612u64)
    return move()=>{let complete=relay(parent);return store_at(complete.data,0).n}
}
fn result()->u64 {
    let f=read()
    var g=change()
    let h=consume()
    if f()!=18446744073709551610u64 { return 1u64 }
    if g()!=18446744073709551612u64 { return 2u64 }
    return h()
}
