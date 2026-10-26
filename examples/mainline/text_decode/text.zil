import utf8

pub struct Metrics { pub scalars:i64; pub units:i64; pub error_at:i64 }

pub fn inspect(bytes:&[u8])->Metrics {
    var offset:i64=0;
    var scalars:i64=0;
    var units:i64=0;
    while offset<bytes.len(){
        let step=utf8.decode(bytes,offset);
        if !step.valid{return Metrics{scalars:scalars,units:units,error_at:offset}}
        scalars+=1;
        if step.value>65535{units+=2}else{units+=1}
        offset=step.next;
    }
    return Metrics{scalars:scalars,units:units,error_at:-1}
}

// -1: invalid input; -2: insufficient output. Both leave the output untouched.
// A valid NUL is data; no terminator is appended to this explicit-length view.
pub fn write_utf16(bytes:&[u8],output:&mut [u16])->i64 {
    let metrics=inspect(bytes);
    if metrics.error_at>=0{return -1}
    if metrics.units>output.len(){return -2}
    var offset:i64=0;
    var at:i64=0;
    while offset<bytes.len(){
        let step=utf8.decode(bytes,offset);
        if step.value<=65535{
            output[at]=u16(step.value);at+=1;
        }else{
            let value=step.value-65536;
            output[at]=u16(i64(55296)+(value>>10));
            output[at+1]=u16(i64(56320)+(value&1023));
            at+=2;
        }
        offset=step.next;
    }
    return at
}

// Map a Unicode scalar position to its byte position, including the end cursor.
// This is not a grapheme, normalized-text or shaped-glyph index.
pub fn byte_position(bytes:&[u8],scalar_position:i64)->i64 {
    let metrics=inspect(bytes);
    if metrics.error_at>=0 || scalar_position<0 || scalar_position>metrics.scalars{return -1}
    var offset:i64=0;
    var at:i64=0;
    while at<scalar_position{
        let step=utf8.decode(bytes,offset);
        offset=step.next;at+=1;
    }
    return offset
}
