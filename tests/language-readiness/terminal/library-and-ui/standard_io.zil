import std.io

fn main()->io.Error!i32 {
    let input=io.stdin()?
    let output=io.stdout()?
    var buffer=io.empty_buffer()
    loop {
        let result=io.read_chunk(&input,&mut buffer)?
        if result.eof {break}
        io.write_all(&output,&buffer[..result.count])?
    }
    return 0
}
