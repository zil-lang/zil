
import std.owning_hash_map
import std.owning_hash_set
import std.owning_entries
import std.owning_map
package Key { manages bytes:ByteBuffer; id:i64 }
package Value { manages bytes:ByteBuffer; id:i64 }
impl owning_map.KeyEq for Key {fn same(self:&Self,other:&Self)->bool{return self.id==other.id}}
impl owning_hash_map.KeyHash for Key {fn hash(self:&Self)->i64{return -9223372036854775808}}
fn key(id:i64)->Key {let bytes=bytes_new();bytes_reserve(bytes,1);bytes_push(bytes,u8(id));return Key{bytes:give bytes,id:id}}
fn value(id:i64)->Value {let bytes=bytes_new();bytes_reserve(bytes,1);bytes_push(bytes,u8(id));return Value{bytes:give bytes,id:id}}

extern "C" fn probe_fail_next();
fn main()->i32 {
    var map=owning_hash_map.new::<Key,Value>()
    let first=owning_entries.Entry::<Key,Value>{key:key(7),value:value(66)}
    match owning_hash_map.insert::<Key,Value>(&mut map,first){.err(_)=>{return 1},.ok(_)=>{}}
    let second=owning_entries.Entry::<Key,Value>{key:key(9),value:value(68)}
    match owning_hash_map.insert::<Key,Value>(&mut map,second){.err(_)=>{return 2},.ok(_)=>{}}
    let query=key(7)
    let cap=owning_hash_map.capacity::<Key,Value>(&map)
    let incoming=owning_entries.Entry::<Key,Value>{key:key(7),value:value(70)}
    unsafe{probe_fail_next()}
    match owning_hash_map.insert::<Key,Value>(&mut map,incoming){
        .ok(_)=>{return 3},
        .err(original)=>{
            if original.key.id!=7 || original.value.id!=70 || bytes_view(original.value.bytes)[0]!=70u8 {return 4}
            if owning_hash_map.capacity::<Key,Value>(&map)!=cap || owning_hash_map.len::<Key,Value>(&map)!=2 {return 5}
            match owning_hash_map.get::<Key,Value>(&map,&query){Option.None()=>{return 6},Option.Some(old)=>{if old.id!=66 || bytes_view(old.bytes)[0]!=66u8{return 7}}}
            match owning_hash_map.insert::<Key,Value>(&mut map,original){.err(_)=>{return 8},.ok(replaced)=>{if !replaced{return 9}}}
        }
    }
    match owning_hash_map.take::<Key,Value>(&mut map,&query){.err(_)=>{return 10},.ok(old)=>{if old.value.id!=70 || bytes_view(old.value.bytes)[0]!=70u8{return 11}}}
    if owning_hash_map.clear::<Key,Value>(&mut map)!=1 {return 12}
    return 0
}
