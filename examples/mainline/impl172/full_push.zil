import model
import storage

fn main() -> model.Item!i32 {
    let value = storage.empty()
    let tag = model.Ticket { code: 90 }
    let data = bytes_new()
    bytes_reserve(data, 1)
    bytes_push(data, u8(90))
    store_push(value, model.Item { ticket: give tag, data: give data, code: 90 })?
    return 0
}
