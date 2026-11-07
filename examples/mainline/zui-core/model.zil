import portable

pub resource Mark { pub code: i32 }
impl Drop for Mark { fn drop(self: Self) { portable.emit(.code) } }

pub package Node {
    manages marker: Mark
    manages bytes: ByteBuffer
    pub id: i32
    pub ready: bool
}
pub package Subscription {
    manages marker: Mark
    manages bytes: ByteBuffer
    pub id: i32
    pub start: i32
    pub ready: bool
}
pub fn node(id: i32) -> Node {
    let marker = Mark { code: 65 + id }
    let bytes = bytes_new()
    let ready = bytes_reserve(bytes, 1)
    if ready { bytes_push(bytes, u8(id)) } else { portable.emit(70) }
    return Node { marker: give marker, bytes: give bytes, id: id, ready: ready }
}
pub fn subscription(id: i32, start: i32) -> Subscription {
    let marker = Mark { code: start + id }
    let bytes = bytes_new()
    let ready = bytes_reserve(bytes, 1)
    if ready { bytes_push(bytes, u8(id)) } else { portable.emit(70) }
    return Subscription { marker: give marker, bytes: give bytes, id: id, start: start, ready: ready }
}
