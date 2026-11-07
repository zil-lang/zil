import portable

resource Mark { code: i32 }
impl Drop for Mark { fn drop(self: Self) { portable.emit(.code) } }

pub package State<T,F,G> {
    manages marker: Mark
    pub active: F
    once: G
    tag: T
}

pub fn pack<T: Copy, F: FnMut(&mut [u8]) -> i32, G: FnOnce() -> i32>(
    active: F, once: G, tag: T, marker: i32
) -> State<T,F,G> {
    return State::<T,F,G> {
        marker: Mark { code: marker }, active: active, once: once, tag: tag
    }
}

pub fn invoke<F: FnMut(&mut [u8]) -> i32>(
    callback: &mut F, values: &mut [u8], expected: i32
) -> i32 {
    let result = callback(values)
    if result < 0 { return 1 }
    if result != expected { return 8 }
    return 0
}

pub fn finish<T: Copy, F: FnMut(&mut [u8]) -> i32, G: FnOnce() -> i32>(
    state: State<T,F,G>, values: &mut [u8], expected: i32
) -> (i32, T) {
    let tag = state.tag
    var last = state
    portable.emit(80)
    let consumed = last.once()
    portable.emit(81)
    if consumed != 1 { return (8, tag) }
    let result = last.active(values)
    if result < 0 { return (1, tag) }
    if result != expected { return (8, tag) }
    return (0, tag)
}
