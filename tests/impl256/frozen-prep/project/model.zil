extern "C" fn getchar()->i32;
extern "C" fn putchar(code:i32)->i32;
pub fn read()->i32{unsafe{return getchar()}}
pub fn emit(code:i32){unsafe{putchar(code)}}
pub resource Mark{pub code:i32}
impl Drop for Mark{fn drop(self:Self){emit(.code)}}
pub enum Fault{NoMemory,Cancelled}
pub package Event{manages mark:Mark;manages bytes:ByteBuffer;pub code:i32}
pub package Reply{manages mark:Mark;manages bytes:ByteBuffer;pub code:i32}
pub enum Packet{Empty,Data(Event)}
pub package Meta{pub packet:Packet}
pub fn prepare(label:i32,code:i32,cancel:bool)->Fault!Event{
    let mark=Mark{code:label}
    let bytes=bytes_new()
    if !bytes_reserve(bytes,1){return Fault.NoMemory}
    bytes_push(bytes,85u8)
    if cancel{return Fault.Cancelled}
    return Event{mark:give mark,bytes:give bytes,code:code}
}
pub fn reply(event:Event,add:i32)->Reply{
    return Reply{mark:give event.mark,bytes:give event.bytes,code:event.code+add}
}
pub fn render(reply:Reply,want:i32)->i32{
    emit(82)
    if bytes_len(reply.bytes)!=1{return 2}
    let view=bytes_view(reply.bytes)
    if view[0]!=85u8{return 3}
    return reply.code-want
}
pub fn marker(label:i32)->impl Fn(i32)->i32{
    let mark=Mark{code:label}
    return move(value:i32)->i32=>{emit(mark.code+32);return value+mark.code}
}
