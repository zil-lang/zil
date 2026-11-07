package Carrier { outcome:Store<u64>!i32; stamp:i32 }
fn fail()->Store<u64>!i32 {
    let values:Store<u64>=store_new()
    store_reserve(values,1)
    store_push(values,18446744073709551613u64)
    return give values
}
fn result()->u64 {
    let carrier=Carrier{outcome:fail(),stamp:7}
    let stamp=carrier.stamp
    let outcome=carrier.outcome
    if stamp!=7 { return 1u64 }
    match outcome {
        .ok(_)=>{return 2u64},
        .err(values)=>{return *store_at(values,0)}
    }
}
