import adapter
import model
import ticket

fn route(event: i32) -> i32 {
    let payload = ticket.make(event)
    let outer = move () -> i32 => {
        let inner = move () -> i32 => {
            let cursor = model.cursor(event)
            let value = adapter.read(&cursor)
            let first = payload.read()
            let last = (give payload).take()
            return first + value + last
        }
        return inner()
    }
    return outer()
}

fn main() -> i32 {
    let output = bytes_new()
    if !bytes_reserve(output, 1024) { return 1 }
    for event in 0..1024 {
        let result = route(event)
        if result != event * 3 { return 2 }
        if !bytes_push(output, u8(result % 256)) { return 3 }
    }
    let view = bytes_view(output)
    if view.len() != 1024 { return 4 }
    for i in 0..view.len() {
        if view[i] != u8((i * 3) % 256) { return 5 }
    }
    return 0
}
