pub fn seed<T:StoreElement>(value:T)->Store<T> contract{result(create)}{
    let items:Store<T>=store_new()
    if !store_reserve(items,3){return give items}
    store_push(items,value)
    return give items
}
pub fn push<T:StoreElement>(items:Store<T>,value:T)->T!void{return store_push(items,value)}
pub fn pop<T:StoreElement>(items:Store<T>)->void!T{return store_pop(items)}
pub fn release<T:StoreElement>(value:T){}
pub fn erase<T:StoreElement>(items:Store<T>,at:i64)->void!i32{
    let count=store_len(items)
    if at<0||at>=count{return 9}
    let suffix:Store<T>=store_new()
    if !store_reserve(suffix,count){return 1}
    while store_len(items)>at+1{let value=store_pop(items)?;store_push(suffix,value)}
    release(store_pop(items)?)
    while store_len(suffix)>0{let value=store_pop(suffix)?;store_push(items,value)}
    return 0
}
