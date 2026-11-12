// Linux/glibc floating text through fixed C ABIs. Explicit C numeric locale;
// no whitespace, hexadecimal input or trailing data. Native floating rounding
// mode applies, as for the language's ordinary floating arithmetic.
extern "C" fn newlocale(mask:i32,name:&u8,base:c_handle)->c_handle contract {provides(noescape(name));effects(ffi);};
extern "C" fn freelocale(locale:c_handle);
extern "C" fn uselocale(locale:c_handle)->c_handle;
extern "C" fn strtod_l(source:&u8,end:c_handle,locale:c_handle)->f64 contract {provides(noescape(source));effects(ffi);};
extern "C" fn strtof_l(source:&u8,end:c_handle,locale:c_handle)->f32 contract {provides(noescape(source));effects(ffi);};
extern "C" fn strfromd(output:&mut [u8;64],size:i64,format:&u8,value:f64)->i32 contract {provides(noescape(output),noescape(format));effects(ffi);};
extern "C" fn strfromf(output:&mut [u8;64],size:i64,format:&u8,value:f32)->i32 contract {provides(noescape(output),noescape(format));effects(ffi);};
import std.io
pub enum Error { Empty, Invalid, Range, Allocation, Locale, Format }
pub struct Decimal {pub bytes:[u8;64];pub len:i64}
resource Locale { handle:c_handle; previous:c_handle }
impl Drop for Locale {fn drop(self:Self){unsafe{uselocale(.previous);freelocale(.handle)}}}
fn locale()->Error!Locale contract {result(create)} {
    let name=[67u8,0u8]
    var handle=c_null()
    var previous=c_null()
    unsafe{handle=newlocale(2,&name[0],c_null())}
    if handle==c_null() {return Error.Locale}
    unsafe{previous=uselocale(handle)}
    if previous==c_null() {unsafe{freelocale(handle)};return Error.Locale}
    return Locale{handle:handle,previous:previous}
}
// The complete syntax is checked before calling a converter with null endptr.
// Return whether the decimal significand is nonzero, to detect underflow to 0.
fn validate(source:&[u8])->Error!bool {
    if source.len()==0 {return Error.Empty}
    var at:i64=0
    if source[0]==43u8 || source[0]==45u8 {at=1}
    let start=at
    var digits:i64=0
    var nonzero=false
    while at<source.len() && source[at]>=48u8 && source[at]<=57u8 {if source[at]!=48u8 {nonzero=true};digits+=1;at+=1}
    if at<source.len() && source[at]==46u8 {
        at+=1
        while at<source.len() && source[at]>=48u8 && source[at]<=57u8 {if source[at]!=48u8 {nonzero=true};digits+=1;at+=1}
    }
    if digits==0 {return Error.Invalid}
    if at<source.len() && (source[at]==101u8 || source[at]==69u8) {
        at+=1
        if at<source.len() && (source[at]==43u8 || source[at]==45u8) {at+=1}
        let exponent=at
        while at<source.len() && source[at]>=48u8 && source[at]<=57u8 {at+=1}
        if at==exponent {return Error.Invalid}
    }
    if at!=source.len() || start==source.len() {return Error.Invalid}
    return nonzero
}
fn terminated(source:&[u8])->Error!ByteBuffer contract {result(create)} {
    if source.len()==9223372036854775807 {return Error.Allocation}
    let bytes=bytes_new()
    if !bytes_reserve(bytes,source.len()+1) {return Error.Allocation}
    var at:i64=0
    while at<source.len() {if !bytes_push(bytes,source[at]) {return Error.Allocation};at+=1}
    if !bytes_push(bytes,0u8) {return Error.Allocation}
    return give bytes
}
// Special values use canonical lower-case spellings; NaN payload/sign are not
// represented by decimal text. Finite parsing below checks every byte.
fn special(source:&[u8])->i32 {
    var at:i64=0
    var negative=false
    if source.len()>0 && (source[0]==43u8 || source[0]==45u8) {negative=source[0]==45u8;at=1}
    if source.len()-at!=3 {return 0}
    if source[at]==105u8 && source[at+1]==110u8 && source[at+2]==102u8 {if negative {return 2};return 1}
    if source[at]==110u8 && source[at+1]==97u8 && source[at+2]==110u8 {return 3}
    return 0
}
pub fn parse_f64(source:&[u8])->Error!f64 {
    let kind=special(source)
    var zero:f64=0.0
    if kind==1 {return 1.0/zero}
    if kind==2 {return -1.0/zero}
    if kind==3 {return zero/zero}
    let nonzero=validate(source)?
    let bytes=terminated(source)?
    let scope=locale()?
    var value:f64=0.0
    unsafe{value=strtod_l(&bytes_view(bytes)[0],c_null(),scope.handle)}
    if value>1.7976931348623157e308 || value< -1.7976931348623157e308 || (nonzero && value==0.0) {return Error.Range}
    return value
}
pub fn parse_f32(source:&[u8])->Error!f32 {
    let kind=special(source)
    var zero:f32=0.0f32
    if kind==1 {return 1.0f32/zero}
    if kind==2 {return -1.0f32/zero}
    if kind==3 {return zero/zero}
    let nonzero=validate(source)?
    let bytes=terminated(source)?
    let scope=locale()?
    var value:f32=0.0f32
    unsafe{value=strtof_l(&bytes_view(bytes)[0],c_null(),scope.handle)}
    if value>3.4028234663852886e38f32 || value< -3.4028234663852886e38f32 || (nonzero && value==0.0f32) {return Error.Range}
    return value
}
// 17/9 significant digits round-trip every finite binary64/binary32 value in
// round-to-nearest mode, including subnormals and signed zero. Output is bounded.
pub fn format_f64(value:f64)->Error!Decimal {
    let scope=locale()?
    let format=[37u8,46u8,49u8,55u8,103u8,0u8]
    var bytes=io.empty_buffer()
    var count:i32=0
    unsafe{count=strfromd(&mut bytes,64,&format[0],value)}
    if count<0 || count>=64 {return Error.Format}
    return Decimal{bytes:bytes,len:i64(count)}
}
pub fn format_f32(value:f32)->Error!Decimal {
    let scope=locale()?
    let format=[37u8,46u8,57u8,103u8,0u8]
    var bytes=io.empty_buffer()
    var count:i32=0
    unsafe{count=strfromf(&mut bytes,64,&format[0],value)}
    if count<0 || count>=64 {return Error.Format}
    return Decimal{bytes:bytes,len:i64(count)}
}
