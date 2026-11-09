import protocol
pub package Item { manages data:ByteBuffer; pub tag:i64 }
impl protocol.Duplicate for Item {
    fn duplicate(self:&Self)->Self {
        let bytes=bytes_new()
        if bytes_reserve(bytes,1) {bytes_push(bytes,bytes_view(self.data)[0])}
        return Item{data:give bytes,tag:self.tag+1}
    }
    fn try_duplicate(self:&Self,fail:bool)->protocol.Error!Self {
        let bytes=bytes_new()
        if !bytes_reserve(bytes,1) {return protocol.Error.Allocation}
        bytes_push(bytes,bytes_view(self.data)[0])
        if fail {return protocol.Error.Requested}
        return Item{data:give bytes,tag:self.tag+2}
    }
}
pub fn make()->protocol.Error!Item {
    let bytes=bytes_new()
    if !bytes_reserve(bytes,1) {return protocol.Error.Allocation}
    bytes_push(bytes,65u8)
    return Item{data:give bytes,tag:7}
}
pub fn size(value:&Item)->i64 {return bytes_len(value.data)}
pub fn byte(value:&Item)->u8 {return bytes_view(value.data)[0]}
