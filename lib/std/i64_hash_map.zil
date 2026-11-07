// Fixed-capacity i64-key open-addressed map with collision resolution.
struct Entry { key:i64; value:i64 }
pub package I64HashMap { manages entries:Store<Entry>; manages buckets:Store<i64>; limit:i64 }
pub enum Error { Capacity, Allocation, Internal }
struct Slot { found:bool; bucket:i64; entry:i64 }
fn locate(map:&I64HashMap,key:i64)->Slot {
    let count=store_len(map.buckets)
    var at=key%count
    if at<0 {at+=count}
    var first:i64=-1
    var seen:i64=0
    while seen<count {
        let marker=bucket_at(map,at)
        if marker==0 {
            if first>=0 {return Slot{found:false,bucket:first,entry:-1}}
            return Slot{found:false,bucket:at,entry:-1}
        }
        if marker<0 {
            if first<0 {first=at}
        } else {
            let item=*store_at(map.entries,marker-1)
            if key==item.key {return Slot{found:true,bucket:at,entry:marker-1}}
        }
        at+=1
        if at==count {at=0}
        seen+=1
    }
    if first>=0 {return Slot{found:false,bucket:first,entry:-1}}
    return Slot{found:false,bucket:-1,entry:-1}
}
fn bucket_at(map:&I64HashMap,index:i64)->i64 {return *store_at(map.buckets,index)}
fn set_entry(map:&mut I64HashMap,index:i64,value:Entry) {let slot=store_at_mut(map.entries,index);*slot=value}
fn set_bucket(map:&mut I64HashMap,index:i64,marker:i64) {let slot=store_at_mut(map.buckets,index);*slot=marker}
fn value_at(map:&I64HashMap,index:i64)->i64 {let entry=*store_at(map.entries,index);return entry.value}
pub fn with_capacity(limit:i64)->Error!I64HashMap {
    if limit<0 || limit>4611686018427387903 {return Error.Capacity}
    let entries:Store<Entry>=store_new()
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
    return I64HashMap{entries:give entries,buckets:give buckets,limit:limit}
}
pub fn len(map:&I64HashMap)->i64 {return store_len(map.entries)}
pub fn get(map:&I64HashMap,key:i64)->Option<i64> {
    let found=locate(map,key)
    if !found.found {return Option::<i64>.None()}
    let entry=*store_at(map.entries,found.entry)
    return Option::<i64>.Some(entry.value)
}
pub fn insert(map:&mut I64HashMap,key:i64,value:i64)->Error!Option<i64> {
    let found=locate(map,key)
    if found.found {
        let previous=value_at(map,found.entry)
        set_entry(map,found.entry,Entry{key:key,value:value})
        return Option::<i64>.Some(previous)
    }
    if store_len(map.entries)>=map.limit || found.bucket<0 {return Error.Capacity}
    let at=store_len(map.entries)
    match store_push(map.entries,Entry{key:key,value:value}) {
        .err(_)=>{return Error.Internal},
        .ok(_)=>{}
    }
    set_bucket(map,found.bucket,at+1)
    return Option::<i64>.None()
}
pub fn remove(map:&mut I64HashMap,key:i64)->Error!Option<i64> {
    let found=locate(map,key)
    if !found.found {return Option::<i64>.None()}
    let length=store_len(map.entries)
    let old=value_at(map,found.entry)
    match store_pop(map.entries) {
        .err(_)=>{return Error.Internal},
        .ok(last)=>{
            set_bucket(map,found.bucket,-1)
            if found.entry<length-1 {
                set_entry(map,found.entry,last)
                var at:i64=0
                while at<store_len(map.buckets) {
                    if bucket_at(map,at)==length {
                        set_bucket(map,at,found.entry+1)
                        break
                    }
                    at+=1
                }
                if at==store_len(map.buckets) {return Error.Internal}
            }
            return Option::<i64>.Some(old)
        }
    }
}
