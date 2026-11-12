import std.owning_map
import std.owning_hash_map
extern "C" fn putchar(value:i32)->i32;
resource Lease {code:i32}
impl Drop for Lease {fn drop(self:Self){unsafe{putchar(.code)}}}
pub struct Key {pub id:u32}
pub package Packet {manages data:ByteBuffer; manages lease:Lease; pub id:u32}
impl owning_map.KeyEq for Key {fn same(self:&Self,other:&Self)->bool{return self.id==other.id}}
impl owning_hash_map.KeyHash for Key {fn hash(self:&Self)->i64{return i64(self.id)}}
pub fn packet(id:u32)->void!Packet {
    let data=bytes_new()
    if !bytes_reserve(data,1) || !bytes_push(data,u8(id)) {return ()}
    return Packet{data:give data,lease:Lease{code:i32(id)+45},id:id}
}
pub fn amount(packet:&Packet)->u32 {return packet.id}
pub fn echo<T:Value>(value:T)->T {return value}
