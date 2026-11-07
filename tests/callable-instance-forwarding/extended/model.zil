extern "C" fn getchar()->i32;
extern "C" fn putchar(code:i32)->i32;
pub fn read()->i32{unsafe{return getchar()}}
pub fn emit(code:i32){unsafe{putchar(code)}}
pub resource Mark{pub code:i32}
impl Drop for Mark{fn drop(self:Self){emit(.code)}}
pub enum Fault{NoMemory,Cancelled}
pub package Leaf{manages mark:Mark;manages bytes:ByteBuffer;pub code:i32}
pub package Items{manages values:Store<Leaf>}
pub enum Packet{Empty,Stored(Items)}
pub package FaultData{pub packet:Packet}
pub fn prepare(base:i32,cancel:bool)->Fault!Items{
    let values:Store<Leaf>=store_new()
    if !store_reserve(values,2){return Fault.NoMemory}
    for i in 0..2{
        let mark=Mark{code:base+i}
        let bytes=bytes_new()
        if !bytes_reserve(bytes,1){return Fault.NoMemory}
        bytes_push(bytes,85u8)
        store_push(values,Leaf{mark:give mark,bytes:give bytes,code:85})
    }
    if cancel{return Fault.Cancelled}
    return Items{values:give values}
}
pub fn checked(items:Items)->Items{
    if store_len(items.values)!=2{emit(88)}
    for i in 0..2{
        let leaf=store_at(items.values,i64(i))
        if leaf.code!=85{emit(88)}
    }
    return items
}
pub fn marker(label:i32)->impl Fn(i32)->i32{
    let mark=Mark{code:label}
    return move(value:i32)->i32=>{emit(mark.code+32);return value+mark.code}
}
