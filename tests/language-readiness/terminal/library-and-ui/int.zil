import std.int

fn check(value:i64,expected:&[u8])->i32 {
    let rendered=int.format_i64(value)
    if rendered.len!=expected.len() {return 1}
    var at:i64=0
    while at<rendered.len {
        if rendered.bytes[at]!=expected[at] {return 2}
        at+=1
    }
    match int.parse_i64(&rendered.bytes[..rendered.len]) {
        .err(_)=>{return 3},
        .ok(parsed)=>{if parsed!=value {return 4}}
    }
    return 0
}
fn main()->i32 {
    if check(0,"0")!=0 {return 1}
    if check(-42,"-42")!=0 {return 2}
    if check(9223372036854775807,"9223372036854775807")!=0 {return 3}
    if check(-9223372036854775808,"-9223372036854775808")!=0 {return 4}
    match int.parse_i64("+0") {.err(_)=>{return 5},.ok(value)=>{if value!=0 {return 6}}}
    match int.parse_i64("9223372036854775808") {
        .ok(_)=>{return 7},
        .err(error)=>{match error {int.ParseError.Overflow()=>{},_=>{return 8}}}
    }
    match int.parse_i64("-9223372036854775809") {
        .ok(_)=>{return 9},
        .err(error)=>{match error {int.ParseError.Overflow()=>{},_=>{return 10}}}
    }
    match int.parse_i64("--1") {
        .ok(_)=>{return 11},
        .err(error)=>{match error {int.ParseError.InvalidDigit()=>{},_=>{return 12}}}
    }
    match int.parse_i64("") {
        .ok(_)=>{return 13},
        .err(error)=>{match error {int.ParseError.Empty()=>{},_=>{return 14}}}
    }
    return 0
}
