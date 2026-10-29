import adapter
import model
import ticket

enum E { Fail }
fn fail() -> E!i32 => E.Fail
fn route(event: i32) -> E!i32 {
    let payload = ticket.make(event)
    let outer = move () -> E!i32 => {
        let inner = move () -> E!i32 => {
            let cursor = model.cursor(event)
            let checked = adapter.read(&cursor)
            if checked != payload.read() { return 7 }
            return (give payload).consume(fail()?)
        }
        return inner()?
    }
    return outer()?
}
fn main() -> E!i32 {
    let output = bytes_new()
    if !bytes_reserve(output, 64) { return E.Fail }
    if !bytes_push(output, 42u8) { return E.Fail }
    return route(21)?
}
