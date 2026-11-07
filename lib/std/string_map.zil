// Owned UTF-8 keys with Copy i64 values. A failed insert returns its key.
// Entries stay in insertion order; deletion may allocate to preserve it.
import std.string
import std.store
pub enum Failure { Capacity, Allocation, Internal }
pub package InsertError { pub key:string.String; pub reason:Failure }
pub struct Put { pub replaced:bool; pub previous:i64 }
pub package StringMap { manages keys:Store<string.String>; manages values:Store<i64> }

fn index(keys:Store<string.String>,key:&string.String)->i64 {
    let target=string.as_bytes(key)
    var at:i64=0
    while at<store_len(keys) {
        let stored=store_at(keys,at)
        let a=string.as_bytes(stored)
        if a.len()==target.len() {
            var part:i64=0
            while part<a.len() && a[part]==target[part] {part+=1}
            if part==a.len() {return at}
        }
        at+=1
    }
    return -1
}
fn at_value(items:Store<i64>,at:i64)->i64 {return *store_at(items,at)}
fn set_value(items:Store<i64>,at:i64,value:i64) {let slot=store_at_mut(items,at);*slot=value}
pub fn new()->StringMap {return StringMap{keys:store_new(),values:store_new()}}
pub fn len(map:&StringMap)->i64 {return store_len(map.keys)}
pub fn get(map:&StringMap,key:&string.String)->Option<i64> {
    let at=index(map.keys,key)
    if at<0 {return Option.None}
    return Option.Some(at_value(map.values,at))
}
// Indexed traversal preserves insertion order. Cloning makes the returned key
// independent of later map mutation; a short limit or allocation failure is
// reported by std.string, and an invalid index reports Range.
pub fn key_clone_at(map:&StringMap,index:i64,byte_limit:i64)->string.Error!string.String {
    if index<0 || index>=store_len(map.keys) {return string.Error.Range}
    let key=store_at(map.keys,index)
    return string.clone(key,byte_limit)
}
pub fn value_at(map:&StringMap,index:i64)->Option<i64> {
    if index<0 || index>=store_len(map.values) {return Option.None}
    return Option.Some(at_value(map.values,index))
}
// Allocation failure preserves entries and returns the key; earlier successful
// reserve operations may already have increased one Store capacity.
pub fn insert(map:&mut StringMap,key:string.String,value:i64)->InsertError!Put {
    let at=index(map.keys,&key)
    if at>=0 {
        // A duplicate retains its original key and consumes the incoming key.
        let prior=at_value(map.values,at)
        set_value(map.values,at,value)
        return Put{replaced:true,previous:prior}
    }
    let length=store_len(map.keys)
    var current=store_capacity(map.keys)
    if store_capacity(map.values)>current {current=store_capacity(map.values)}
    if length==store_capacity(map.keys) || length==store_capacity(map.values) {
        if current>4611686018427387903 {return InsertError{key:key,reason:Failure.Capacity}}
        var next:i64=8
        if current>=8 {next=current*2}
        if !store_reserve(map.keys,next) {return InsertError{key:key,reason:Failure.Allocation}}
        if !store_reserve(map.values,next) {return InsertError{key:key,reason:Failure.Allocation}}
    }
    // Push the Copy value first. If the owning key push fails, its error
    // returns the original key and the value slot can be rolled back.
    match store_push(map.values,value) {
        .err(_)=>{return InsertError{key:key,reason:Failure.Internal}},
        .ok(_)=>{}
    }
    match store_push(map.keys,key) {
        .ok(_)=>{return Put{replaced:false,previous:0}},
        .err(original)=>{
            store_pop(map.values)
            return InsertError{key:original,reason:Failure.Internal}
        }
    }
}

pub fn remove(map:&mut StringMap,key:&string.String)->Failure!Option<i64> {
    let at=index(map.keys,key)
    if at<0 {return Option.None}
    match store.remove::<string.String>(map.keys,at) {
        .err(_)=>{return Failure.Allocation},
        .ok(_)=>{}
    }
    let old=at_value(map.values,at)
    var pos=at
    while pos<store_len(map.values)-1 {
        let next=at_value(map.values,pos+1)
        set_value(map.values,pos,next)
        pos+=1
    }
    match store_pop(map.values) {
        .ok(_)=>{return Option.Some(old)},
        .err(_)=>{return Failure.Internal}
    }
}
