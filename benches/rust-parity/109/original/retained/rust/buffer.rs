pub fn create() -> Vec<u8> { Vec::new() }
pub fn append(value: &mut Vec<u8>, byte: u8) -> bool {
    let capacity = value.capacity();
    if value.len() == capacity {
        let next = if capacity == 0 { 8 } else {
            if capacity > 4_611_686_018_427_387_903 { return false; }
            capacity * 2
        };
        if value.try_reserve_exact(next - value.len()).is_err() { return false; }
    }
    value.push(byte);
    true
}
