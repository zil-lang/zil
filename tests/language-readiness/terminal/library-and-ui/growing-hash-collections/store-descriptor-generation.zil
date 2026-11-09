package Holder { manages items:Store<i64> }
fn main()->i32 {
    let items:Store<i64>=store_new()
    if !store_reserve(items,1) {return 1}
    store_push(items,11)
    var holder=Holder{items:give items}
    var pointer=&mut holder
    let old=pointer.items
    let child=store_at(old,0)
    let value=*child
    pointer=&mut holder
    if store_len(pointer.items)!=1 {return 2}
    return i32(value)-11
}
