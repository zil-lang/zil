import std.io
import std.string
import std.int

// A bounded byte stream becomes one checked UTF-8 String before searching.
fn read_input()->io.Error!ByteBuffer contract {result(create)} {
    let input=io.stdin()?
    let raw=io.read_to_end(&input,4096)?
    return give raw
}

fn occurrences(text:&string.String)->string.Error!i64 {
    var count:i64=0
    var start:i64=0
    loop {
        let position=string.find_from(text,"猫",start)?
        if position<0 {break}
        count+=1
        start=position+1
    }
    return count
}

fn emit_count(value:i64)->io.Error!void {
    let output=io.stdout()?
    let decimal=int.format_i64(value)
    io.write_all(&output,&decimal.bytes[..decimal.len])?
    io.write_all(&output,"\n")?
    return ()
}

fn main()->i32 {
    match read_input() {
        .err(_)=>{return 3},
        .ok(raw)=>{
            match string.from_utf8(bytes_view(raw),4096) {
                .err(_)=>{return 2},
                .ok(text)=>{
                    match occurrences(&text) {
                        .err(_)=>{return 5},
                        .ok(count)=>{
                            match emit_count(count) {
                                .err(_)=>{return 6},
                                .ok(())=>{return 0}
                            }
                        }
                    }
                }
            }
        }
    }
}
