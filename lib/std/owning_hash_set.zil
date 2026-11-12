import std.owning_hash_map
import std.owning_map
import std.owning_entries
pub package OwningHashSet<K:StoreElement+owning_map.KeyEq+owning_hash_map.KeyHash> { state:owning_hash_map.OwningHashMap<K,bool> }
pub fn new<K:StoreElement+owning_map.KeyEq+owning_hash_map.KeyHash>()->OwningHashSet<K> {
    return OwningHashSet::<K>{state:owning_hash_map.new::<K,bool>()}
}
pub fn len<K:StoreElement+owning_map.KeyEq+owning_hash_map.KeyHash>(set:&OwningHashSet<K>)->i64 {return owning_hash_map.len::<K,bool>(&set.state)}
pub fn capacity<K:StoreElement+owning_map.KeyEq+owning_hash_map.KeyHash>(set:&OwningHashSet<K>)->i64 {return owning_hash_map.capacity::<K,bool>(&set.state)}
pub fn reserve<K:StoreElement+owning_map.KeyEq+owning_hash_map.KeyHash>(set:&mut OwningHashSet<K>,capacity:i64)->owning_hash_map.Error!void {return owning_hash_map.reserve::<K,bool>(&mut set.state,capacity)}
pub fn contains<K:StoreElement+owning_map.KeyEq+owning_hash_map.KeyHash>(set:&OwningHashSet<K>,key:&K)->bool {return owning_hash_map.contains::<K,bool>(&set.state,key)}
pub fn key_at<life s,K:StoreElement+owning_map.KeyEq+owning_hash_map.KeyHash>(set:&'s OwningHashSet<K>,index:i64)->Option<&'s K> {return owning_hash_map.key_at::<K,bool>(&set.state,index)}
// The failed key is returned intact. Duplicate insertion consumes the old key
// exactly once and retains the new representative, matching map replacement.
pub fn insert<K:StoreElement+owning_map.KeyEq+owning_hash_map.KeyHash>(set:&mut OwningHashSet<K>,key:K)->K!bool {
    let entry=owning_entries.Entry::<K,bool>{key:key,value:true}
    match owning_hash_map.insert::<K,bool>(&mut set.state,entry) {
        .err(original)=>{return original.key},
        .ok(replaced)=>{return !replaced}
    }
}
pub fn take<K:StoreElement+owning_map.KeyEq+owning_hash_map.KeyHash>(set:&mut OwningHashSet<K>,key:&K)->void!K {
    let entry=owning_hash_map.take::<K,bool>(&mut set.state,key)?
    return entry.key
}
pub fn clear<K:StoreElement+owning_map.KeyEq+owning_hash_map.KeyHash>(set:&mut OwningHashSet<K>)->i64 {return owning_hash_map.clear::<K,bool>(&mut set.state)}
