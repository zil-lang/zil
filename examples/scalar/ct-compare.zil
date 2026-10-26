fn main() -> i32 { let x: i64 = 2147483648; if x == #(2147483647 + 1) { return 42 } else { return 0 } }
