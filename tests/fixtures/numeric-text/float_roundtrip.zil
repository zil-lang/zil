import std.float
extern "C" fn test_float_mode()->i32;
extern "C" fn input_f64(index:i32)->f64;
extern "C" fn input_f32(index:i32)->f32;
extern "C" fn check_f64(value:f64,index:i32)->i32;
extern "C" fn check_f32(value:f32,index:i32)->i32;
fn check_double(index:i32)->i32 {
    var input:f64=0.0
    unsafe{input=input_f64(index)}
    match float.format_f64(input) {
        .err(_)=>{return 1},
        .ok(text)=>{match float.parse_f64(&text.bytes[..text.len]){
            .err(_)=>{return 2},
            .ok(value)=>{var same:i32=0;unsafe{same=check_f64(value,index)};if same!=1{return 3}}
        }}
    }
    return 0
}
fn check_single(index:i32)->i32 {
    var input:f32=0.0f32
    unsafe{input=input_f32(index)}
    match float.format_f32(input) {
        .err(_)=>{return 4},
        .ok(text)=>{match float.parse_f32(&text.bytes[..text.len]){
            .err(_)=>{return 5},
            .ok(value)=>{var same:i32=0;unsafe{same=check_f32(value,index)};if same!=1{return 6}}
        }}
    }
    return 0
}
fn main()->i32 {
    var mode:i32=0
    unsafe{mode=test_float_mode()}
    if mode!=0 {
        match float.parse_f64("1.25") {.ok(_)=>{return 7},.err(error)=>{match error{float.Error.Locale()=>{},_=>{return 8}}}}
        return 0
    }
    var at:i32=0
    while at<2000 {
        let first=check_double(at)
        if first!=0 {return first}
        let second=check_single(at)
        if second!=0 {return second}
        at+=1
    }
    // Exact midpoint rounds to the even significand, including f32 directly.
    match float.parse_f64("0.500000000000000055511151231257827021181583404541015625") {.err(_)=>{return 9},.ok(value)=>{if value!=0.5{return 10}}}
    match float.parse_f32("1.000000059604644775390625") {.err(_)=>{return 11},.ok(value)=>{if value!=1.0f32{return 12}}}
    return 0
}
