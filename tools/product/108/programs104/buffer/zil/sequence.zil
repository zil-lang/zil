pub fn uppercase(items: &mut [u8]) {
    var index: i64 = 0
    while index < items.len() {
        let byte = items[index]
        if byte >= u8(97) && byte <= u8(122) { items[index] = byte - u8(32) }
        index += 1
    }
}
