import std.utf8
import std.text_edit

pub enum Error { InvalidUtf8, Range, Capacity, Allocation, Internal }
// The private buffer is always initialized, owned, and valid UTF-8.
pub package String { manages data:ByteBuffer }

pub fn empty()->String { return String{data:bytes_new()} }
pub fn from_utf8(source:&[u8], byte_limit:i64)->Error!String {
    let valid=utf8.locate(source,0,0)
    if valid.error>=0 { return Error.InvalidUtf8 }
    if byte_limit<0 || source.len()>byte_limit { return Error.Capacity }
    let data=bytes_new()
    if !bytes_reserve(data,source.len()) { return Error.Allocation }
    var at:i64=0
    while at<source.len() {
        if !bytes_push(data,source[at]) { return Error.Internal }
        at+=1
    }
    return String{data:give data}
}
pub fn byte_len(value:&String)->i64 { return bytes_len(value.data) }
pub fn scalar_len(value:&String)->i64 { return utf8.locate(bytes_view(value.data),0,0).count }
pub fn capacity(value:&String)->i64 { return bytes_capacity(value.data) }
pub fn as_bytes(value:&String)->&[u8] { return bytes_view(value.data) }
pub fn into_bytes(value:String)->ByteBuffer contract { result(give) } { return give value.data }
pub fn clone(value:&String, byte_limit:i64)->Error!String { return from_utf8(bytes_view(value.data),byte_limit) }

// begin/end count Unicode scalars, not bytes or grapheme clusters.
pub fn replace(value:&mut String, begin:i64, end:i64, insertion:&[u8], byte_limit:i64)->Error!void {
    let result=text_edit.replace_in_place(value.data,begin,end,insertion,byte_limit)
    if result.code==0 { return () }
    if result.code==1 || result.code==2 { return Error.InvalidUtf8 }
    if result.code==3 { return Error.Range }
    if result.code==4 { return Error.Capacity }
    if result.code==5 { return Error.Allocation }
    return Error.Internal
}
pub fn append(value:&mut String, insertion:&[u8], byte_limit:i64)->Error!void {
    let end=scalar_len(value)
    return replace(value,end,end,insertion,byte_limit)
}
pub fn clear(value:&mut String) { bytes_truncate(value.data,0) }
pub fn scalar_at(value:&String, byte_offset:i64)->utf8.Scalar { return utf8.decode(bytes_view(value.data),byte_offset) }
