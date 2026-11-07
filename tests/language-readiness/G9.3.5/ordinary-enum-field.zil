enum Choice { First(Store<u64>), Second(Store<u64>) }
package Carrier { choice:Choice; stamp:i32 }
fn make(n:u64)->Store<u64> contract{result(create)} {
    let values:Store<u64>=store_new()
    store_reserve(values,1)
    store_push(values,n)
    return give values
}
fn result()->u64 {
    var carrier=Carrier{choice:Choice.First(make(18446744073709551610u64)),stamp:7}
    carrier.choice=Choice.Second(make(18446744073709551612u64))
    let stamp=carrier.stamp
    let choice=carrier.choice
    if stamp!=7 { return 1u64 }
    match choice {
        Choice.First(values)=>{return *store_at(values,0)},
        Choice.Second(values)=>{return *store_at(values,0)}
    }
}
