package Pair<T,F> { manages bytes: ByteBuffer; first: F; second: F; value: T }
package Item<T,F> { manages bytes: ByteBuffer; callback: F; value: T }

fn make_callback(seed: i32) -> impl Fn() -> i32 {
    let log = bytes_new()
    return move () => { bytes_len(log); return seed }
}

fn pair<T: Copy, F: Fn() -> i32>(first: F, second: F, value: T) -> Pair<T,F> {
    return Pair::<T,F> {
        bytes: bytes_new(), first: first, second: second, value: value
    }
}

fn split_and_consume<F: Fn() -> i32>(state: Pair<i32,F>) -> void!i32 {
    let items: Store<Item<i32,F>> = store_new()
    if !store_reserve(items, 2) { return () }
    let first = Item::<i32,F> {
        bytes: bytes_new(), callback: state.first, value: state.value
    }
    let second = Item::<i32,F> {
        bytes: bytes_new(), callback: state.second, value: state.value
    }
    store_push(items, first)
    store_push(items, second)
    let tail = store_pop(items)?
    let head = store_pop(items)?
    return head.callback() + tail.callback() + head.value + tail.value - 42
}

fn main() -> void!i32 {
    return split_and_consume(pair(make_callback(17), make_callback(23), 1))
}
