pub fn initialize<F: Fn(i64) -> i64>(f: &F, seed: i64) -> i64 {
    return f(seed);
}

pub fn warm<F: FnMut(i64) -> i64>(f: &mut F) -> i64 {
    return f(1) + f(2);
}
