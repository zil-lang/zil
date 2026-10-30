import config
import io

enum Failure { Allocation }
resource Marker { code: i32 }
impl Drop for Marker { fn drop(self: Self) { io.emit(.code) } }

fn drive(input: i32) -> Failure!i32 {
    #for (i in 0..config.rounds()) {
        let marker = Marker { code: 65 + i }
        let buffer = bytes_new()
        if !bytes_reserve(buffer, i64(i + 1)) { return Failure.Allocation }
        if bytes_capacity(buffer) < i64(i + 1) { return 9 }
        io.emit(97 + i)
        if input == 48 + i { return i }
    }
    io.emit(90)
    return 0
}

fn main() -> Failure!i32 {
    let input = io.read()
    if input < 48 || input > 51 { return 9 }
    let frame = Marker { code: 82 }
    return drive(input)?
}
