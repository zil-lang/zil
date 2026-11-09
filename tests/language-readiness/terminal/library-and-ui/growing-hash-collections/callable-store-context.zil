resource Marker { n:i64 }
package Holder<T:StoreElement> { manages items:Store<T>; manages marker:Marker }
fn replace<T:StoreElement>(holder:&mut Holder<T>,give next:Store<T>) {
    holder.items=give next
    holder.marker=Marker{n:99}
}
fn refill<T:Copy+StoreElement,E:Fn()->i64>(holder:&mut Holder<T>,value:T,observe:&E)->bool {
    observe()
    let next:Store<T>=store_new()
    if !store_reserve(next,1) {return false}
    store_push(next,value)
    replace(holder,give next)
    observe()
    return true
}
fn forward<T:Copy+StoreElement,E:Fn()->i64>(holder:&mut Holder<T>,value:T,observe:&E)->bool {
    return refill(holder,value,observe)
}
fn main()->i32 {
    let original:Store<i64>=store_new()
    if !store_reserve(original,1) {return 1}
    store_push(original,11)
    var holder=Holder::<i64>{items:give original,marker:Marker{n:11}}
    let separate:Store<i64>=store_new()
    if !store_reserve(separate,1) {return 2}
    store_push(separate,77)
    var other=Holder::<i64>{items:give separate,marker:Marker{n:77}}
    let old=holder.items
    if store_len(old)!=1 {return 3}
    let observed=other.marker
    let observe=()=>observed.n
    if !forward(&mut holder,22,&observe) {return 4}
    return i32(*store_at(holder.items,0))-22
}
