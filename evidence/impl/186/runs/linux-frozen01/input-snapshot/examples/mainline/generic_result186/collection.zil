import model;

pub fn build<T:StoreElement>(a:T,b:T,fail:bool)->model.Error!Store<T>
contract { result(create) } {
    let store:Store<T>=store_new();
    if !store_reserve(store,1) { return model.Error.Allocate };
    store_push(store,a);
    if !store_reserve(store,4) { return model.Error.Allocate };
    store_push(store,b);
    if fail { return model.Error.Logical };
    return give store
}

pub fn checked<T:StoreElement>(a:T,b:T,fail:bool)->model.Error!Store<T>
contract { result(create) } {
    let pending=build(a,b,fail);
    let moved=pending;
    let store=moved?;
    return give store
}
