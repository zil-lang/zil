import model
import storage

fn main() -> model.Item!i32 {
    let value = storage.empty()
    store_push(value, model.lossy(90))?
    return 0
}
