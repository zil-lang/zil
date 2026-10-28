import helpers

pub package State<F> { manages storage: ByteBuffer; callback: F; count: i64 }

pub fn make<F: FnMut(u8) -> &[u8]>(callback: F) -> State<F> {
    return State::<F> { storage: bytes_new(), callback: callback, count: 0 }
}

pub fn run<F: FnMut(u8) -> &[u8]>(state: State<F>, count: i64) -> i32 {
    var current = state
    while current.count < count {
        let callback = &mut current.callback
        let view = callback(u8(current.count + 66))
        current.count += 1
        if view.len() != current.count { return 1 }
        helpers.emit(i32(view[current.count - 1]))
    }
    return 0
}
