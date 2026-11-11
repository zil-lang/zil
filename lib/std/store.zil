// Explicit-capacity contiguous owning storage. Standard names grant no authority.
pub fn new<T:StoreElement>()->Store<T> contract {result(create)} { return store_new() }
pub fn reserve<T:StoreElement>(items:Store<T>,capacity:i64)->bool { return store_reserve(items,capacity) }
pub fn push<T:StoreElement>(items:Store<T>,value:T)->T!void { return store_push(items,value) }
// Grow only when full; failed preparation returns the actual incoming owner.
// Existing contents and capacity survive a failed reservation.
pub fn push_growing<T:StoreElement>(items:Store<T>,value:T)->T!void {
    let length=store_len(items)
    if length>=store_capacity(items) {
        if length>=9223372036854775807 {return value}
        var next:i64=1
        if length>4611686018427387903 {next=9223372036854775807}
        else {if length>0 {next=length*2}}
        if !store_reserve(items,next) {return value}
    }
    return store_push(items,value)
}
pub fn pop<T:StoreElement>(items:Store<T>)->void!T { return store_pop(items) }
pub fn len<T:StoreElement>(items:Store<T>)->i64 { return store_len(items) }
pub fn capacity<T:StoreElement>(items:Store<T>)->i64 { return store_capacity(items) }
pub fn clear<T:StoreElement>(items:Store<T>)->i64 {
    var count:i64=0
    while store_len(items)>0 { store_pop(items); count+=1 }
    return count
}
// Remove one owned element, preserving the order of all others. A range or
// allocation error returns Unit before modifying items. Preparation reserves
// the entire suffix; subsequent pushes fit their actual existing capacities.
pub fn remove<T:StoreElement>(items:Store<T>,index:i64)->void!T {
    let length=store_len(items)
    if index<0 || index>=length { return () }
    let suffix:Store<T>=store_new()
    if !store_reserve(suffix,length-index-1) { return () }
    while store_len(items)>index+1 {
        let value=store_pop(items)?
        store_push(suffix,value)
    }
    let removed=store_pop(items)?
    while store_len(suffix)>0 {
        let value=store_pop(suffix)?
        store_push(items,value)
    }
    return removed
}

// Requires one existing spare slot. The error owns the untouched incoming T.
pub fn insert<T:StoreElement>(items:Store<T>,index:i64,value:T)->T!void {
    let length=store_len(items)
    if index<0 || index>length || length>=store_capacity(items) { return value }
    let suffix:Store<T>=store_new()
    if !store_reserve(suffix,length-index) { return value }
    // All source counts and destination capacities are established above;
    // these transfers have no allocation or recoverable failure path.
    move_suffix(items,suffix,index)
    store_push(items,value)
    move_suffix(suffix,items,0)
    return ()
}
pub fn truncate<T:StoreElement>(items:Store<T>,length:i64)->bool {
    if length<0 || length>store_len(items) { return false }
    while store_len(items)>length { store_pop(items) }
    return true
}

fn move_suffix<T:StoreElement>(source:Store<T>,destination:Store<T>,length:i64)->void!i32 {
    while store_len(source)>length {
        let value=store_pop(source)?
        store_push(destination,value)
    }
    return 0
}
