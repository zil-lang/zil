// Linux x86_64 POSIX byte I/O. The foreign calls are synchronous, do not
// retain buffer addresses, reenter ZIL, or unwind. A File owns exactly one fd.
extern "C" fn read(fd:i32,buffer:&mut [u8;64],count:i64)->i64
contract {provides(noescape(buffer));effects(ffi);};
extern "C" fn write(fd:i32,buffer:&[u8;64],count:i64)->i64
contract {provides(noescape(buffer));effects(ffi);};
extern "C" fn dup(fd:i32)->i32;
extern "C" fn close(fd:i32)->i32;
extern "C" fn open(path:&u8,flags:i32)->i32
contract {provides(noescape(path));effects(ffi);};
extern "C" fn creat(path:&u8,mode:i32)->i32
contract {provides(noescape(path));effects(ffi);};

pub enum Error { Duplicate, Open, InvalidPath, Capacity, Allocation, Read, Write, NoProgress, InvalidResult }
pub struct ReadResult { pub count:i64; pub eof:bool }
pub resource File { fd:i32 }
impl Drop for File {fn drop(self:Self){unsafe{close(.fd)}}}

// The returned duplicate has independent ownership. Dropping it closes only
// that descriptor; the caller still owns the original fd.
pub fn duplicate(fd:i32)->Error!File contract {result(create)} {
    var owned:i32=-1
    unsafe{owned=dup(fd)}
    if owned<0 {return Error.Duplicate}
    return File{fd:owned}
}
pub fn stdin()->Error!File contract {result(create)} {return duplicate(0)}
pub fn stdout()->Error!File contract {result(create)} {return duplicate(1)}

// Path bytes must be nonempty and contain no NUL. The private owned buffer
// supplies exactly one terminator and stays live throughout the C call.
fn c_path(path:&[u8])->Error!ByteBuffer contract {result(create)} {
    if path.len()==0 || path.len()==9223372036854775807 {return Error.InvalidPath}
    var at:i64=0
    while at<path.len() {
        if path[at]==0u8 {return Error.InvalidPath}
        at+=1
    }
    let encoded=bytes_new()
    if !bytes_reserve(encoded,path.len()+1) {return Error.Allocation}
    at=0
    while at<path.len() {
        if !bytes_push(encoded,path[at]) {return Error.InvalidResult}
        at+=1
    }
    if !bytes_push(encoded,0u8) {return Error.InvalidResult}
    return give encoded
}

// open(O_RDONLY) and creat(mode) use the Linux x86_64 libc ABI.
pub fn open_read(path:&[u8])->Error!File contract {result(create)} {
    let name=c_path(path)?
    var fd:i32=-1
    unsafe{fd=open(&bytes_view(name)[0],0)}
    if fd<0 {return Error.Open}
    return File{fd:fd}
}
pub fn create_write(path:&[u8],mode:i32)->Error!File contract {result(create)} {
    let name=c_path(path)?
    var fd:i32=-1
    unsafe{fd=creat(&bytes_view(name)[0],mode)}
    if fd<0 {return Error.Open}
    return File{fd:fd}
}

pub fn empty_buffer()->[u8;64] {
    return [0u8,0u8,0u8,0u8,0u8,0u8,0u8,0u8,
             0u8,0u8,0u8,0u8,0u8,0u8,0u8,0u8,
             0u8,0u8,0u8,0u8,0u8,0u8,0u8,0u8,
             0u8,0u8,0u8,0u8,0u8,0u8,0u8,0u8,
             0u8,0u8,0u8,0u8,0u8,0u8,0u8,0u8,
             0u8,0u8,0u8,0u8,0u8,0u8,0u8,0u8,
             0u8,0u8,0u8,0u8,0u8,0u8,0u8,0u8,
             0u8,0u8,0u8,0u8,0u8,0u8,0u8,0u8]
}

// A short read is a successful count, zero is EOF, and -1 is an I/O error.
// Bytes after count retain their old initialized values.
pub fn read_chunk(file:&File,buffer:&mut [u8;64])->Error!ReadResult {
    var count:i64=0
    unsafe{count=read(file.fd,buffer,64)}
    if count<0 {return Error.Read}
    if count>64 {return Error.InvalidResult}
    return ReadResult{count:count,eof:count==0}
}

// Read to EOF with an explicit byte limit. A capacity or I/O error drops the
// partial buffer; callers receive bytes only after a complete successful read.
// Errors preserve the current file cursor; retrying does not restart the read.
pub fn read_to_end(file:&File,byte_limit:i64)->Error!ByteBuffer contract {result(create)} {
    if byte_limit<0 {return Error.Capacity}
    let data=bytes_new()
    var block=empty_buffer()
    loop {
        let result=read_chunk(file,&mut block)?
        if result.eof {break}
        let length=bytes_len(data)
        if length>byte_limit-result.count {return Error.Capacity}
        let needed=length+result.count
        if needed>bytes_capacity(data) {
            var next=bytes_capacity(data)
            if next<64 {next=64}
            while next<needed {
                if next>byte_limit/2 {next=byte_limit;break}
                next*=2
            }
            if next>byte_limit {next=byte_limit}
            if !bytes_reserve(data,next) {return Error.Allocation}
        }
        var at:i64=0
        while at<result.count {
            if !bytes_push(data,block[at]) {return Error.InvalidResult}
            at+=1
        }
    }
    return give data
}

// Complete writes of arbitrary borrowed bytes using one fixed C ABI buffer.
// A short write is retried with the unwritten prefix; source is unchanged.
// On error, a prefix may already have reached the fd.
pub fn write_all(file:&File,source:&[u8])->Error!void {
    var block=empty_buffer()
    var at:i64=0
    while at<source.len() {
        var length=source.len()-at
        if length>64 {length=64}
        var copied:i64=0
        while copied<length {block[copied]=source[at+copied];copied+=1}
        var remaining=length
        while remaining>0 {
            var written:i64=0
            unsafe{written=write(file.fd,&block,remaining)}
            if written<0 {return Error.Write}
            if written==0 {return Error.NoProgress}
            if written>remaining {return Error.InvalidResult}
            remaining-=written
            var index:i64=0
            while index<remaining {block[index]=block[index+written];index+=1}
        }
        at+=length
    }
    return ()
}
