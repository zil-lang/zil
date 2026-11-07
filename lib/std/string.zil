import std.utf8
import std.text_edit

pub enum Error { InvalidUtf8, Range, Capacity, Allocation, Internal, InvalidScalar }
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

pub fn is_empty(value:&String)->bool { return byte_len(value)==0 }
// UTF-8 preserves Unicode scalar order in lexicographic byte order.
// Return -1, 0, or 1; no normalization or locale collation is performed.
pub fn compare(left:&String, right:&String)->i32 {
    let a=bytes_view(left.data)
    let b=bytes_view(right.data)
    var at:i64=0
    while at<a.len() && at<b.len() {
        if a[at]<b[at] { return -1 }
        if a[at]>b[at] { return 1 }
        at+=1
    }
    if a.len()<b.len() { return -1 }
    if a.len()>b.len() { return 1 }
    return 0
}

// Return the scalar index of the first match at or after start, or -1.
// The needle is fully validated even when the haystack is empty or start is
// beyond its last possible match. Empty needles match at start, including end.
pub fn find_from(value:&String, needle:&[u8], start:i64)->Error!i64 {
    let checked=utf8.locate(needle,0,0)
    if checked.error>=0 { return Error.InvalidUtf8 }
    let haystack=bytes_view(value.data)
    let bounds=utf8.locate(haystack,start,start)
    if start<0 || bounds.first<0 { return Error.Range }
    var at=bounds.first
    var scalar=start
    while at<=haystack.len() {
        if needle.len()<=haystack.len()-at {
            var part:i64=0
            while part<needle.len() && haystack[at+part]==needle[part] { part+=1 }
            if part==needle.len() { return scalar }
        }
        if at==haystack.len() { break }
        let decoded=utf8.decode(haystack,at)
        if decoded.status!=1 { return Error.Internal }
        at=decoded.next
        scalar+=1
    }
    return i64(-1)
}
pub fn find(value:&String, needle:&[u8])->Error!i64 {
    return find_from(value,needle,0)
}
pub fn reserve(value:&mut String, capacity:i64, byte_limit:i64)->Error!void {
    if capacity<0 || byte_limit<0 || capacity>byte_limit { return Error.Capacity }
    if !bytes_reserve(value.data,capacity) { return Error.Allocation }
    return ()
}
pub fn push_scalar(value:&mut String, scalar:i32, byte_limit:i64)->Error!void {
    let width=utf8.encoded_len(scalar)
    if width==0 { return Error.InvalidScalar }
    var encoded=[0u8,0u8,0u8,0u8]
    if utf8.encode(scalar,&mut encoded[..])!=width { return Error.Internal }
    return append(value,&encoded[..width],byte_limit)
}
pub fn truncate(value:&mut String, scalar_count:i64)->Error!void {
    let bounds=utf8.locate(bytes_view(value.data),0,scalar_count)
    if scalar_count<0 || bounds.last<0 { return Error.Range }
    if !bytes_truncate(value.data,bounds.last) { return Error.Internal }
    return ()
}
pub fn substring(value:&String, begin:i64, end:i64, byte_limit:i64)->Error!String {
    let bounds=utf8.locate(bytes_view(value.data),begin,end)
    if begin<0 || end<begin || bounds.first<0 || bounds.last<0 { return Error.Range }
    let view=bytes_view(value.data)
    return from_utf8(&view[bounds.first..bounds.last],byte_limit)
}
