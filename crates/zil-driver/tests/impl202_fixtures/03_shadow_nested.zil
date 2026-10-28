
fn empty<T: StoreElement>() -> Store<T> contract { result(create) } { return store_new() }
fn reserve<T: StoreElement>(items: Store<T>, count: i64) -> bool { return store_reserve(items, count) }
fn push<T: StoreElement>(items: Store<T>, value: T) -> T!void { return store_push(items, value) }
fn pop<T: StoreElement>(items: Store<T>) -> void!T { return store_pop(items) }

struct T { value:i32 }
fn concrete(value:T)->i32{return value.value}
fn nested_empty<U:StoreElement>()->Store<U> contract{result(create)}{return empty::<U>()}
fn middle<U:StoreElement>(items:Store<U>,count:i64)->bool{return reserve::<U>(items,count)}
fn shifted<U:StoreElement>(tag:i32,items:Store<U>,count:i64)->bool{tag;return middle::<U>(items,count)}
fn append<U:StoreElement>(items:Store<U>,value:U)->U!void{return push::<U>(items,value)}
extern "C" fn putchar(n: i32) -> i32;
resource Ticket { code: i32 }
impl Drop for Ticket { fn drop(self: Self) { unsafe { putchar(.code) } } }
package Pair<T> { manages root: Ticket; first: T; second: T }
package Item<T,T> { manages root: Ticket; callback: T; value: T }

fn make(label: i32, seed: i32) -> impl FnMut() -> i32 {
    let ticket = Ticket { code: label }
    var count = seed
    return move () => { ticket.code; count += 1; return count }
}
fn pair<T: FnMut() -> i32>(first: T, second: T) -> Pair<T> {
    return Pair::<T> { root: Ticket { code: 90 }, first: first, second: second }
}
fn run<T: FnMut() -> i32>(pair: Pair<T>) -> i32 {
    let items: Store<Item<i32,T>> = nested_empty::<Item<i32,T>>()
    if !shifted(0,items,2) { return 1 }
    append(items, Item::<i32,T> { root: Ticket { code: 49 }, callback: pair.first, value: 10 })
    append(items, Item::<i32,T> { root: Ticket { code: 50 }, callback: pair.second, value: 20 })
    let first = store_at_mut(items, 0)
    if first.callback() != 11 { return 2 }
    if first.callback() != 12 { return 3 }
    let second = store_at_mut(items, 1)
    if second.callback() != 21 { return 4 }
    if store_at_mut(items, 0).callback() != 13 { return 5 }
    return 0
}
fn main() -> i32 { return concrete(T{value:0})+run(pair(make(65, 10), make(66, 20))) }
