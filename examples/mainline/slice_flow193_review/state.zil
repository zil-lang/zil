import helpers
pub package State<F> { manages observed: ByteBuffer; callback: F; counter: i32 }
pub fn make<F: FnMut(&mut [u8], &mut i32) -> i32>(callback: F) -> State<F> {
    return State::<F> { observed: bytes_new(), callback: callback, counter: 65 }
}
pub fn run<F: FnMut(&mut [u8], &mut i32) -> i32>(state: State<F>, count: i64) -> i32 {
    var current = state
    if !bytes_reserve(current.observed, 1) { return 1 }
    if !bytes_push(current.observed, 0u8) { return 2 }
    let window = bytes_view_mut(current.observed)
    let empty = helpers.repeat(&mut current.callback, window, &mut current.counter, 0)
    if empty != 0 { return 6 }
    return helpers.repeat(&mut current.callback, window, &mut current.counter, count)
}
