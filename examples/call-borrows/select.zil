pub fn choose(a: &i64, b: &i64, first: bool) -> &i64 {
    return if first { a } else { b }
}

pub fn relay(a: &i64, b: &i64, depth: i32) -> &i64 {
    if depth == 0 { return choose(a, b, false) }
    return relay(a, b, depth - 1)
}

pub fn read(p: &i64) -> i64 { return *p }
