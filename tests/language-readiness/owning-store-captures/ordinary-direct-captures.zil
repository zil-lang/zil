fn read()->impl Fn()->u64 {
    let data:Store<u64>=store_new()
    store_reserve(data,1)
    store_push(data,18446744073709551610u64)
    return move()=>*store_at(data,0)
}
fn change()->impl FnMut()->u64 {
    let data:Store<u64>=store_new()
    store_reserve(data,1)
    store_push(data,18446744073709551611u64)
    return move()=>{let value=store_at_mut(data,0);*value+=1u64;return *value}
}
fn relay(give data:Store<u64>)->Store<u64> contract{result(give)} { return give data }
fn consume()->impl FnOnce()->u64 {
    let data:Store<u64>=store_new()
    store_reserve(data,1)
    store_push(data,18446744073709551612u64)
    return move()=>{let owned=relay(give data);return *store_at(owned,0)}
}
fn result()->u64 {
    let f=read()
    var g=change()
    let h=consume()
    if f()!=18446744073709551610u64 { return 1u64 }
    if g()!=18446744073709551612u64 { return 2u64 }
    return h()
}
