import helpers

pub package State<F> { manages observed: ByteBuffer; callback: F }

pub fn make<F: FnMut(&mut i32) -> i32>(callback: F) -> State<F> {
    return State::<F> { observed: bytes_new(), callback: callback }
}

pub fn run<F: FnMut(&mut i32) -> i32>(state: State<F>, first: i32, count: i64) -> i32 {
    var current = state
    var value = first
    return helpers.repeat(&mut current.callback, &mut value, current.observed, count)
}
