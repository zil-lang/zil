import model

pub fn empty() -> Store<model.Item> contract { result(create) } { return model.empty() }
pub fn reserve(items: Store<model.Item>, capacity: i64) -> bool => store_reserve(items, capacity)
pub fn append(items: Store<model.Item>, code: i32) { store_push(items, model.item(code)) }
