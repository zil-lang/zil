import model

pub fn code(item: &model.Item) -> i32 {
    let metadata = item.metadata
    if metadata.coordinates[0] != metadata.code || metadata.coordinates[1] != metadata.code + 1 {
        return -1000
    }
    return metadata.code
}

pub fn sum(storage: Store<model.Item>) -> i32 {
    var index: i64 = 0
    var total: i32 = 0
    while index < store_len(storage) {
        let item = store_at(storage, index)
        total += code(item)
        index += 1
    }
    return total
}
