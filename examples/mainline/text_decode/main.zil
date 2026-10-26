import buffer
import text

extern "C" fn getchar()->i32;

// A bounded text-input consumer; the decoder and UTF-16 writer are general views.
// Entry "value" makes the CLI print the full result for reproducible checking.
fn value()->i64 {
    let data=bytes_new();
    var input:i32=0;
    while true{
        unsafe{input=getchar()}
        if input<0{break}
        if bytes_len(data)>=1024{return -2000}
        if !buffer.append(data,u8(input)){return -2001}
    }
    let bytes=bytes_view(data);
    let metrics=text.inspect(bytes);
    if metrics.error_at>=0{return -(metrics.error_at+1)}
    var wide=[0u16,0u16,0u16,0u16,0u16,0u16,0u16,0u16,
        0u16,0u16,0u16,0u16,0u16,0u16,0u16,0u16,
        0u16,0u16,0u16,0u16,0u16,0u16,0u16,0u16,
        0u16,0u16,0u16,0u16,0u16,0u16,0u16,0u16];
    let units=text.write_utf16(bytes,&mut wide[..]);
    if units<0{return -3000}
    var hash:i64=0;
    var at:i64=0;
    while at<units{hash=(hash*33+i64(wide[at]))%1000000000;at+=1}
    let end=text.byte_position(bytes,metrics.scalars);
    if end!=bytes.len(){return -4000}
    return metrics.scalars*1000000000000+units*1000000000+hash
}
