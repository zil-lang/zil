fn transfer<T: Transfer>(value: T) -> T { return value }
fn main() -> i32 { return transfer(7) - 7 }
