struct Key{n:i32}
trait Hash{fn hash(self:&Self)->i64;}
impl Hash for Key{fn hash()->i64{return i64(.n)}}
struct Value{flag:bool}
struct Entry<K:Copy,V:Copy>{key:K;value:V}
package Map<K:Copy+StoreElement,V:Copy+StoreElement>{manages entries:Store<Entry<K,V>>;manages buckets:Store<i64>}
fn collect<K:Copy+StoreElement,V:Copy+StoreElement>(key:K,value:V)->i64{
    let items:Store<Entry<K,V>>=store_new()
    store_reserve(items,1)
    match store_push(items,Entry::<K,V>{key:key,value:value}){
        .ok(_)=>{},.err(_)=>{return 99}
    }
    return store_len(items)
}
fn equal_with<K:Copy+StoreElement+Hash,V:Copy+StoreElement,F:Fn(K,K)->bool>(left:K,right:K,value:V,equals:&F)->bool{
    value;
    left.hash();
    return equals(left,right)
}
fn main()->i32{
    let entries:Store<Entry<Key,Value>>=store_new()
    let buckets:Store<i64>=store_new()
    let map=Map::<Key,Value>{entries:give entries,buckets:give buckets}
    let count=collect::<Key,Value>(Key{n:42},Value{flag:true})
    let eq=(a:Key,b:Key)=>a.n==b.n
    if !equal_with(Key{n:7},Key{n:7},Value{flag:true},&eq){return 3}
    return i32(count+store_len(map.entries))-1
}
