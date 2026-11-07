// Finite i64-key collection family. Growth is explicit in the error result;
// successful replacement never allocates. No key ordering is promised.
pub enum Error { Capacity, Allocation, Internal }
pub struct Put { pub replaced:bool; pub previous:i64 }
pub struct Removal { pub found:bool; pub value:i64 }
struct Entry { key:i64; value:i64 }
pub package I64Map { manages entries:Store<Entry> }
pub package I64Set { manages entries:Store<i64> }

// These helpers finish their element loan before a caller mutates the Store.
fn map_index(items:Store<Entry>,key:i64)->i64 {
    var at:i64=0
    while at<store_len(items) {
        let entry=*store_at(items,at)
        if entry.key==key {return at}
        at+=1
    }
    return -1
}
fn map_value(items:Store<Entry>,index:i64)->i64 {
    let entry=*store_at(items,index)
    return entry.value
}
fn map_set(items:Store<Entry>,index:i64,item:i64) {
    let target=store_at_mut(items,index)
    target.value=item
}
fn map_replace(items:Store<Entry>,index:i64,item:Entry) {
    let target=store_at_mut(items,index)
    *target=item
}
fn set_index(items:Store<i64>,key:i64)->i64 {
    var at:i64=0
    while at<store_len(items) {
        if *store_at(items,at)==key {return at}
        at+=1
    }
    return -1
}
fn set_replace(items:Store<i64>,index:i64,item:i64) {
    let target=store_at_mut(items,index)
    *target=item
}

pub fn map_new()->I64Map {return I64Map{entries:store_new()}}
pub fn map_len(value:&I64Map)->i64 {return store_len(value.entries)}
pub fn map_capacity(value:&I64Map)->i64 {return store_capacity(value.entries)}
pub fn map_get(value:&I64Map,key:i64)->Option<i64> {
    let index=map_index(value.entries,key)
    if index<0 {return Option.None}
    return Option.Some(map_value(value.entries,index))
}
pub fn map_insert(value:&mut I64Map,key:i64,item:i64)->Error!Put {
    let index=map_index(value.entries,key)
    if index>=0 {
        let previous=map_value(value.entries,index)
        map_set(value.entries,index,item)
        return Put{replaced:true,previous:previous}
    }
    let length=store_len(value.entries)
    if length==store_capacity(value.entries) {
        let old=store_capacity(value.entries)
        if old>4611686018427387903 {return Error.Capacity}
        var next:i64=8
        if old>=8 {next=old*2}
        if !store_reserve(value.entries,next) {return Error.Allocation}
    }
    match store_push(value.entries,Entry{key:key,value:item}) {
        .ok(())=>{return Put{replaced:false,previous:0}},
        .err(_)=>{return Error.Internal}
    }
}
pub fn map_remove(value:&mut I64Map,key:i64)->Error!Removal {
    let at=map_index(value.entries,key)
    let length=store_len(value.entries)
    if at<0 {return Removal{found:false,value:0}}
    let previous=map_value(value.entries,at)
    match store_pop(value.entries) {
        .err(_)=>{return Error.Internal},
        .ok(last)=>{
            if at<length-1 {
                map_replace(value.entries,at,last)
            }
            return Removal{found:true,value:previous}
        }
    }
}

pub fn set_new()->I64Set {return I64Set{entries:store_new()}}
pub fn set_len(value:&I64Set)->i64 {return store_len(value.entries)}
pub fn set_capacity(value:&I64Set)->i64 {return store_capacity(value.entries)}
pub fn set_contains(value:&I64Set,key:i64)->bool {
    return set_index(value.entries,key)>=0
}
pub fn set_insert(value:&mut I64Set,key:i64)->Error!bool {
    if set_contains(value,key) {return false}
    let length=store_len(value.entries)
    if length==store_capacity(value.entries) {
        let old=store_capacity(value.entries)
        if old>4611686018427387903 {return Error.Capacity}
        var next:i64=8
        if old>=8 {next=old*2}
        if !store_reserve(value.entries,next) {return Error.Allocation}
    }
    match store_push(value.entries,key) {
        .ok(())=>{return true},
        .err(_)=>{return Error.Internal}
    }
}
pub fn set_remove(value:&mut I64Set,key:i64)->Error!bool {
    let at=set_index(value.entries,key)
    let length=store_len(value.entries)
    if at<0 {return false}
    match store_pop(value.entries) {
        .err(_)=>{return Error.Internal},
        .ok(last)=>{
            if at<length-1 {
                set_replace(value.entries,at,last)
            }
            return true
        }
    }
}
