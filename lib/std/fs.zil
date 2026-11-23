// Linux x86_64 POSIX filesystem convenience helpers built on std.io.
import std.io

extern "C" fn access(path:&u8,mode:i32)->i32
contract {provides(noescape(path));effects(ffi);};
extern "C" fn unlink(path:&u8)->i32
contract {provides(noescape(path));effects(ffi);};

pub enum RemoveError { InvalidPath, Allocation, InvalidResult, Remove }

fn c_path(path:&[u8])->RemoveError!ByteBuffer contract {result(create)} {
    if path.len()==0 || path.len()==9223372036854775807 {return RemoveError.InvalidPath}
    var at:i64=0
    while at<path.len() {
        if path[at]==0u8 {return RemoveError.InvalidPath}
        at+=1
    }
    let encoded=bytes_new()
    if !bytes_reserve(encoded,path.len()+1) {return RemoveError.Allocation}
    at=0
    while at<path.len() {
        if !bytes_push(encoded,path[at]) {return RemoveError.InvalidResult}
        at+=1
    }
    if !bytes_push(encoded,0u8) {return RemoveError.InvalidResult}
    return give encoded
}

// Opens path for reading, reads all bytes into an owned ByteBuffer, and closes
// the descriptor on both success and error paths via File's Drop.
pub fn read_to_bytes(path:&[u8])->io.Error!ByteBuffer contract {result(create)} {
    let file=io.open_read(path)?
    return io.read_to_end(&file,9223372036854775807)
}

// Creates or truncates path (mode 0644), writes all bytes, and closes cleanly.
pub fn write_bytes(path:&[u8],data:&[u8])->io.Error!void {
    let file=io.create_write(path,420)?
    return io.write_all(&file,data)
}

// Returns true iff path exists (F_OK = 0); returns false on empty path or embedded NUL.
pub fn exists(path:&[u8])->bool {
    match c_path(path) {
        .err(_)=>{return false},
        .ok(name)=>{
            var status:i32=-1
            unsafe{status=access(&bytes_view(name)[0],0)}
            return status==0
        }
    }
}

// Removes a file via POSIX unlink(2).
pub fn remove_file(path:&[u8])->RemoveError!void {
    let name=c_path(path)?
    var status:i32=-1
    unsafe{status=unlink(&bytes_view(name)[0])}
    if status!=0 {return RemoveError.Remove}
    return ()
}
