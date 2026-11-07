pub fn leaf<F: FnMut(&mut [u8]) -> i32>(
    first: &mut F, second: &mut F, values: &mut [u8], expected: i32
) -> i32 {
    let a = first(values)
    if a < 0 { return 1 }
    if a != expected { return 8 }
    let b = second(values)
    if b < 0 { return 1 }
    if b != expected { return 8 }
    return 0
}

pub fn drive<T: Copy, F: FnMut(&mut [u8]) -> i32>(
    first: &mut F, second: &mut F, values: &mut [u8], count: i32, tag: T
) -> (i32, T) {
    var at = 0
    var completed = 0
    while at < count {
        if at == 1 {
            at += 1
            continue
        }
        if at == 4 { break }
        let forward = leaf(first, second, values, completed * 2 + 1)
        if forward != 0 { return (forward, tag) }
        let reverse = leaf(second, first, values, completed * 2 + 2)
        if reverse != 0 { return (reverse, tag) }
        completed += 1
        at += 1
    }
    return (0, tag)
}
