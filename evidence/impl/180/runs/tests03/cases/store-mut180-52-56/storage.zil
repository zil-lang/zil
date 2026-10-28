import model

pub fn empty() -> Store<model.Item> contract { result(create) } { return store_new() }

pub fn fill(storage: Store<model.Item>, count: i32) -> model.Fault!void {
    if !store_reserve(storage, i64(count)) { return model.Fault.Memory }
    var i: i32 = 0
    while i < count {
        let item = model.make(65 + i)?
        // Explicit reserve guarantees the complete element can be inserted.
        store_push(storage, item)
        i += 1
    }
    return ()
}
