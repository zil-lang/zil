import buffer
import sequence

fn consume(give value: ByteBuffer) -> i64 {
    let data = buffer.view(value)
    var total: i64 = 0
    var index: i64 = 0
    while index < data.len() {
        total += i64(data[index])
        index += 1
    }
    return total
}

fn main() -> i32 {
    let bytes = buffer.create()
    if bytes_len(bytes) != 0 || bytes_capacity(bytes) != 0 { return 1 }
    if bytes_view(bytes).len() != 0 { return 2 }
    if bytes_push(bytes, u8(1)) { return 3 }
    if bytes_reserve(bytes, -1) { return 4 }
    if !bytes_reserve(bytes, 0) { return 5 }
    var i: i64 = 0
    while i < 1000 {
        if !buffer.append(bytes, u8(i % 251)) { return 6 }
        i += 1
    }
    if bytes_len(bytes) != 1000 || bytes_capacity(bytes) < 1000 { return 7 }
    let before = buffer.view(bytes)
    if before[0] != u8(0) || before[999] != u8(246) { return 8 }
    sequence.put(buffer.view_mut(bytes), 0, u8(7))
    let moved = buffer.relay(give bytes)
    if consume(give moved) != 124513 { return 9 }
    return 0
}
