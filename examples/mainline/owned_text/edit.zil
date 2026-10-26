import utf8

// code: 0 success; 1 malformed source; 2 malformed insertion; 3 scalar range;
// 4 byte limit/overflow; 5 allocation failure; 6 primitive contract violation.
// A failed result owns an empty buffer. The input is never mutated.
pub package Edit { manages value:ByteBuffer; pub code:i32; pub source_error:i64; pub insert_error:i64 }

fn failure(code:i32, source_error:i64, insert_error:i64)->Edit {
    return Edit{value:bytes_new(),code:code,source_error:source_error,insert_error:insert_error}
}

pub fn replace(source:&[u8], begin:i64, end:i64, insertion:&[u8], byte_limit:i64)->Edit {
    let bounds=utf8.locate(source,begin,end)
    if bounds.error>=0 { return failure(1,bounds.error,-1) }
    let inserted=utf8.locate(insertion,0,0)
    if inserted.error>=0 { return failure(2,-1,inserted.error) }
    if begin<0 || end<begin || bounds.first<0 || bounds.last<0 { return failure(3,-1,-1) }
    let kept=source.len()-(bounds.last-bounds.first)
    if insertion.len()>9223372036854775807-kept { return failure(4,-1,-1) }
    let length=kept+insertion.len()
    if byte_limit<0 || length>byte_limit { return failure(4,-1,-1) }
    let value=bytes_new()
    if !bytes_reserve(value,length) { return failure(5,-1,-1) }
    var at:i64=0
    while at<bounds.first {
        if !bytes_push(value,source[at]) { return failure(6,-1,-1) }
        at+=1
    }
    at=0
    while at<insertion.len() {
        if !bytes_push(value,insertion[at]) { return failure(6,-1,-1) }
        at+=1
    }
    at=bounds.last
    while at<source.len() {
        if !bytes_push(value,source[at]) { return failure(6,-1,-1) }
        at+=1
    }
    return Edit{value:give value,code:0,source_error:-1,insert_error:-1}
}

// Consume the result package and transfer its one owned buffer.
pub fn take(answer:Edit)->ByteBuffer contract { result(give) } {
    return give answer.value
}
