fn share<T: Share>(value: T) -> T { return value }
fn main() -> i32 { return share(7) - 7 }
