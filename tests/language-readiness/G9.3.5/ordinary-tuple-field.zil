package Carrier { data:(Store<u64>,i32); stamp:i32 }
fn make()->Store<u64> contract{result(create)} {
    let values:Store<u64>=store_new()
    store_reserve(values,1)
    store_push(values,18446744073709551611u64)
    return give values
}
fn relay(value:Carrier)->Carrier { return value }
fn result()->u64 {
    let carrier=relay(Carrier{data:(make(),7),stamp:9})
    let stamp=carrier.stamp
    let data=carrier.data
    let (values,tag)=data
    if stamp!=9 || tag!=7 { return 1u64 }
    return *store_at(values,0)
}
