// Ordinary reusable sequence functions: no length template, allocation or UI.
pub fn sum(bytes: &[u8]) -> i64 {
    var index: i64 = 0
    var total: i64 = 0
    while index < bytes.len() {
        total += i64(bytes[index])
        index += 1
    }
    return total
}

pub fn uppercase_ascii(bytes: &mut [u8]) {
    var index: i64 = 0
    while index < bytes.len() {
        let byte = bytes[index]
        if byte >= 97u8 && byte <= 122u8 { bytes[index] = byte - 32u8 }
        index += 1
    }
}

pub fn tail(bytes: &[u8], start: i64) -> &[u8] { return &bytes[start..] }
pub fn relay(bytes: &[u8], start: i64) -> &[u8] { return tail(bytes, start) }
pub fn mutable_tail(bytes: &mut [u8], start: i64) -> &mut [u8] {
    return &mut bytes[start..]
}
