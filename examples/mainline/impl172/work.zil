import model
import storage

pub fn fill(value: Store<model.Item>, count: i32) -> model.Fault!void {
    if !store_reserve(value, 2) { return model.Fault.Memory }
    var index: i32 = 0
    while index < count {
        if store_len(value) == store_capacity(value) {
            if !store_reserve(value, store_capacity(value) + 2) { return model.Fault.Memory }
        }
        // Capacity was explicitly prepared. A failed item preparation leaves
        // every previously published element in the original prefix.
        storage.append(value, model.make(65 + index)?)
        index += 1
    }
    return ()
}

pub fn drain(value: Store<model.Item>) -> void!i32 {
    var sum: i32 = 0
    while store_len(value) > 0 {
        let item = storage.tail(value)?
        sum += item.code
    }
    return sum
}
