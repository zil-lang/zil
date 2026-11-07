import model

pub fn empty()->Store<model.Root> contract { result(create) } {
    return store_new()
}

pub fn pop(items:Store<model.Root>)->void!model.Root {
    return store_pop(items)
}
