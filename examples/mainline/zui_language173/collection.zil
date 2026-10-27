import model

pub fn empty() -> Store<model.Item> contract { result(create) } { return store_new() }
pub fn relay(give storage: Store<model.Item>) -> Store<model.Item> contract { result(give) } { return give storage }
pub fn insert(storage: Store<model.Item>, item: model.Item) -> model.Item!void { return store_push(storage, item) }
pub fn take_last(storage: Store<model.Item>) -> void!model.Item { return store_pop(storage) }
