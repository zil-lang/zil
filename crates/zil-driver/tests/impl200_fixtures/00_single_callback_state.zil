package State<T,F> { manages bytes: ByteBuffer; callback: F; value: T }

fn make_callback(seed: i32) -> impl Fn() -> i32 {
    let log = bytes_new()
    return move () => { bytes_len(log); return seed }
}

fn pack<T: Copy, F: Fn() -> i32>(callback: F, value: T) -> State<T,F> {
    return State::<T,F> { bytes: bytes_new(), callback: callback, value: value }
}

fn inspect<F: Fn() -> i32>(state: State<i32,F>) -> i32 {
    return state.callback() + state.value
}

fn main() -> i32 {
    let state = pack(make_callback(40), 2)
    return inspect(state) - 42
}
