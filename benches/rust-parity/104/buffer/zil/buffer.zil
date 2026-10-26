// Growth policy is ordinary ZIL code; the primitive push never allocates.
pub fn create() -> ByteBuffer contract { result(create) } {
    return bytes_new()
}

pub fn relay(give value: ByteBuffer) -> ByteBuffer contract { result(give) } {
    return give value
}

pub fn append(value: ByteBuffer, byte: u8) -> bool {
    let length = bytes_len(value)
    let capacity = bytes_capacity(value)
    if length == capacity {
        var next: i64 = 8
        if capacity > 0 {
            if capacity > 4611686018427387903 { return false }
            next = capacity * 2
        }
        if !bytes_reserve(value, next) { return false }
    }
    return bytes_push(value, byte)
}

pub fn view(value: ByteBuffer) -> &[u8] {
    return bytes_view(value)
}

pub fn view_mut(value: ByteBuffer) -> &mut [u8] {
    return bytes_view_mut(value)
}
