enum Choice { Value(Store<u64>) }
fn make(n:u64)->Store<u64> contract{result(create)} {
    let values:Store<u64>=store_new()
    store_reserve(values,1)
    store_push(values,n)
    return give values
}
fn dual_mutator()->impl FnMut()->u64 {
    var left=Choice.Value(make(11u64))
    var right=Choice.Value(make(13u64))
    return move()=>{
        let old_left=left
        left=Choice.Value(make(17u64))
        let old_right=right
        right=Choice.Value(make(19u64))
        match old_left { Choice.Value(values)=>{return *store_at(values,0)} }
    }
}
fn result()->u64 {
    var f=dual_mutator()
    return f()+f()
}
