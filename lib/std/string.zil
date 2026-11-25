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
// Index by Unicode scalar position. A byte offset can be used with scalar_at.
pub fn scalar_at_index(value:&String,index:i64)->Error!i32 {
    if index<0 || index==9223372036854775807 {return Error.Range}
    let view=bytes_view(value.data)
    let bounds=utf8.locate(view,index,index+1)
    if bounds.error>=0 {return Error.Internal}
    if bounds.first<0 || bounds.last<0 {return Error.Range}
    let decoded=utf8.decode(view,bounds.first)
    if decoded.status!=1 {return Error.Internal}
    return decoded.value
}

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

fn is_ascii_whitespace(byte:u8)->bool {
    return byte==32u8 || byte==9u8 || byte==13u8 || byte==10u8
}

fn bytes_match_at(haystack:&[u8], offset:i64, needle:&[u8])->bool {
    if needle.len()>haystack.len()-offset { return false }
    var at:i64=0
    while at<needle.len() {
        if haystack[offset+at]!=needle[at] { return false }
        at+=1
    }
    return true
}

fn push_part(items:Store<String>, item:String)->Error!void {
    let length=store_len(items)
    let cap=store_capacity(items)
    if length>=cap {
        if length>=9223372036854775807 { return Error.Capacity }
        var next:i64=4
        if cap>=4 {
            if cap>4611686018427387903 { next=9223372036854775807 }
            else { next=cap*2 }
        }
        if !store_reserve(items,next) { return Error.Allocation }
    }
    match store_push(items,item) {
        .ok(_)=>{ return () },
        .err(_)=>{ return Error.Internal }
    }
}

fn append_bytes_bounded(out:ByteBuffer, slice:&[u8], byte_limit:i64)->Error!void {
    let cur=bytes_len(out)
    if slice.len()>byte_limit-cur { return Error.Capacity }
    let needed=cur+slice.len()
    if needed>bytes_capacity(out) {
        var next=bytes_capacity(out)
        if next<16 { next=16 }
        if next>byte_limit { next=byte_limit }
        while next<needed {
            if next>byte_limit/2 { next=byte_limit; break }
            next*=2
        }
        if next<needed { next=needed }
        if !bytes_reserve(out,next) { return Error.Allocation }
    }
    var at:i64=0
    while at<slice.len() {
        if !bytes_push(out,slice[at]) { return Error.Internal }
        at+=1
    }
    return ()
}

// Prefix/suffix/substring predicates validate the pattern as UTF-8 even when
// value is empty or shorter than the pattern.
pub fn starts_with(value:&String, prefix:&[u8])->Error!bool {
    if utf8.locate(prefix,0,0).error>=0 { return Error.InvalidUtf8 }
    let view=bytes_view(value.data)
    return bytes_match_at(view,0,prefix)
}

pub fn ends_with(value:&String, suffix:&[u8])->Error!bool {
    if utf8.locate(suffix,0,0).error>=0 { return Error.InvalidUtf8 }
    let view=bytes_view(value.data)
    if suffix.len()>view.len() { return false }
    return bytes_match_at(view,view.len()-suffix.len(),suffix)
}

pub fn contains(value:&String, needle:&[u8])->Error!bool {
    let idx=find(value,needle)?
    return idx>=0
}

// Trims leading/trailing ASCII whitespace (' ', '\t', '\r', '\n').
pub fn trim_start(value:&String, byte_limit:i64)->Error!String {
    let view=bytes_view(value.data)
    var begin:i64=0
    while begin<view.len() && is_ascii_whitespace(view[begin]) { begin+=1 }
    return from_utf8(&view[begin..view.len()],byte_limit)
}

pub fn trim_end(value:&String, byte_limit:i64)->Error!String {
    let view=bytes_view(value.data)
    var end=view.len()
    while end>0 && is_ascii_whitespace(view[end-1]) { end-=1 }
    return from_utf8(&view[0..end],byte_limit)
}

