import config
import io

enum Failure { Allocation }
resource Marker { code: i32 }
impl Drop for Marker { fn drop(self: Self) { io.emit(.code) } }

#if (config.compact()) {
    fn capacity() -> i64 { return 2 }
} else {
    fn capacity() -> i64 { return 4 }
}

fn drive(input: i32) -> Failure!i32 {
    let outer = Marker { code: 90 }
    #if (config.compact()) {
        let branch = Marker { code: 97 }
        let data = bytes_new()
        if !bytes_reserve(data, 2) { return Failure.Allocation }
        io.emit(65)
    } else {
        let branch = Marker { code: 98 }
        let data = bytes_new()
        if !bytes_reserve(data, 4) { return Failure.Allocation }
        io.emit(66)
    }
    if input == 48 {
        io.emit(input)
        return 0
    }
    let after = Marker { code: 121 }
    if !bytes_push(data, u8(input)) { return 8 }
    io.emit(input)
    if input == 49 { return 0 }
    if !bytes_reserve(data, capacity() + 3) { return Failure.Allocation }
    io.emit(71)
    if input == 50 { return 2 }
    #if (config.compact()) {
        let extra = bytes_new()
        if !bytes_reserve(extra, 1) { return Failure.Allocation }
        let extra_capacity: i64 = 1
    } else {
        let extra = bytes_new()
        if !bytes_reserve(extra, 3) { return Failure.Allocation }
        let extra_capacity: i64 = 3
    }
    io.emit(69)
    if bytes_len(data) != 1 || bytes_capacity(extra) != extra_capacity { return 8 }
    return 0
}

fn main() -> Failure!i32 {
    let input = io.read()
    if input < 48 || input > 51 { return 9 }
    let frame = Marker { code: 82 }
    return drive(input)?
}
