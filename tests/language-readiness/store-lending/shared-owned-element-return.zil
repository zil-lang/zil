package Item { manages data:ByteBuffer; value:i64 }
package Map { manages items:Store<Item> }
fn at<life s>(map:&'s Map,index:i64)->&'s Item {
    return store_at(map.items,index)
}
fn result()->i64 {
    let items:Store<Item>=store_new()
    if !store_reserve(items,2) {return 1}
    store_push(items,Item{data:bytes_new(),value:42})
    let map=Map{items:give items}
    let value=at(&map,0)
    let observed=value.value
    store_push(map.items,Item{data:bytes_new(),value:8})
    return observed
}
