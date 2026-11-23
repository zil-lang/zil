// Linux x86_64 POSIX process environment helpers.
import std.io

extern "C" fn getenv(name:&u8)->c_handle
contract {provides(noescape(name));effects(ffi);};
extern "C" fn strlen(s:c_handle)->usize;
extern "C" fn memcpy(dst:&mut u8,src:c_handle,n:usize)->c_handle
contract {provides(noescape(dst));effects(ffi);};
extern "C" fn getcwd(buf:&mut u8,size:usize)->c_handle
contract {provides(noescape(buf));effects(ffi);};
extern "C" fn _exit(code:i32);
extern "C" fn zil_glibc_errno_exchange(next:i32)->i32;

fn c_name(name:&[u8])->io.Error!ByteBuffer contract {result(create)} {
    if name.len()==0 || name.len()==9223372036854775807 {return io.Error.InvalidPath}
    var at:i64=0
    while at<name.len() {
        if name[at]==0u8 {return io.Error.InvalidPath}
        at+=1
    }
    let encoded=bytes_new()
    if !bytes_reserve(encoded,name.len()+1) {return io.Error.Allocation}
    at=0
    while at<name.len() {
        if !bytes_push(encoded,name[at]) {return io.Error.InvalidResult}
        at+=1
    }
    if !bytes_push(encoded,0u8) {return io.Error.InvalidResult}
    return give encoded
}

fn copy_slice_from_c(dst:&mut [u8],src:c_handle,n:usize) {
    unsafe{memcpy(&mut dst[0],src,n)}
}

fn copy_from_c(dst:ByteBuffer,src:c_handle,n:usize) {
    copy_slice_from_c(bytes_view_mut(dst),src,n)
}

fn getcwd_slice(dst:&mut [u8],size:usize)->bool {
    var ptr=c_null()
    unsafe{ptr=getcwd(&mut dst[0],size)}
    return ptr!=c_null()
}

fn getcwd_into(dst:ByteBuffer,size:usize)->bool {
    return getcwd_slice(bytes_view_mut(dst),size)
}

// Looks up an environment variable by name, returning an owned ByteBuffer copy
// if present, or Option.None() if unset, empty name, or containing embedded NUL.
pub fn get(name:&[u8])->Option<Managed<ByteBuffer>> {
    match c_name(name) {
        .err(_)=>{return Option::<Managed<ByteBuffer>>.None()},
        .ok(encoded)=>{
            var ptr=c_null()
            unsafe{ptr=getenv(&bytes_view(encoded)[0])}
            if ptr==c_null() {return Option::<Managed<ByteBuffer>>.None()}
            var n:usize=0usize
            unsafe{n=strlen(ptr)}
            if u64(n)>9223372036854775807u64 {return Option::<Managed<ByteBuffer>>.None()}
            let length=i64(n)
            let out=bytes_new()
            if !bytes_reserve(out,length) {return Option::<Managed<ByteBuffer>>.None()}
            var at:i64=0
            while at<length {
                if !bytes_push(out,0u8) {return Option::<Managed<ByteBuffer>>.None()}
                at+=1
            }
            if length>0 {
                copy_from_c(out,ptr,n)
            }
            return Option::<Managed<ByteBuffer>>.Some(give out)
        }
    }
}

fn find_nul(view:&[u8])->i64 {
    var nul:i64=0
    while nul<view.len() && view[nul]!=0u8 {nul+=1}
    return nul
}

// Returns the current working directory as an owned ByteBuffer (without NUL).
pub fn current_dir()->io.Error!ByteBuffer contract {result(create)} {
    var size:i64=256
    let max_size:i64=1048576
    loop {
        let buf=bytes_new()
        if !bytes_reserve(buf,size) {return io.Error.Allocation}
        var at:i64=0
        while at<size {
            if !bytes_push(buf,0u8) {return io.Error.InvalidResult}
            at+=1
        }
        var err:i32=0
        unsafe{zil_glibc_errno_exchange(0)}
        let ok=getcwd_into(buf,usize(size))
        unsafe{err=zil_glibc_errno_exchange(0)}
        if ok {
            let nul=find_nul(bytes_view(buf))
            if !bytes_truncate(buf,nul) {return io.Error.InvalidResult}
            return give buf
        }
        // ERANGE (34 on Linux x86_64): grow buffer and retry.
        if err==34 && size<=max_size/2 {
            size*=2
        } else {
            return io.Error.Read
        }
    }
}

// Terminates the current process immediately with the given exit code.
pub fn exit(code:i32) {
    unsafe{_exit(code)}
}
