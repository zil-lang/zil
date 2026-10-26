pub fn append(buffer:ByteBuffer,byte:u8)->bool {
    let length=bytes_len(buffer);
    let capacity=bytes_capacity(buffer);
    if length==capacity{
        var next:i64=8;
        if capacity>0{next=capacity*2}
        if !bytes_reserve(buffer,next){return false}
    }
    return bytes_push(buffer,byte)
}
