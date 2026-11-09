import std.pod_hash_map

// A nominal set for actual Copy, StoreElement keys. It shares the map's probing
// and transactional rehash engine while keeping value writes out of its API.
pub package PodHashSet<K:Copy+StoreElement> { state:pod_hash_map.PodHashMap<K,bool> }

pub fn with_capacity<K:Copy+StoreElement>(capacity:i64)->pod_hash_map.Error!PodHashSet<K> {
    let state=pod_hash_map.with_capacity::<K,bool>(capacity)?
    return PodHashSet::<K>{state:state}
}
pub fn len<K:Copy+StoreElement>(set:&PodHashSet<K>)->i64 {return pod_hash_map.len::<K,bool>(&set.state)}
pub fn capacity<K:Copy+StoreElement>(set:&PodHashSet<K>)->i64 {return pod_hash_map.capacity::<K,bool>(&set.state)}
pub fn key_at<K:Copy+StoreElement>(set:&PodHashSet<K>,index:i64)->Option<K> {
    match pod_hash_map.at::<K,bool>(&set.state,index) {
        Option.None()=>{return Option::<K>.None()},
        Option.Some(pair)=>{return Option::<K>.Some(pair.key)}
    }
}
pub fn contains<K:Copy+StoreElement,E:Fn(K,K)->bool,H:Fn(K)->i64>(set:&PodHashSet<K>,key:K,equals:&E,hash:&H)->bool {
    match pod_hash_map.get(&set.state,key,equals,hash) {
        Option.None()=>{return false},
        Option.Some(_)=>{return true}
    }
}
pub fn insert<K:Copy+StoreElement,E:Fn(K,K)->bool,H:Fn(K)->i64>(set:&mut PodHashSet<K>,key:K,equals:&E,hash:&H)->pod_hash_map.Error!bool {
    let old=pod_hash_map.insert(&mut set.state,key,true,equals,hash)?
    match old {
        Option.None()=>{return true},
        Option.Some(_)=>{return false}
    }
}
pub fn insert_growing<K:Copy+StoreElement,E:Fn(K,K)->bool,H:Fn(K)->i64>(set:&mut PodHashSet<K>,key:K,equals:&E,hash:&H)->pod_hash_map.Error!bool {
    let old=pod_hash_map.insert_growing(&mut set.state,key,true,equals,hash)?
    match old {
        Option.None()=>{return true},
        Option.Some(_)=>{return false}
    }
}
pub fn reserve<K:Copy+StoreElement,E:Fn(K,K)->bool,H:Fn(K)->i64>(set:&mut PodHashSet<K>,capacity:i64,equals:&E,hash:&H)->pod_hash_map.Error!void {
    return pod_hash_map.reserve(&mut set.state,capacity,equals,hash)
}
pub fn remove<K:Copy+StoreElement,E:Fn(K,K)->bool,H:Fn(K)->i64>(set:&mut PodHashSet<K>,key:K,equals:&E,hash:&H)->pod_hash_map.Error!bool {
    let old=pod_hash_map.remove(&mut set.state,key,equals,hash)?
    match old {
        Option.None()=>{return false},
        Option.Some(_)=>{return true}
    }
}
pub fn clear<K:Copy+StoreElement>(set:&mut PodHashSet<K>)->i64 {return pod_hash_map.clear::<K,bool>(&mut set.state)}
