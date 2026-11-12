extern "C" fn putchar(value:i32)->i32;
resource Mark {code:i32}
impl Drop for Mark {fn drop(self:Self){unsafe{putchar(.code)}}}
pub trait Eq {fn same(self:&Self,other:&Self)->bool;}
pub trait Hash {fn hash(self:&Self)->i32;}
package Item {manages mark:Mark; value:i32}
impl Eq for Item {fn same(self:&Self,other:&Self)->bool{return self.value==other.value}}
impl Hash for Item {fn hash(self:&Self)->i32{return self.value}}
package Entry<K:StoreElement,V:StoreElement> {key:K;value:V}
package Map<K:StoreElement+Eq+Hash,V:StoreElement>{manages entries:Store<Entry<K,V>>}
fn get<life a,K:StoreElement+Eq+Hash,V:StoreElement>(map:&'a Map<K,V>,key:&K)->Option<&'a V>{
    if store_len(map.entries)==0{return Option::<&'a V>.None()}
    let entry=store_at(map.entries,0)
    if !entry.key.same(key) || entry.key.hash()!=key.hash(){return Option::<&'a V>.None()}
    return Option::<&'a V>.Some(&entry.value)
}
package Wrapper<K:StoreElement+Eq+Hash,V:StoreElement>{map:Map<K,V>}
fn forwarded<life b,K:StoreElement+Eq+Hash,V:StoreElement>(wrapper:&'b Wrapper<K,V>,key:&K)->Option<&'b V>{return get::<K,V>(&wrapper.map,key)}
fn main()->i32 {
    let entries:Store<Entry<Item,Item>>=store_new()
    if !store_reserve(entries,1){return 1}
    store_push(entries,Entry::<Item,Item>{key:Item{mark:Mark{code:65},value:7},value:Item{mark:Mark{code:66},value:42}})
    let map=Map::<Item,Item>{entries:give entries}
    let wrapper=Wrapper::<Item,Item>{map:map}
    let key=Item{mark:Mark{code:81},value:7}
    match forwarded(&wrapper,&key){Option.Some(value)=>{return value.value-42},Option.None()=>{return 2}}
}
