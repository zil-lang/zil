import stdio

// Convert printable ASCII to uppercase, keeping tabs and line endings.
// Exit codes: 0 nonempty success, 2 invalid input, 3 empty/initial EOF,
// 4 output failure observed by putchar. Valid prefixes remain visible on error.
fn main() -> i32 {
    var seen = false
    loop {
        let value = stdio.read()
        if value < 0 { break }
        seen = true
        if (value < 32 && value != 9 && value != 10 && value != 13) || value > 126 {
            return 2
        }
        var output = value
        if value >= 97 && value <= 122 { output = value - 32 }
        if !stdio.write(output) { return 4 }
    }
    if !seen { return 3 }
    return 0
}
