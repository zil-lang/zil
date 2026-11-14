import portable

resource Mark { code: i32 }
impl Drop for Mark { fn drop(self: Self) { portable.emit(.code) } }

pub package State<T,F> {
    manages marker: Mark
    pub a: F
    pub b: F
    pub tag: T
}

pub fn pack<T: Copy, F: FnMut(i32) -> i32>(a: F, b: F, tag: T) -> State<T,F> {
    return State::<T,F> { marker: Mark { code: 83 }, a: a, b: b, tag: tag }
}

pub fn replace<T: Copy, F: FnMut(i32) -> i32>(
    state: State<T,F>, replacement: F, cancel: bool
) -> State<T,F> {
    var next = state
    if cancel { portable.emit(75); return next }
    portable.emit(80)
    next.a = replacement
    portable.emit(81)
    return next
}

pub fn replace_first<T: Copy, F: FnMut(i32) -> i32>(
    replacement: F, stamp: i64, state: State<T,F>, cancel: bool
) -> State<T,F> {
    if stamp != i64(42) { return state }
    return replace(state, replacement, cancel)
}

pub fn consume_replace<T: Copy, F: FnOnce(i32) -> i32>(
    state: State<T,F>, replacement: F
) -> State<T,F> {
    var next = state
    portable.emit(85)
    let result = next.a(0)
    if result < 0 { portable.emit(70) }
    portable.emit(86)
    next.a = replacement
    portable.emit(81)
    return next
}

pub fn invoke<F: FnMut(i32) -> i32>(callback: &mut F, expected: i32) -> i32 {
    let result = callback(0)
    if result < 0 { return 1 }
    if result != expected { return 8 }
    return 0
}
