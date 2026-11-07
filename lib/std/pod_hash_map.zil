// Fixed-capacity open-addressed map for Copy, StoreElement keys and values.
// The key trait is implemented for concrete user records; hash collisions use
// equals, and negative i64 hash values are normalized before probing.
pub trait HashKey { fn hash(self:&Self)->i64; fn equals(self:&Self,other:Self)->bool; }
struct Entry<K:Copy,V:Copy> { key:K; value:V }
pub package PodHashMap<K:Copy+StoreElement+HashKey,V:Copy+StoreElement> { manages entries:Store<Entry<K,V>>; manages buckets:Store<i64>; limit:i64 }
pub enum Error { Capacity, Allocation, Internal }
struct Slot { found:bool; bucket:i64; entry:i64 }
fn locate<K:Copy+StoreElement+HashKey,V:Copy+StoreElement>(map:&PodHashMap<K,V>,key:K)->Slot {
    let count=store_len(map.buckets)
    var at=key.hash()%count
    if at<0 {at+=count}
    var first:i64=-1
    var seen:i64=0
    while seen<count {
        let marker=*store_at(map.buckets,at)
        if marker==0 {
            if first>=0 {return Slot{found:false,bucket:first,entry:-1}}
            return Slot{found:false,bucket:at,entry:-1}
        }
        if marker<0 {
            if first<0 {first=at}
        } else {
            let item=*store_at(map.entries,marker-1)
            if key.equals(item.key) {return Slot{found:true,bucket:at,entry:marker-1}}
        }
        at+=1
        if at==count {at=0}
        seen+=1
    }
    if first>=0 {return Slot{found:false,bucket:first,entry:-1}}
    return Slot{found:false,bucket:-1,entry:-1}
}
pub fn with_capacity<K:Copy+StoreElement+HashKey,V:Copy+StoreElement>(limit:i64)->Error!PodHashMap<K,V> {
    if limit<0 || limit>4611686018427387903 {return Error.Capacity}
    let entries:Store<Entry<K,V>>=store_new()
    let buckets:Store<i64>=store_new()
    let slots=limit*2+1
    if !store_reserve(entries,limit) {return Error.Allocation}
    if !store_reserve(buckets,slots) {return Error.Allocation}
    var at:i64=0
    while at<slots {
        match store_push(buckets,0) {
            .err(_)=>{return Error.Internal},
            .ok(_)=>{}
        }
        at+=1
    }
    return PodHashMap::<K,V>{entries:give entries,buckets:give buckets,limit:limit}
}
pub fn len<K:Copy+StoreElement+HashKey,V:Copy+StoreElement>(map:&PodHashMap<K,V>)->i64 {return store_len(map.entries)}
pub fn get<K:Copy+StoreElement+HashKey,V:Copy+StoreElement>(map:&PodHashMap<K,V>,key:K)->Option<V> {
    let found=locate::<K,V>(map,key)
    if !found.found {return Option::<V>.None()}
    let entry=*store_at(map.entries,found.entry)
    return Option::<V>.Some(entry.value)
}
pub fn insert<K:Copy+StoreElement+HashKey,V:Copy+StoreElement>(map:&mut PodHashMap<K,V>,key:K,value:V)->Error!Option<V> {
    let found=locate::<K,V>(map,key)
    if found.found {
        let prior_entry=*store_at(map.entries,found.entry)
        let previous=prior_entry.value
        let slot=store_at_mut(map.entries,found.entry)
        *slot=Entry::<K,V>{key:key,value:value}
        return Option::<V>.Some(previous)
    }
    if store_len(map.entries)>=map.limit || found.bucket<0 {return Error.Capacity}
    let at=store_len(map.entries)
    match store_push(map.entries,Entry::<K,V>{key:key,value:value}) {
        .err(_)=>{return Error.Internal},
        .ok(_)=>{}
    }
    let slot=store_at_mut(map.buckets,found.bucket)
    *slot=at+1
    return Option::<V>.None()
}
pub fn remove<K:Copy+StoreElement+HashKey,V:Copy+StoreElement>(map:&mut PodHashMap<K,V>,key:K)->Error!Option<V> {
    let found=locate::<K,V>(map,key)
    if !found.found {return Option::<V>.None()}
    let length=store_len(map.entries)
    let old=*store_at(map.entries,found.entry)
    match store_pop(map.entries) {
        .err(_)=>{return Error.Internal},
        .ok(last)=>{
            let slot=store_at_mut(map.buckets,found.bucket)
            *slot=-1
            if found.entry<length-1 {
                let slot=store_at_mut(map.entries,found.entry)
                *slot=last
                var at:i64=0
                while at<store_len(map.buckets) {
                    if *store_at(map.buckets,at)==length {
                        let slot=store_at_mut(map.buckets,at)
                        *slot=found.entry+1
                        break
                    }
                    at+=1
                }
                if at==store_len(map.buckets) {return Error.Internal}
            }
            return Option::<V>.Some(old.value)
        }
    }
}
