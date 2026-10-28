import model;

pub fn marker(mode:i32)->model.Error!model.Mark contract { result(create) } {
    if mode==5 { return model.Error.Failed };
    return model.marker(77)
}

pub fn buffer(mode:i32)->model.Error!ByteBuffer contract { result(create) } {
    let marker=model.marker(66);
    let buffer=bytes_new();
    if !bytes_reserve(buffer,1) { return model.Error.Failed };
    bytes_push(buffer,u8(65));
    if !bytes_reserve(buffer,4) { return model.Error.Failed };
    if mode==1 { return model.Error.Failed };
    return give buffer
}

pub fn store(mode:i32)->model.Error!Store<model.Item> contract { result(create) } {
    let marker=model.marker(83);
    let store:Store<model.Item> = store_new();
    if !store_reserve(store,1) { return model.Error.Failed };
    store_push(store,model.item(67));
    if !store_reserve(store,3) { return model.Error.Failed };
    if mode==2 { return model.Error.Failed };
    return give store
}
