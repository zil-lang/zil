// Fixed memory buffered POSIX I/O. Failed flush retains the unwritten suffix;
// retry never writes an already acknowledged prefix. Drop closes the file and
// abandons pending bytes. Call finish to observe write errors before cleanup.
import std.io
pub package Reader { manages file:io.File; block:[u8;64]; begin:i64; end:i64; eof:bool }
pub package Writer { manages file:io.File; block:[u8;64]; used:i64 }
pub fn reader(give file:io.File)->Reader {return Reader{file:give file,block:io.empty_buffer(),begin:0,end:0,eof:false}}
pub fn writer(give file:io.File)->Writer {return Writer{file:give file,block:io.empty_buffer(),used:0}}
pub fn pending(value:&Writer)->i64 {return value.used}
pub fn buffered(value:&Reader)->i64 {return value.end-value.begin}
pub fn read_byte(value:&mut Reader)->io.Error!Option<u8> {
    if value.begin==value.end {
        if value.eof {return Option::<u8>.None()}
        let result=io.read_chunk(&value.file,&mut value.block)?
        value.begin=0
        value.end=result.count
        value.eof=result.eof
        if result.eof {return Option::<u8>.None()}
    }
    let byte=value.block[value.begin]
    value.begin+=1
    return Option::<u8>.Some(byte)
}
// A positive count may be smaller than the output window. A zero-length window
// does not touch the file; zero with a nonempty window means EOF.
pub fn read(value:&mut Reader,output:&mut [u8])->io.Error!i64 {
    var at:i64=0
    while at<output.len() {
        match read_byte(value)? {
            Option.None()=>{break},
            Option.Some(byte)=>{output[at]=byte;at+=1}
        }
        if value.begin==value.end {break}
    }
    return at
}
pub fn flush(value:&mut Writer)->io.Error!void {
    while value.used>0 {
        let count=io.write_chunk(&value.file,&value.block,value.used)?
        value.used-=count
        var at:i64=0
        while at<value.used {value.block[at]=value.block[at+count];at+=1}
    }
    return ()
}
pub fn write_byte(value:&mut Writer,byte:u8)->io.Error!void {
    if value.used==64 {flush(value)?}
    value.block[value.used]=byte
    value.used+=1
    return ()
}
// A partial successful count tells the caller exactly how much input was
// accepted. An error occurs only before accepting any new input. The caller may
// retry the remaining suffix after handling a flush error.
pub fn write(value:&mut Writer,source:&[u8])->io.Error!i64 {
    if source.len()==0 {return i64(0)}
    if value.used==64 {flush(value)?}
    var count=64-value.used
    if count>source.len() {count=source.len()}
    var at:i64=0
    while at<count {value.block[value.used]=source[at];value.used+=1;at+=1}
    return count
}
// Failure returns the actual writer, with its descriptor and unwritten bytes.
// No hidden final write happens while destroying a writer.
pub package Finished { manages file:io.File }
pub fn into_file(value:Finished)->io.File contract {result(give)} {return give value.file}
pub fn finish(value:Writer)->Writer!Finished {
    var owned=value
    match flush(&mut owned) {.err(_)=>{return owned},.ok(_)=>{return Finished{file:give owned.file}}}
}
