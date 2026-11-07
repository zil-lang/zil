pub fn upper(byte: u8) -> u8 {
    if byte >= u8(97) && byte <= u8(122) { return byte - u8(32) }
    return byte
}
