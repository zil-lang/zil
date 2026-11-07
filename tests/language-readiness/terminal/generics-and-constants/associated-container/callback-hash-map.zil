// Fixed-capacity open-addressed map for Copy, StoreElement keys and values.
// Caller-supplied hash and equality callbacks handle collisions, and negative
// i64 hash values are normalized before probing.
struct Entry<K:Copy,V:Copy> { key:K; value:V }
pub package PodHashMap<K:Copy+StoreElement,V:Copy+StoreElement> { manages entries:Store<Entry<K,V>>; manages buckets:Store<i64>; limit:i64 }
pub enum Error { Capacity, Allocation, Internal }
struct Slot { found:bool; bucket:i64; entry:i64 }
fn locate<K:Copy+StoreElement,V:Copy+StoreElement,E:Fn(K,K)->bool,H:Fn(K)->i64>(map:&PodHashMap<K,V>,key:K,equals:&E,hash:&H)->Slot {
    let count=store_len(map.buckets)
    var at=hash(key)%count
    if at<0 {at+=count}
    var first:i64=-1
    var seen:i64=0
    while seen<count {
        let marker=bucket_at::<K,V>(map,at)
        if marker==0 {
            if first>=0 {return Slot{found:false,bucket:first,entry:-1}}
            return Slot{found:false,bucket:at,entry:-1}
        }
        if marker<0 {
            if first<0 {first=at}
        } else {
            let item=*store_at(map.entries,marker-1)
            if equals(key,item.key) {return Slot{found:true,bucket:at,entry:marker-1}}
        }
        at+=1
        if at==count {at=0}
        seen+=1
    }
    if first>=0 {return Slot{found:false,bucket:first,entry:-1}}
    return Slot{found:false,bucket:-1,entry:-1}
}
fn bucket_at<K:Copy+StoreElement,V:Copy+StoreElement>(map:&PodHashMap<K,V>,index:i64)->i64 {return *store_at(map.buckets,index)}
fn set_entry<K:Copy+StoreElement,V:Copy+StoreElement>(map:&mut PodHashMap<K,V>,index:i64,value:Entry<K,V>) {let slot=store_at_mut(map.entries,index);*slot=value}
fn set_bucket<K:Copy+StoreElement,V:Copy+StoreElement>(map:&mut PodHashMap<K,V>,index:i64,marker:i64) {let slot=store_at_mut(map.buckets,index);*slot=marker}
fn value_at<K:Copy+StoreElement,V:Copy+StoreElement>(map:&PodHashMap<K,V>,index:i64)->V {let entry=*store_at(map.entries,index);return entry.value}
pub fn with_capacity<K:Copy+StoreElement,V:Copy+StoreElement>(limit:i64)->Error!PodHashMap<K,V> {
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
pub fn len<K:Copy+StoreElement,V:Copy+StoreElement>(map:&PodHashMap<K,V>)->i64 {return store_len(map.entries)}
pub fn get<K:Copy+StoreElement,V:Copy+StoreElement,E:Fn(K,K)->bool,H:Fn(K)->i64>(map:&PodHashMap<K,V>,key:K,equals:&E,hash:&H)->Option<V> {
    let found=locate(map,key,equals,hash)
    if !found.found {return Option::<V>.None()}
    let entry=*store_at(map.entries,found.entry)
    return Option::<V>.Some(entry.value)
}
pub fn insert<K:Copy+StoreElement,V:Copy+StoreElement,E:Fn(K,K)->bool,H:Fn(K)->i64>(map:&mut PodHashMap<K,V>,key:K,value:V,equals:&E,hash:&H)->Error!Option<V> {
    let found=locate(map,key,equals,hash)
    if found.found {
        let previous=value_at::<K,V>(map,found.entry)
        set_entry::<K,V>(map,found.entry,Entry::<K,V>{key:key,value:value})
        return Option::<V>.Some(previous)
    }
    if store_len(map.entries)>=map.limit || found.bucket<0 {return Error.Capacity}
    let at=store_len(map.entries)
    match store_push(map.entries,Entry::<K,V>{key:key,value:value}) {
        .err(_)=>{return Error.Internal},
        .ok(_)=>{}
    }
    set_bucket::<K,V>(map,found.bucket,at+1)
    return Option::<V>.None()
}
pub fn remove<K:Copy+StoreElement,V:Copy+StoreElement,E:Fn(K,K)->bool,H:Fn(K)->i64>(map:&mut PodHashMap<K,V>,key:K,equals:&E,hash:&H)->Error!Option<V> {
    let found=locate(map,key,equals,hash)
    if !found.found {return Option::<V>.None()}
    let length=store_len(map.entries)
    let old=value_at::<K,V>(map,found.entry)
    match store_pop(map.entries) {
        .err(_)=>{return Error.Internal},
        .ok(last)=>{
            set_bucket::<K,V>(map,found.bucket,-1)
            if found.entry<length-1 {
                set_entry::<K,V>(map,found.entry,last)
                var at:i64=0
                while at<store_len(map.buckets) {
                    if bucket_at::<K,V>(map,at)==length {
                        set_bucket::<K,V>(map,at,found.entry+1)
                        break
                    }
                    at+=1
                }
                if at==store_len(map.buckets) {return Error.Internal}
            }
            return Option::<V>.Some(old)
        }
    }
}
struct Key{n:i32}
struct Value{n:i32}
fn main()->i32 {
 let equals=(a:Key,b:Key)=>a.n==b.n
 let hash=(a:Key)=>i64(a.n)
 match with_capacity::<Key,Value>(4) {
 .err(_)=>{return 1},
 .ok(created)=>{
  var map:PodHashMap<Key,Value>=created
  match insert(&mut map,Key{n:7},Value{n:12},&equals,&hash) {.err(_)=>{return 2},.ok(_)=>{}}
  match get(&map,Key{n:7},&equals,&hash) {Option.None()=>{return 3},Option.Some(item)=>{if item.n!=12{return 4}}}
  match remove(&mut map,Key{n:7},&equals,&hash) {.err(_)=>{return 5},.ok(old)=>{let value:Option<Value>=old;match value {Option.None()=>{return 6},Option.Some(item)=>{if item.n!=12{return 7}}}}}
  return i32(len::<Key,Value>(&map))
 }
 }
}
