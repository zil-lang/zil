package Holder<T:StoreElement> { manages items:Store<T> }
fn refill<T:Copy+StoreElement,E:Fn()->i64>(holder:&mut Holder<T>,value:T,observe:&E)->bool {
    observe()
    let next:Store<T>=store_new()
    if !store_reserve(next,1) {return false}
    store_push(next,value)
    holder.items=give next
    return true
}
fn main()->i32 {
    let original:Store<i64>=store_new()
    if !store_reserve(original,1) {return 1}
    store_push(original,11)
    var holder=Holder::<i64>{items:give original}
    let observe=()=>0
    if !refill(&mut holder,22,&observe) {return 2}
    return i32(*store_at(holder.items,0))-22
}
