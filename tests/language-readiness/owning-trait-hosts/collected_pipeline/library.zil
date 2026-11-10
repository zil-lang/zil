import protocol

pub fn next_tagged<life a,I:protocol.LendingIterator+protocol.Tag,T:Copy>(cursor:&'a mut I,tag:T)->Option<&'a I.Item> {
    let actual=cursor.tag()
    if actual<0 {return Option::<&'a I.Item>.None()}
    let copied=tag
    let copied_again=tag
    return cursor.next()
}

// A failed growth returns the exact incoming owner; existing elements stay put.
pub fn collect<T:StoreElement>(items:Store<T>,item:T)->T!void {
    let length=store_len(items)
    if length>=4611686018427387903 || !store_reserve(items,length+1) {return item}
    return store_push(items,item)
}
