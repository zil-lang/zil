extern "C" fn putchar(value:i32)->i32;
fn tick(){unsafe{putchar(88)}}
enum Fault { Requested }
package Holder<T:StoreElement> { manages items:Store<T> }
fn fail()->Fault!i32 {return Fault.Requested()}
fn stage<T:StoreElement>(give prepared:Store<T>,n:i32)->Store<T> contract{result(give)} {
    tick()
    return give prepared
}
fn refill<T:StoreElement>(holder:&mut Holder<T>,give next:Store<T>)->Fault!i32 {
    (*holder).items=stage(give next,fail()?)
    return 0
}
fn main()->i32 {
    let original:Store<i64>=store_new()
    if !store_reserve(original,1) {return 1}
    store_push(original,11)
    var holder=Holder::<i64>{items:give original}
    let replacement:Store<i64>=store_new()
    if !store_reserve(replacement,1) {return 2}
    store_push(replacement,22)
    match refill(&mut holder,give replacement) {
        .ok(_)=>{return 3},
        .err(_)=>{
            if store_len(holder.items)!=1 {return 4}
            return i32(*store_at(holder.items,0))-11
        }
    }
}
