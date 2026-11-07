enum Choice { Value(Store<u64>) }
package Carrier { choice:Choice }
fn make()->Store<u64> contract{result(create)} {
    let values:Store<u64>=store_new()
    store_reserve(values,1)
    store_push(values,18446744073709551614u64)
    return give values
}
fn relay(value:Carrier)->Carrier { return value }
fn produce()->impl FnOnce()->u64 {
    let carrier=Carrier{choice:Choice.Value(make())}
    return move()=>{
        let owned=relay(carrier)
        let choice=owned.choice
        match choice { Choice.Value(values)=>{return *store_at(values,0)} }
    }
}
fn result()->u64 { let f=produce();return f() }
