import helpers

pub package State<F> { callback: F; next: i32; count: i64 }

pub fn make<F: FnMut(&mut i32) -> i32>(callback: F, next: i32) -> State<F> {
    return State::<F> { callback: callback, next: next, count: 0 }
}

pub fn run<F: FnMut(&mut i32) -> i32>(state: State<F>, count: i64) -> i32 {
    var current = state
    while current.count < count {
        let callback = &mut current.callback
        let next = &mut current.next
        let length = callback(next)
        current.count += 1
        if i64(length) != current.count + 1 { return 7 }
        helpers.emit(current.next - 1)
    }
    return 0
}
