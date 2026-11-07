struct Key{n:i32}
struct Value{n:i32}
package Map<K:Copy+StoreElement,V:Copy+StoreElement>{manages keys:Store<K>;manages values:Store<V>}
fn first<K:Copy+StoreElement,V:Copy+StoreElement>(map:&Map<K,V>)->K{return *store_at(map.keys,0)}
fn update<K:Copy+StoreElement,V:Copy+StoreElement>(map:&mut Map<K,V>,value:K){
    let slot=store_at_mut(map.keys,0)
    *slot=value
}
fn main()->i32 {
    let keys:Store<Key>=store_new()
    let values:Store<Value>=store_new()
    store_reserve(keys,1)
    store_reserve(values,1)
    store_push(keys,Key{n:7})
    store_push(values,Value{n:8})
    var map=Map::<Key,Value>{keys:give keys,values:give values}
    let before=first::<Key,Value>(&map).n
    update::<Key,Value>(&mut map,Key{n:11})
    return before+first::<Key,Value>(&map).n-18
}
