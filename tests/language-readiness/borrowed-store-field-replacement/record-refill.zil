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
    if !store_reserve(replacement,3) {return 2}
    store_push(replacement,Leaf{n:22})
    store_push(replacement,Leaf{n:33})
    store_push(replacement,Leaf{n:44})
    refill(&mut holder,give replacement)
    if store_len(holder.items)!=3 || store_capacity(holder.items)<3 {return 4}
    let value=*store_at(holder.items,0)
    let last=*store_at(holder.items,2)
    return i32(value.n+last.n)-66
}
