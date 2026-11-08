package Item { manages data:ByteBuffer; value:i64 }
package Map { manages items:Store<Item> }
fn get<life s>(map:&'s Map,index:i64,key:&Item)->Option<&'s Item> {
    if key.value<0 {return Option::<&'s Item>.None()}
    if index<0 || index>=store_len(map.items) {
        return Option::<&'s Item>.None()
    }
    let entry=store_at(map.items,index)
    return Option::<&'s Item>.Some(entry)
}
fn inspect<life s>(found:Option<&'s Item>)->i64 {
    match found {Option.Some(value)=>{return value.value},Option.None()=>{return -1}}
}
fn result()->i64 {
    let items:Store<Item>=store_new()
    if !store_reserve(items,2) {return 1}
    store_push(items,Item{data:bytes_new(),value:42})
    let map=Map{items:give items}
    let query=Item{data:bytes_new(),value:7}
    let found=get(&map,0,&query)
    let observed=inspect(found)
    store_push(map.items,Item{data:bytes_new(),value:8})
    return observed
}

fn main()->i32{return i32(result()-42)}
