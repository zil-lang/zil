
fn empty<T: StoreElement>() -> Store<T> contract { result(create) } { return store_new() }
fn reserve<T: StoreElement>(items: Store<T>, count: i64) -> bool { return store_reserve(items, count) }
fn push<T: StoreElement>(items: Store<T>, value: T) -> T!void { return store_push(items, value) }
fn pop<T: StoreElement>(items: Store<T>) -> void!T { return store_pop(items) }

extern "C" fn putchar(n: i32) -> i32;
resource Ticket { code: i32 }
impl Drop for Ticket { fn drop(self: Self) { unsafe { putchar(.code) } } }
package Pair<F> { manages root: Ticket; first: F; second: F }
package Item<T,F> { manages root: Ticket; callback: F; value: T }

fn make(label: i32, seed: i32) -> impl FnMut() -> i32 {
    let ticket = Ticket { code: label }
    var count = seed
    return move () => { ticket.code; count += 1; return count }
}
fn pair<F: FnMut() -> i32>(first: F, second: F) -> Pair<F> {
    return Pair::<F> { root: Ticket { code: 90 }, first: first, second: second }
}
fn run<F: FnMut() -> i32>(pair: Pair<F>) -> i32 {
    let items: Store<Item<i32,F>> = empty()
    if !reserve(items, 2) { return 1 }
    store_push(items, Item::<i32,F> { root: Ticket { code: 49 }, callback: pair.first, value: 10 })
    store_push(items, Item::<i32,F> { root: Ticket { code: 50 }, callback: pair.second, value: 20 })
    let first = store_at_mut(items, 0)
    if first.callback() != 11 { return 2 }
    if first.callback() != 12 { return 3 }
    let second = store_at_mut(items, 1)
    if second.callback() != 21 { return 4 }
    if store_at_mut(items, 0).callback() != 13 { return 5 }
    return 0
}
fn main() -> i32 { return run(pair(make(65, 10), make(66, 20))) }
