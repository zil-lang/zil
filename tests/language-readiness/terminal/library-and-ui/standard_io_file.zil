import std.io

fn main()->io.Error!i32 {
    match io.open_read("") {
        .ok(_)=>{return 1},
        .err(_)=>{}
    }
    match io.open_read("FILEPATH-missing") {
        .ok(_)=>{return 2},
        .err(_)=>{}
    }
    if true {
        let output=io.create_write("FILEPATH",384)?
        io.write_all(&output,"猫\0Z")?
    }
    let input=io.open_read("FILEPATH")?
    var buffer=io.empty_buffer()
    let result=io.read_chunk(&input,&mut buffer)?
    if result.eof || result.count!=5 {return 3}
    if buffer[0]!=231u8 || buffer[1]!=140u8 || buffer[2]!=171u8 || buffer[3]!=0u8 || buffer[4]!=90u8 {return 4}
    let end=io.read_chunk(&input,&mut buffer)?
    if !end.eof || end.count!=0 {return 5}
    return 0
}
