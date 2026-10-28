import edit
extern "C" fn getchar()->i32;
extern "C" fn putchar(value:i32)->i32;
package State { manages data:ByteBuffer }

fn read_word()->i64 {
    unsafe {
        let a=getchar()
        let b=getchar()
        if a<0 || b<0 { return -1 }
        return i64(a)+i64(b)*256
    }
}
fn position(value:i64)->i64 { if value==65535 { return -1 }; return value }
fn write_word(value:i64) {
    var word=value
    if word<0 { word=65535 }
    unsafe { putchar(i32(word%256)); putchar(i32(word/256)) }
}
fn read_buffer(length:i64)->ByteBuffer contract { result(create) } {
    let data=bytes_new()
    if !bytes_reserve(data,length) { return give data }
    var at:i64=0
    while at<length {
        var byte:i32=-1
        unsafe { byte=getchar() }
        if byte<0 { return give data }
        if !bytes_push(data,u8(byte)) { return give data }
        at+=1
    }
    return give data
}
fn main()->i32 {
    let old_length=read_word()
    let insert_length=read_word()
    let begin=position(read_word())
    let end=position(read_word())
    let limit=position(read_word())
    if old_length<0 || insert_length<0 { return 90 }
    var state=State{data:read_buffer(old_length)}
    let insertion=read_buffer(insert_length)
    if bytes_len(state.data)!=old_length || bytes_len(insertion)!=insert_length { return 91 }
    let capacity=bytes_capacity(state.data)
    let answer=edit.replace_in_place(state.data,begin,end,bytes_view(insertion),limit)
    if answer.code!=0 && (bytes_len(state.data)!=old_length || bytes_capacity(state.data)!=capacity) { return 92 }
    if answer.code==0 && bytes_len(state.data)<=old_length && bytes_capacity(state.data)!=capacity { return 94 }
    unsafe { putchar(answer.code) }
    write_word(answer.source_error)
    write_word(answer.insert_error)
    write_word(bytes_len(state.data))
    let bytes=bytes_view(state.data)
    var at:i64=0
    while at<bytes.len() { unsafe { putchar(i32(bytes[at])) }; at+=1 }
    return 0
}
