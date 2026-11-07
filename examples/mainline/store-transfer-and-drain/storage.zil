import model

pub fn empty() -> Store<model.Item> contract { result(create) } { return store_new() }
pub fn relay(give value: Store<model.Item>) -> Store<model.Item> contract { result(give) } { return give value }
pub fn append(value: Store<model.Item>, item: model.Item) -> model.Item!void { return store_push(value, item) }
pub fn tail(value: Store<model.Item>) -> void!model.Item { return store_pop(value) }
