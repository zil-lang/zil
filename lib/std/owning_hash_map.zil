// Owning open-addressed map. Key equality/hash must remain stable while stored;
// equal keys must have equal hashes. Buckets own indices, never borrowed keys.
import std.owning_entries
import std.owning_map
import std.store

pub trait KeyHash { fn hash(self:&Self)->i64; }
pub enum Error { Capacity, Allocation, Internal }
struct Slot { bucket:i64; entry:i64 }
pub package OwningHashMap<K:StoreElement+owning_map.KeyEq+KeyHash,V:StoreElement> {
    manages entries:Store<owning_entries.Entry<K,V>>
    manages buckets:Store<i64>
    limit:i64
}

pub fn new<K:StoreElement+owning_map.KeyEq+KeyHash,V:StoreElement>()->OwningHashMap<K,V> {
    return OwningHashMap::<K,V>{entries:store_new(),buckets:store_new(),limit:0}
}
pub fn len<K:StoreElement+owning_map.KeyEq+KeyHash,V:StoreElement>(map:&OwningHashMap<K,V>)->i64 {return store_len(map.entries)}
pub fn capacity<K:StoreElement+owning_map.KeyEq+KeyHash,V:StoreElement>(map:&OwningHashMap<K,V>)->i64 {return map.limit}
fn locate<K:StoreElement+owning_map.KeyEq+KeyHash,V:StoreElement>(map:&OwningHashMap<K,V>,key:&K)->Slot {
    let count=store_len(map.buckets)
    if count==0 {return Slot{bucket:-1,entry:-1}}
    var at=key.hash()%count
    if at<0 {at+=count}
    var first:i64=-1
    var seen:i64=0
    while seen<count {
        let marker=*store_at(map.buckets,at)
        if marker==0 {
            if first>=0 {return Slot{bucket:first,entry:-1}}
            return Slot{bucket:at,entry:-1}
        }
        if marker<0 {if first<0 {first=at}}
        else {
            let entry=store_at(map.entries,marker-1)
            let stored=&entry.key
            if stored.same(key) {return Slot{bucket:at,entry:marker-1}}
        }
        at+=1
        if at==count {at=0}
        seen+=1
    }
    return Slot{bucket:first,entry:-1}
}
pub fn contains<K:StoreElement+owning_map.KeyEq+KeyHash,V:StoreElement>(map:&OwningHashMap<K,V>,key:&K)->bool {let found=locate::<K,V>(map,key);return found.entry>=0}
pub fn get<life s,K:StoreElement+owning_map.KeyEq+KeyHash,V:StoreElement>(map:&'s OwningHashMap<K,V>,key:&K)->Option<&'s V> {
    let found=locate::<K,V>(map,key)
    if found.entry<0 {return Option::<&'s V>.None()}
    let entry=store_at(map.entries,found.entry)
    return Option::<&'s V>.Some(&entry.value)
}
pub fn key_at<life s,K:StoreElement+owning_map.KeyEq+KeyHash,V:StoreElement>(map:&'s OwningHashMap<K,V>,index:i64)->Option<&'s K> {
    if index<0 || index>=store_len(map.entries) {return Option::<&'s K>.None()}
    let entry=store_at(map.entries,index)
    return Option::<&'s K>.Some(&entry.key)
}
// Prepare every fallible bucket operation first. The entries reserve is the
// last allocation; failure leaves all old entries, indices and capacity intact.
pub fn reserve<K:StoreElement+owning_map.KeyEq+KeyHash,V:StoreElement>(map:&mut OwningHashMap<K,V>,capacity:i64)->Error!void {
    if capacity<0 || capacity>4611686018427387903 {return Error.Capacity}
    if capacity<=map.limit {return ()}
    let buckets:Store<i64>=store_new()
    let count=capacity*2+1
    if !store_reserve(buckets,count) {return Error.Allocation}
    var at:i64=0
    while at<count {
        match store_push(buckets,0) {.err(_)=>{return Error.Internal},.ok(_)=>{}}
        at+=1
    }
    at=0
    while at<store_len(map.entries) {
        let entry=store_at(map.entries,at)
        let key=&entry.key
        var bucket=key.hash()%count
        if bucket<0 {bucket+=count}
        while *store_at(buckets,bucket)!=0 {bucket+=1;if bucket==count {bucket=0}}
        let place=store_at_mut(buckets,bucket)
        *place=at+1
        at+=1
    }
    if !store_reserve(map.entries,capacity) {return Error.Allocation}
    map.buckets=give buckets
    map.limit=capacity
    return ()
}
// Removal preserves dense order. Preparing the suffix can fail before any
// move. Reindexing is O(capacity); successful lookup remains hash based.
fn reindex_removed<K:StoreElement+owning_map.KeyEq+KeyHash,V:StoreElement>(map:&mut OwningHashMap<K,V>,removed:i64) {
    var at:i64=0
    while at<store_len(map.buckets) {
        let place=store_at_mut(map.buckets,at)
        if *place==removed+1 {*place=-1}
        else {if *place>removed+1 {*place-=1}}
        at+=1
    }
}
pub fn take<K:StoreElement+owning_map.KeyEq+KeyHash,V:StoreElement>(map:&mut OwningHashMap<K,V>,key:&K)->void!owning_entries.Entry<K,V> {
    let found=locate::<K,V>(map,key)
    if found.entry<0 {return ()}
    let old=store.remove::<owning_entries.Entry<K,V>>(map.entries,found.entry)?
    reindex_removed::<K,V>(map,found.entry)
    return old
}
// Error returns the complete incoming key and value. New keys grow
// geometrically; replacing an existing key does not grow or rehash.
pub fn insert<K:StoreElement+owning_map.KeyEq+KeyHash,V:StoreElement>(map:&mut OwningHashMap<K,V>,entry:owning_entries.Entry<K,V>)->owning_entries.Entry<K,V>!bool {
    let found=locate::<K,V>(map,&entry.key)
    if found.entry>=0 {
        match store.remove::<owning_entries.Entry<K,V>>(map.entries,found.entry) {
            .err(_)=>{return entry},
            .ok(old)=>{
                reindex_removed::<K,V>(map,found.entry)
                let index=store_len(map.entries)
                match store_push(map.entries,entry) {.err(original)=>{return original},.ok(_)=>{}}
                let place=store_at_mut(map.buckets,found.bucket)
                *place=index+1
                return true
            }
        }
    }
    if store_len(map.entries)>=map.limit {
        if map.limit>=4611686018427387903 {return entry}
        var next:i64=1
        if map.limit>2305843009213693951 {next=4611686018427387903}
        else {if map.limit>0 {next=map.limit*2}}
        match reserve::<K,V>(map,next) {.err(_)=>{return entry},.ok(_)=>{}}
    }
    let prepared=locate::<K,V>(map,&entry.key)
    if prepared.bucket<0 {return entry}
    let index=store_len(map.entries)
    match store_push(map.entries,entry) {.err(original)=>{return original},.ok(_)=>{}}
    let place=store_at_mut(map.buckets,prepared.bucket)
    *place=index+1
    return false
}
pub fn clear<K:StoreElement+owning_map.KeyEq+KeyHash,V:StoreElement>(map:&mut OwningHashMap<K,V>)->i64 {
    let count=store_len(map.entries)
    while store_len(map.entries)>0 {store_pop(map.entries)}
    var at:i64=0
    while at<store_len(map.buckets) {let place=store_at_mut(map.buckets,at);*place=0;at+=1}
    return count
}
