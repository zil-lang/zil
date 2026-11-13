
import std.float
import std.io
import std.buffered_io
fn check(value:f64)->i32 {
    match float.format_f64(value) {
        .err(_)=>{return 1},
        .ok(formatted)=>{
            match float.parse_f64(&formatted.bytes[..formatted.len]) {.err(_)=>{return 2},.ok(parsed)=>{if parsed!=value{return 3}}}
        }
    }
    return 0
}
fn main()->i32 {
    if check(0.1)!=0 || check(1.7976931348623157e308)!=0 || check(5e-324)!=0 || check(-0.0)!=0 {return 1}
    match float.parse_f64("1e309") {.err(_)=>{},.ok(_)=>{return 2}}
    match float.parse_f64("1e-9999") {.err(_)=>{},.ok(_)=>{return 3}}
    match float.parse_f64("1.2x") {.err(_)=>{},.ok(_)=>{return 4}}
    match float.format_f32(0.1f32) {.err(_)=>{return 5},.ok(formatted)=>{match float.parse_f32(&formatted.bytes[..formatted.len]){.err(_)=>{return 6},.ok(parsed)=>{if parsed!=0.1f32{return 7}}}}}
    let path="TEST_FILE_PATH"
    match io.create_write(path,384) {
        .err(_)=>{return 8},
        .ok(file)=>{
            var writer=buffered_io.writer(give file)
            var at:i64=0
            while at<200 {match buffered_io.write_byte(&mut writer,u8(at%127)){.err(_)=>{return 9},.ok(_)=>{}};at+=1}
            match buffered_io.finish(writer){.err(_)=>{return 10},.ok(file)=>{}}
        }
    }
    match io.open_read(path) {
        .err(_)=>{return 11},
        .ok(file)=>{
            var reader=buffered_io.reader(give file)
            var at:i64=0
            while at<200 {match buffered_io.read_byte(&mut reader){.err(_)=>{return 12},.ok(byte)=>{match byte{Option.None()=>{return 13},Option.Some(value)=>{if value!=u8(at%127){return 14}}}}};at+=1}
            match buffered_io.read_byte(&mut reader){.err(_)=>{return 15},.ok(byte)=>{match byte{Option.None()=>{},Option.Some(_)=>{return 16}}}}
        }
    }
    return 0
}