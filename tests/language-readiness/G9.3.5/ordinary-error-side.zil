fn fail()->Store<u64>!i32 {
    let values:Store<u64>=store_new()
    store_reserve(values,1)
    store_push(values,18446744073709551610u64)
    return give values
}
fn result()->u64 {
    match fail() {
        .ok(_)=>{return 1u64},
        .err(values)=>{return *store_at(values,0)}
    }
}