pub fn trim(value:&String, byte_limit:i64)->Error!String {
    let view=bytes_view(value.data)
    var begin:i64=0
    while begin<view.len() && is_ascii_whitespace(view[begin]) { begin+=1 }
    var end=view.len()
    while end>begin && is_ascii_whitespace(view[end-1]) { end-=1 }
    return from_utf8(&view[begin..end],byte_limit)
}

// Splits value on non-overlapping occurrences of UTF-8 separator sep.
// An empty separator splits around every Unicode scalar (including leading and
// trailing empty parts). byte_limit bounds the total emitted payload bytes.
pub fn split(value:&String, sep:&[u8], byte_limit:i64)->Error!Store<String> contract {result(create)} {
    if utf8.locate(sep,0,0).error>=0 { return Error.InvalidUtf8 }
    if byte_limit<0 { return Error.Capacity }
    let out:Store<String>=store_new()
    let view=bytes_view(value.data)
    var remaining=byte_limit
    if sep.len()==0 {
        push_part(out,empty())?
        var at:i64=0
        while at<view.len() {
            let decoded=utf8.decode(view,at)
            if decoded.status!=1 { return Error.Internal }
            let part=from_utf8(&view[at..decoded.next],remaining)?
            remaining-=decoded.next-at
            push_part(out,part)?
            at=decoded.next
        }
        push_part(out,empty())?
        return give out
    }
    var seg_start:i64=0
    var at:i64=0
    while at<=view.len()-sep.len() {
        if bytes_match_at(view,at,sep) {
            let part=from_utf8(&view[seg_start..at],remaining)?
            remaining-=at-seg_start
            push_part(out,part)?
            at+=sep.len()
            seg_start=at
        } else {
            at+=1
        }
    }
    let tail=from_utf8(&view[seg_start..view.len()],remaining)?
    push_part(out,tail)?
    return give out
}

// Splits value into lines terminated by '\n' or "\r\n", omitting a trailing
// empty line after a final line terminator (matching Rust str::lines).
pub fn lines(value:&String, byte_limit:i64)->Error!Store<String> contract {result(create)} {
    if byte_limit<0 { return Error.Capacity }
    let out:Store<String>=store_new()
    let view=bytes_view(value.data)
    var remaining=byte_limit
    var start:i64=0
    while start<view.len() {
        var at=start
        while at<view.len() && view[at]!=10u8 { at+=1 }
        var line_end=at
        if at<view.len() && line_end>start && view[line_end-1]==13u8 {
            line_end-=1
        }
        let part=from_utf8(&view[start..line_end],remaining)?
        remaining-=line_end-start
        push_part(out,part)?
        if at<view.len() {
            start=at+1
        } else {
            start=view.len()
        }
    }
    return give out
}

// Replaces all non-overlapping occurrences of UTF-8 pattern `from` with `to`.
// An empty `from` inserts `to` before each Unicode scalar and at the end.
pub fn replace_all(value:&String, from:&[u8], to:&[u8], byte_limit:i64)->Error!String {
    if utf8.locate(from,0,0).error>=0 { return Error.InvalidUtf8 }
    if utf8.locate(to,0,0).error>=0 { return Error.InvalidUtf8 }
    if byte_limit<0 { return Error.Capacity }
    let view=bytes_view(value.data)
    let out=bytes_new()
    if from.len()==0 {
        append_bytes_bounded(out,to,byte_limit)?
        var at:i64=0
        while at<view.len() {
            let decoded=utf8.decode(view,at)
            if decoded.status!=1 { return Error.Internal }
            append_bytes_bounded(out,&view[at..decoded.next],byte_limit)?
            append_bytes_bounded(out,to,byte_limit)?
            at=decoded.next
        }
        return String{data:give out}
    }
    var seg_start:i64=0
    var at:i64=0
    while at<=view.len()-from.len() {
        if bytes_match_at(view,at,from) {
            if at>seg_start {
                append_bytes_bounded(out,&view[seg_start..at],byte_limit)?
            }
            append_bytes_bounded(out,to,byte_limit)?
            at+=from.len()
            seg_start=at
        } else {
            at+=1
        }
    }
    if seg_start<view.len() {
        append_bytes_bounded(out,&view[seg_start..view.len()],byte_limit)?
    }
    return String{data:give out}
}
