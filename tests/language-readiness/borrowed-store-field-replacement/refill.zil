package Holder<T:StoreElement> { manages items:Store<T> }
fn refill<T:StoreElement>(holder:&mut Holder<T>,give next:Store<T>) {
    holder.items=give next
}
fn main()->i32 {
    let original:Store<i64>=store_new()
    if !store_reserve(original,1) {return 1}
    store_push(original,11)
    var holder=Holder::<i64>{items:give original}
    let replacement:Store<i64>=store_new()
    if !store_reserve(replacement,1) {return 2}
    store_push(replacement,22)
    refill(&mut holder,give replacement)
    return i32(*store_at(holder.items,0))-22
}
