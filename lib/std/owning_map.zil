// A finite map with complete owning keys and values. KeyEq must be stable for
// stored keys. Replacement may move an entry to the end of iteration order.
import std.owning_entries
import std.store

pub trait KeyEq { fn same(self:&Self,other:&Self)->bool; }
pub package OwningMap<K:StoreElement+KeyEq,V:StoreElement> {
    manages items:Store<owning_entries.Entry<K,V>>
}

pub fn new<K:StoreElement+KeyEq,V:StoreElement>()->OwningMap<K,V> {
    return OwningMap::<K,V>{items:store_new()}
}

pub fn len<K:StoreElement+KeyEq,V:StoreElement>(map:&OwningMap<K,V>)->i64 {
    return store_len(map.items)
}

// The returned index is a snapshot. A later mutation may move entries.
pub fn find<K:StoreElement+KeyEq,V:StoreElement>(map:&OwningMap<K,V>,key:&K)->i64 {
    var at:i64=0
    while at<store_len(map.items) {
        let entry=store_at(map.items,at)
        let stored=&entry.key
        if stored.same(key) {return at}
        at+=1
    }
    return -1
}

pub fn contains<K:StoreElement+KeyEq,V:StoreElement>(map:&OwningMap<K,V>,key:&K)->bool {
    return find::<K,V>(map,key)>=0
}

// A Copy value can be read without moving its owning key. General V remains
// available through take, which transfers the complete old Entry.
pub fn get_copy<K:StoreElement+KeyEq,V:Copy+StoreElement>(map:&OwningMap<K,V>,key:&K)->Option<V> {
    let at=find::<K,V>(map,key)
    if at<0 {return Option::<V>.None()}
    let entry=store_at(map.items,at)
    return Option::<V>.Some(entry.value)
}

// A failed reserve or suffix preparation returns the original incoming Entry
// while preserving all old map entries. A successful replacement consumes and
// cleans the old Entry exactly once. After removal, the existing capacity has
// one spare slot, so the final push cannot allocate or recoverably fail.
pub fn insert_or_replace<K:StoreElement+KeyEq,V:StoreElement>(map:&mut OwningMap<K,V>,entry:owning_entries.Entry<K,V>)->owning_entries.Entry<K,V>!bool {
    let at=find::<K,V>(map,&entry.key)
    if at>=0 {
        match store.remove::<owning_entries.Entry<K,V>>(map.items,at) {
            .err(_)=>{return entry},
            .ok(old)=>{store_push(map.items,entry)?;return true}
        }
    }
    let length=store_len(map.items)
    if length>=4611686018427387903 || !store_reserve(map.items,length+1) {return entry}
    match store_push(map.items,entry) {
        .err(original)=>{return original},
        .ok(_)=>{return false}
    }
}

// Returns the old Entry with replaced=true, or returns the original incoming
// Entry with replaced=false when the key is missing or suffix preparation
// fails. The status is a separate caller-owned Copy slot because both outcomes
// transfer the same complete Entry type. A successful remove frees one slot.
pub fn replace_existing<K:StoreElement+KeyEq,V:StoreElement>(map:&mut OwningMap<K,V>,entry:owning_entries.Entry<K,V>,replaced:&mut bool)->owning_entries.Entry<K,V> {
    *replaced=false
    let at=find::<K,V>(map,&entry.key)
    if at<0 {return entry}
    match store.remove::<owning_entries.Entry<K,V>>(map.items,at) {
        .err(_)=>{return entry},
        .ok(old)=>{
            match store_push(map.items,entry) {
                .err(original)=>{store_push(map.items,old);return original},
                .ok(_)=>{*replaced=true;return old}
            }
        }
    }
}

// A miss leaves the map unchanged; a hit transfers the actual key and value.
pub fn take<K:StoreElement+KeyEq,V:StoreElement>(map:&mut OwningMap<K,V>,key:&K)->void!owning_entries.Entry<K,V> {
    let at=find::<K,V>(map,key)
    if at<0 {return ()}
    return store.remove::<owning_entries.Entry<K,V>>(map.items,at)
}
