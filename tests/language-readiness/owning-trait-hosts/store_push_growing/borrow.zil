import std.store
fn main()->i32 {
    let items:Store<i64>=store.new()
    if !store.reserve(items,1) {return 1}
    match store.push(items,10) {.err(_)=>{return 2},.ok(_)=>{}}
    let held=store_at(items,0)
    let snapshot=*held
    match store.push_growing(items,20) {.err(_)=>{return 3},.ok(_)=>{}}
    if snapshot!=10 || store.len(items)!=2 || store.capacity(items)!=2 {return 4}
    if *store_at(items,0)!=10 || *store_at(items,1)!=20 {return 5}
    return 0
}
