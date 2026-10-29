fn main() -> i32 { var values = [7, 9]; let f = (a: &[i32], b: &mut [i32]) => { b[0] = a[1]; return b[0] }; return f(&values[..], &mut values[..]) }
