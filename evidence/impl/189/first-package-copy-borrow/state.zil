import helpers

pub package State<F> { manages observed: ByteBuffer; callback: F; next: i32; count: i64 }

pub fn make<F: FnMut(&mut i32) -> i32>(callback: F, next: i32) -> State<F> {
    return State::<F> { observed: bytes_new(), callback: callback, next: next, count: 0 }
}

pub fn run<F: FnMut(&mut i32) -> i32>(state: State<F>, count: i64) -> i32 {
    var current = state
    while current.count < count {
        let callback = &mut current.callback
        let next = &mut current.next
        let length = callback(next)
        current.count += 1
        if i64(length) != current.count + 1 { return 7 }
        bytes_reserve(current.observed, current.count)
        bytes_push(current.observed, u8(current.next - 1))
        let view = bytes_view(current.observed)
        helpers.emit(i32(view[current.count - 1]))
    }
    return 0
}
