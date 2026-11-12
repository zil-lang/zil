import std.buffered_io
import std.io
extern "C" fn test_mode()->i32;
fn main()->i32 {
    var mode:i32=0
    unsafe{mode=test_mode()}
    match io.stdout() {
        .err(_)=>{return 1},
        .ok(file)=>{
            var writer=buffered_io.writer(give file)
            match buffered_io.write(&mut writer,"ABCDEFGHIJ") {.err(_)=>{return 2},.ok(count)=>{if count!=10{return 3}}}
            match buffered_io.finish(writer) {
                .ok(done)=>{if mode!=0{return 4}},
                .err(original)=>{
                    if mode==0 {return 5}
                    if mode==3 {if buffered_io.pending(&original)!=10{return 6}}
                    else {
                        if buffered_io.pending(&original)!=7{return 7}
                        match buffered_io.finish(original){.err(_)=>{return 8},.ok(done)=>{}}
                    }
                }
            }
        }
    }
    match io.stdin() {
        .err(_)=>{return 9},
        .ok(file)=>{
            var reader=buffered_io.reader(give file)
            var at:i64=0
            while at<7 {
                match buffered_io.read_byte(&mut reader){.err(_)=>{return 10},.ok(byte)=>{match byte{Option.None()=>{return 11},Option.Some(value)=>{if value!=49u8+u8(at){return 12}}}}}
                at+=1
            }
            var ends:i32=0
            while ends<2 {match buffered_io.read_byte(&mut reader){.err(_)=>{return 13},.ok(byte)=>{match byte{Option.None()=>{},Option.Some(_)=>{return 14}}}};ends+=1}
        }
    }
    return 0
}
