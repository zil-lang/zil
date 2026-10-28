package State<T,F> { manages bytes: ByteBuffer; first: F; second: F; value: T }

fn make_callback(seed: i32) -> impl Fn() -> i32 {
    let log = bytes_new()
    return move () => { bytes_len(log); return seed }
}

fn pack<T: Copy, F: Fn() -> i32>(first: F, second: F, value: T) -> State<T,F> {
    return State::<T,F> {
        bytes: bytes_new(), first: first, second: second, value: value
    }
}

fn inspect<F: Fn() -> i32>(state: State<i32,F>) -> i32 {
    return state.first() + state.second() + state.value
}

fn main() -> i32 {
    let state = pack(make_callback(17), make_callback(23), 2)
    return inspect(state) - 42
}
