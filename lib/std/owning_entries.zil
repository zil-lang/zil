// Indexed storage of complete owning key and value pairs. This preserves
// insertion order; callers choose positions and supply their own key policy.
import std.store

pub package Entry<K:StoreElement,V:StoreElement> { pub key:K; pub value:V }
pub package OwningEntries<K:StoreElement,V:StoreElement> { manages items:Store<Entry<K,V>> }

pub fn new<K:StoreElement,V:StoreElement>()->OwningEntries<K,V> {
    return OwningEntries::<K,V>{items:store_new()}
}

pub fn len<K:StoreElement,V:StoreElement>(entries:&OwningEntries<K,V>)->i64 {
    return store_len(entries.items)
}

pub fn reserve<K:StoreElement,V:StoreElement>(entries:&mut OwningEntries<K,V>,capacity:i64)->bool {
    return store_reserve(entries.items,capacity)
}

// A failed push returns the entire Entry to its caller.
pub fn push<K:StoreElement,V:StoreElement>(entries:&mut OwningEntries<K,V>,entry:Entry<K,V>)->Entry<K,V>!void {
    return store_push(entries.items,entry)
}

// std.store.insert prepares the complete suffix before any owned transfer.
// The caller gets the untouched Entry back on range or allocation failure.
pub fn insert_at<K:StoreElement,V:StoreElement>(entries:&mut OwningEntries<K,V>,index:i64,entry:Entry<K,V>)->Entry<K,V>!void {
    let length=store_len(entries.items)
    if length>=4611686018427387903 || !store_reserve(entries.items,length+1) {return entry}
    return store.insert::<Entry<K,V>>(entries.items,index,entry)
}

// A failed removal leaves all entries in place; success transfers the old
// key and value together after complete suffix preparation.
pub fn remove_at<K:StoreElement,V:StoreElement>(entries:&mut OwningEntries<K,V>,index:i64)->void!Entry<K,V> {
    return store.remove::<Entry<K,V>>(entries.items,index)
}
