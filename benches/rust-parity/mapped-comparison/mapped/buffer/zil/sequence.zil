// The same generic algorithms accept fixed arrays and dynamic storage windows.
pub fn tail<T>(items: &[T], start: i64) -> &[T] {
    return &items[start..]
}

pub fn tail_mut<T>(items: &mut [T], start: i64) -> &mut [T] {
    return &mut items[start..]
}

pub fn put<T>(items: &mut [T], index: i64, value: T) {
    items[index] = value
}

// A value operation: checked subtraction is evaluated only for lowercase ASCII.
pub fn ascii_upper(byte: u8) -> u8 {
    if byte >= u8(97) && byte <= u8(122) {
        return byte - u8(32)
    }
    return byte
}

// Every element of the explicitly mutable window is assigned its mapped value.
pub fn uppercase(items: &mut [u8]) {
    var index: i64 = 0
    while index < items.len() {
        let byte = items[index]
        put(items, index, ascii_upper(byte))
        index += 1
    }
}
