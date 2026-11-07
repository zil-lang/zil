enum Choice { Value(Store<u64>) }
fn make(n:u64)->Store<u64> contract{result(create)} {
    let values:Store<u64>=store_new()
    store_reserve(values,1)
    store_push(values,n)
    return give values
}
fn fail()->i32!u64 { let code:i32=7; return code }
fn mutable()->impl FnMut()->i32!u64 {
    var choice=Choice.Value(make(11u64))
    return move()=>{
        let previous=choice
        choice=Choice.Value(make(12u64))
        if store_len(make(1u64))==1 { return fail() }
        match previous { Choice.Value(values)=>{return *store_at(values,0)} }
    }
}
fn result()->u64 {
    var f=mutable()
    match f() {
        .ok(n)=>{return n},
        .err(_)=>{return 17u64}
    }
}
