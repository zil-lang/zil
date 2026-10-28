fn main() -> i32 { let first = [7, 9]; var second = [2, 4]; let f = (a: &[i32], b: &mut [i32]) => { b[0] = a[1]; return b[0] }; return f(&first[..], &mut second[..]) - 9 }
