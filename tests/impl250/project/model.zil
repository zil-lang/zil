extern "C" fn getchar()->i32;
extern "C" fn putchar(n:i32)->i32;
pub fn read()->i32{unsafe{return getchar()}}
pub fn emit(n:i32){unsafe{putchar(n)}}
pub resource Mark{pub code:i32}
impl Drop for Mark{fn drop(self:Self){emit(.code)}}
pub enum Fault{NoMemory,Cancelled}
pub package Event{manages mark:Mark;manages payload:ByteBuffer;pub code:i32}
pub package Reply{manages mark:Mark;manages payload:ByteBuffer;pub code:i32}
pub enum Packet{Empty,Data(Event)}
pub package Leaf{manages mark:Mark;manages bytes:ByteBuffer}
pub package Items{manages values:Store<Leaf>}
pub fn prepare(cancel:bool,label:i32,code:i32)->Fault!Event{
    let mark=Mark{code:label}
    let payload=bytes_new()
    if !bytes_reserve(payload,1){return Fault.NoMemory}
    bytes_push(payload,85u8)
    if cancel{return Fault.Cancelled}
    return Event{mark:give mark,payload:give payload,code:code}
}
pub fn reply(event:Event,add:i32)->Reply{
    return Reply{mark:give event.mark,payload:give event.payload,code:event.code+add}
}
pub fn render(reply:Reply,want:i32)->i32{
    emit(82)
    if bytes_len(reply.payload)!=1{return 2}
    let view=bytes_view(reply.payload)
    if view[0]!=85u8{return 3}
    return reply.code-want
}
pub fn packet(packet:Packet)->i32{
    match packet{
        Packet.Empty()=>{emit(69);return 0}
        Packet.Data(event)=>{return render(reply(event,1),42)}
    }
}
pub fn items()->Fault!Items{
    let values:Store<Leaf>=store_new()
    if !store_reserve(values,2){return Fault.NoMemory}
    for i in 0..2{
        let mark=Mark{code:65+i}
        let bytes=bytes_new()
        if !bytes_reserve(bytes,1){return Fault.NoMemory}
        bytes_push(bytes,85u8)
        store_push(values,Leaf{mark:give mark,bytes:give bytes})
    }
    return Items{values:give values}
}
pub fn render_items(items:Items)->i32{
    emit(82)
    if store_len(items.values)!=2{return 4}
    for i in 0..2{
        let leaf=store_at(items.values,i64(i))
        if bytes_len(leaf.bytes)!=1{return 5}
        let view=bytes_view(leaf.bytes)
        if view[0]!=85u8{return 6}
    }
    return 0
}
