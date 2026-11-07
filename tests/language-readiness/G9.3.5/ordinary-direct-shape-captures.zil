enum Choice { Value(Store<u64>) }
fn make(n:u64)->Store<u64> contract{result(create)} {
    let values:Store<u64>=store_new()
    store_reserve(values,1)
    store_push(values,n)
    return give values
}
fn make_error()->Store<u64>!i32 { return make(18446744073709551614u64) }
fn relay_error(value:Store<u64>!i32)->Store<u64>!i32 { return value }

fn tuple_reader()->impl Fn()->u64 {
    let pair=(make(18446744073709551610u64),7)
    return move()=>{
        let (ref values,tag)=pair
        if tag!=7 { return 1u64 }
        return *store_at(values,0)
    }
}
fn enum_mutator()->impl FnMut()->u64 {
    var choice=Choice.Value(make(18446744073709551611u64))
    return move()=>{
        let previous=choice
        choice=Choice.Value(make(18446744073709551612u64))
        match previous { Choice.Value(values)=>{return *store_at(values,0)} }
    }
}
fn error_consumer()->impl FnOnce()->u64 {
    let outcome=make_error()
    return move()=>{
        let previous=relay_error(outcome)
        match previous {
            .ok(_)=>{return 2u64},
            .err(values)=>{return *store_at(values,0)}
        }
    }
}
fn result()->u64 {
    let reader=tuple_reader()
    var mutator=enum_mutator()
    let consumer=error_consumer()
    if reader()!=18446744073709551610u64 { return 3u64 }
    if mutator()!=18446744073709551611u64 { return 4u64 }
    if mutator()!=18446744073709551612u64 { return 5u64 }
    return consumer()
}
