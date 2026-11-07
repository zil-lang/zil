struct Cell { n:u64 }
package Holder { manages data:Store<Cell> }
enum Choice { First(Holder), Second(Holder) }
enum Fault { Bad }
fn make(n:u64)->Holder {
    let items:Store<Cell>=store_new()
    store_reserve(items,1)
    store_push(items,Cell{n:n})
    return Holder{data:give items}
}
fn relay(value:(Holder,i32))->(Holder,i32) { return value }
fn fallible(value:Holder,fail:bool)->Fault!Holder {
    if fail { return Fault.Bad() }
    return value
}
fn result()->u64 {
    let (first,mark)=relay((make(18446744073709551612u64),7))
    if mark!=7 { return 1u64 }
    var sum=Choice.First(first)
    sum=Choice.Second(make(18446744073709551613u64))
    match sum {
        Choice.First(item)=>{return store_at(item.data,0).n},
        Choice.Second(item)=>{
            match fallible(item,false) {
                .ok(ready)=>{return store_at(ready.data,0).n},
                .err(_)=>{return 2u64}
            }
        }
    }
}
