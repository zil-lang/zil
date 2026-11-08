extern "C" fn putchar(code:i32)->i32;
pub fn emit(code:i32){unsafe{putchar(code)}}
resource Marker{code:i32}
impl Drop for Marker{fn drop(self:Self){emit(.code)}}
pub package Item{manages payload:ByteBuffer;manages marker:Marker;pub tag:i32}
pub package Reply{manages payload:ByteBuffer;manages marker:Marker;pub tag:i32}
pub fn item()->void!Item{
    let bytes=bytes_new();if !bytes_reserve(bytes,5){return ()}
    return Item{payload:give bytes,marker:Marker{code:73},tag:7}
}
pub fn reply(item:&Item,count:i32)->void!Reply{
    let bytes=bytes_new();if !bytes_reserve(bytes,7){return ()}
    return Reply{payload:give bytes,marker:Marker{code:65+count},tag:item.tag+count}
}
