fn main() -> i32 { var i = 0; var sum = 0; while i < 10 { i += 1; if i == 3 { continue }; if i == 8 { break }; sum += i }; return sum }
