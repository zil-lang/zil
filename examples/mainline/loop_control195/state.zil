import helpers

pub package State<F> { manages observed: ByteBuffer; callback: F }

pub fn make<F: FnMut(&mut i32) -> i32>(callback: F) -> State<F> {
    return State::<F> { observed: bytes_new(), callback: callback }
}

pub fn run<F: FnMut(&mut i32) -> i32>(state: State<F>, count: i64) -> i32 {
    var current = state
    var value = 65
    return helpers.repeat(&mut current.callback, &mut value, current.observed, count)
}
