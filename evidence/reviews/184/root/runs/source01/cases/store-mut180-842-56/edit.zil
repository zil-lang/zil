import model

pub fn translate(view: &mut model.Item, delta: i32) -> i32 {
    view.metadata.code += delta
    view.metadata.coordinates[0] += delta
    view.metadata.coordinates[1] += delta
    return view.metadata.code
}

pub fn apply(items: Store<model.Item>, delta: i32) -> i32 {
    let count = store_len(items)
    var index: i64 = 0
    var total: i32 = 0
    while index < count {
        let view = store_at_mut(items, index)
        total += translate(view, delta)
        if view.metadata.coordinates[0] != view.metadata.code ||
           view.metadata.coordinates[1] != view.metadata.code + 1 { return -1 }
        index += 1
    }
    return total
}

pub fn code(view: &model.Item) -> i32 { return view.metadata.code }
