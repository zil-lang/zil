import std.owning_map
import std.owning_entries
package Key { manages data:ByteBuffer; id:i64 }
impl owning_map.KeyEq for Key { fn same(self:&Self,other:&Self)->bool{return self.id==other.id} }
package Value { manages data:ByteBuffer; number:i64 }
fn inspect<life s>(found:Option<&'s Value>)->i64 {
    match found { Option.Some(value)=>{return value.number}, Option.None()=>{return -1} }
}
fn result()->i64 {
    var map=owning_map.new::<Key,Value>()
    let key=Key{data:bytes_new(),id:7}
    let value=Value{data:bytes_new(),number:42}
    match owning_map.insert_or_replace::<Key,Value>(&mut map,owning_entries.Entry::<Key,Value>{key:key,value:value}) {
        .ok(_)=>{}, .err(_)=>{return 1}
    }
    let query=Key{data:bytes_new(),id:7}
    let found=owning_map.get::<Key,Value>(&map,&query)
    let observed=inspect(found)
    return observed
}

fn main()->i32{return i32(result()-42)}
