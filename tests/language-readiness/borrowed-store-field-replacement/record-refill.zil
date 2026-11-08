struct Leaf{n:i64}
package Holder<T:StoreElement> { manages items:Store<T> }
fn refill<T:StoreElement>(holder:&mut Holder<T>,give next:Store<T>) {
    holder.items=give next
}
fn main()->i32 {
    let original:Store<Leaf>=store_new()
    if !store_reserve(original,1) {return 1}
    store_push(original,Leaf{n:11})
    var holder=Holder::<Leaf>{items:give original}
    let replacement:Store<Leaf>=store_new()
    if !store_reserve(replacement,1) {return 2}
    store_push(replacement,Leaf{n:22})
    refill(&mut holder,give replacement)
    let value=*store_at(holder.items,0)
    return i32(value.n)-22
}
