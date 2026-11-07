package Item { manages data:ByteBuffer; value:i64 }
package Map { manages items:Store<Item> }
fn at(map:&Map,index:i64)->&i64 {
    let entry=store_at(map.items,index)
    return &entry.value
}
fn result()->i64 {
    let items:Store<Item>=store_new()
    if !store_reserve(items,2) {return 1}
    store_push(items,Item{data:bytes_new(),value:42})
    let map=Map{items:give items}
    let value=at(&map,0)
    let other=Map{items:store_new()}
    store_reserve(other.items,1)
    let observed=*value
    store_push(map.items,Item{data:bytes_new(),value:8})
    return observed
}
