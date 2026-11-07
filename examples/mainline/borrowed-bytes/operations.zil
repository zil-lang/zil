import model;

pub fn make()->ByteBuffer contract{result(create)}{return bytes_new()}
fn read_inner(buffer:ByteBuffer)->i32{return i32(bytes_len(buffer))}
pub fn read(buffer:ByteBuffer)->i32{return read_inner(buffer)}
fn clear_inner(buffer:ByteBuffer)->bool{return bytes_truncate(buffer,0)}
pub fn clear(buffer:ByteBuffer)->bool{return clear_inner(buffer)}
pub fn consume(give buffer:ByteBuffer){}

pub fn append(buffer:ByteBuffer)->bool!i32{
    let old_length=bytes_len(buffer);
    let old_capacity=bytes_capacity(buffer);
    var old_last=0;
    if old_length>0{old_last=i32(bytes_view(buffer)[old_length-1])};
    if !bytes_reserve(buffer,old_length+1){
        if bytes_len(buffer)!=old_length||bytes_capacity(buffer)!=old_capacity{model.emit(88)};
        if old_length>0{
            if i32(bytes_view(buffer)[old_length-1])!=old_last{model.emit(89)}
        };
        return true
    };
    if !bytes_push(buffer,u8(66+old_length)){return true};
    let length=i32(bytes_len(buffer));
    model.emit(48+length);
    return length
}
