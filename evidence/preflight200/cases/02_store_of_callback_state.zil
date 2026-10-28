package State<T,F> { manages bytes: ByteBuffer; callback: F; value: T }

fn make_callback(seed: i32) -> impl Fn() -> i32 {
    let log = bytes_new()
    return move () => { bytes_len(log); return seed }
}

fn pack<T: Copy, F: Fn() -> i32>(callback: F, value: T) -> State<T,F> {
    return State::<T,F> { bytes: bytes_new(), callback: callback, value: value }
}

fn enqueue<F: Fn() -> i32>(state: State<i32,F>) -> i32 {
    let items: Store<State<i32,F>> = store_new()
    if !store_reserve(items, 1) { return 1 }
    store_push(items, state)
    return 0
}

fn main() -> i32 {
    return enqueue(pack(make_callback(40), 2))
}
