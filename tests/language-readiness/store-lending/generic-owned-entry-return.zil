package Entry<K:StoreElement,V:StoreElement> { pub key:K; pub value:V }
trait KeyEq { fn same(self:&Self,other:&Self)->bool; }
package Key { manages data:ByteBuffer; id:i64 }
impl KeyEq for Key { fn same(self:&Self,other:&Self)->bool {return self.id==other.id} }
package Value { manages data:ByteBuffer; number:i64 }
package Map<K:StoreElement+KeyEq,V:StoreElement> { manages items:Store<Entry<K,V>> }
fn at<life s,K:StoreElement+KeyEq,V:StoreElement>(map:&'s Map<K,V>,index:i64)->&'s V {
    let entry=store_at(map.items,index)
    return &entry.value
}
fn result()->i64 {
    let items:Store<Entry<Key,Value>>=store_new()
    if !store_reserve(items,2) {return 1}
    store_push(items,Entry::<Key,Value>{key:Key{data:bytes_new(),id:7},value:Value{data:bytes_new(),number:42}})
    let map=Map::<Key,Value>{items:give items}
    let value=at::<Key,Value>(&map,0)
    let observed=value.number
    store_push(map.items,Entry::<Key,Value>{key:Key{data:bytes_new(),id:8},value:Value{data:bytes_new(),number:9}})
    return observed
}
