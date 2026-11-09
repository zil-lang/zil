package Holder<T:StoreElement> { manages items:Store<T> }
fn replace<T:StoreElement>(holder:&mut Holder<T>,give next:Store<T>) { holder.items=give next }
fn invoke<F:FnMut()->void>(action:&mut F) { action() }
fn forward<F:FnMut()->void>(action:&mut F) { invoke(action) }
fn main()->i32 {
    let original:Store<i64>=store_new()
    if !store_reserve(original,1) {return 1}
    store_push(original,11)
    var holder=Holder::<i64>{items:give original}
    let old=holder.items
    if store_len(old)!=1 {return 2}
    let target=&mut holder
    var action=()=>{
        let next:Store<i64>=store_new()
        if store_reserve(next,1) {
            store_push(next,22)
            replace(target,give next)
        }
    }
    forward(&mut action)
    return i32(*store_at(holder.items,0))-22
}
