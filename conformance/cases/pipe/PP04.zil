fn inc(x: i32) -> i32 => x + 1
fn choose(flag: bool) -> fn(i32) -> i32 { return inc }
fn main() -> i32 { return 3 |> choose(true) }
