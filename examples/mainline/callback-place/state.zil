pub package State<T,F> {
    manages bytes: ByteBuffer
    callback: F
    value: T
}

pub fn pack<T: Copy, F: FnMut(&mut T) -> i32>(callback: F, value: T) -> State<T,F> {
    return State::<T,F> { bytes: bytes_new(), callback: callback, value: value }
}

pub fn drive<T: Copy, G: FnMut(&mut T) -> i32>(state: State<T,G>, count: i32) -> i32 {
    var current = state
    var index = 0
    while index < count {
        if !bytes_reserve(current.bytes, i64(index + 1)) { return 1 }
        if !bytes_push(current.bytes, 0u8) { return 1 }
        let result = current.callback(&mut current.value)
        if result != 0 { return result }
        index += 1
    }
    return 0
}
