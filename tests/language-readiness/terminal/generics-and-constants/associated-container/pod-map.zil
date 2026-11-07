struct Key{n:i32}
struct Value{flag:bool}
struct Entry<K:Copy,V:Copy>{key:K;value:V}
package Map<K:Copy+StoreElement,V:Copy+StoreElement>{manages entries:Store<Entry<K,V>>}
fn collect<K:Copy+StoreElement,V:Copy+StoreElement>(key:K,value:V)->i64{
    let items:Store<Entry<K,V>>=store_new()
    store_reserve(items,1)
    match store_push(items,Entry::<K,V>{key:key,value:value}){
        .ok(_)=>{},.err(_)=>{return 99}
    }
    return store_len(items)
}
fn main()->i32{
    let entries:Store<Entry<Key,Value>>=store_new()
    let map=Map::<Key,Value>{entries:give entries}
    let count=collect::<Key,Value>(Key{n:42},Value{flag:true})
    return i32(count+store_len(map.entries))-1
}
