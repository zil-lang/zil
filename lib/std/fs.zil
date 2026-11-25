// Linux x86_64 POSIX filesystem convenience helpers built on std.io.
import std.io
import std.store

extern "C" fn access(path:&u8,mode:i32)->i32
contract {provides(noescape(path));effects(ffi);};
extern "C" fn unlink(path:&u8)->i32
contract {provides(noescape(path));effects(ffi);};
extern "C" fn stat(path:&u8,buf:&mut [u64;18])->i32
contract {provides(noescape(path),noescape(buf));effects(ffi);};
extern "C" fn mkdirat(dirfd:i32,path:&u8,mode:u32)->i32
contract {provides(noescape(path));effects(ffi);};
extern "C" fn rmdir(path:&u8)->i32
contract {provides(noescape(path));effects(ffi);};
extern "C" fn renameat(olddirfd:i32,oldpath:&u8,newdirfd:i32,newpath:&u8)->i32
contract {provides(noescape(oldpath),noescape(newpath));effects(ffi);};
extern "C" fn opendir(path:&u8)->c_handle
contract {provides(noescape(path));effects(ffi);};
extern "C" fn readdir(dirp:c_handle)->c_handle;
extern "C" fn closedir(dirp:c_handle)->i32;
extern "C" fn memcpy(dst:&mut u8,src:c_handle,n:usize)->c_handle
contract {provides(noescape(dst));effects(ffi);};
extern "C" fn zil_glibc_errno_exchange(next:i32)->i32;

pub enum RemoveError { InvalidPath, Allocation, InvalidResult, Remove }
pub struct Metadata { pub size:i64; pub is_dir:bool; pub is_file:bool }

resource DirStream { ptr:c_handle }
impl Drop for DirStream { fn drop(self:Self) { unsafe{closedir(.ptr)} } }

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

fn io_c_path(path:&[u8])->io.Error!ByteBuffer contract {result(create)} {
    if path.len()==0 || path.len()==9223372036854775807 {return io.Error.InvalidPath}
    var at:i64=0
    while at<path.len() {
        if path[at]==0u8 {return io.Error.InvalidPath}
        at+=1
    }
    let encoded=bytes_new()
    if !bytes_reserve(encoded,path.len()+1) {return io.Error.Allocation}
    at=0
    while at<path.len() {
        if !bytes_push(encoded,path[at]) {return io.Error.InvalidResult}
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

fn ensure_scratch_len(scratch:ByteBuffer,target:i64)->bool {
    let cur=bytes_len(scratch)
    if cur<target {
        if !bytes_reserve(scratch,target) {return false}
        var at=cur
        while at<target {
            if !bytes_push(scratch,0u8) {return false}
            at+=1
        }
    }
    return true
}

// Extracts d_name from a copied Linux x86_64 struct dirent (d_name at offset 19).
// Returns None for "." and "..", or Some(owned name) otherwise.
fn extract_dirent(scratch:ByteBuffer,reclen:i64)->io.Error!Option<Managed<ByteBuffer>> {
    let view=bytes_view(scratch)
    var nul:i64=19
    while nul<reclen && view[nul]!=0u8 {nul+=1}
    if nul>=reclen {return io.Error.InvalidResult}
    let name_len=nul-19
    if name_len==1 && view[19]==46u8 {return Option::<Managed<ByteBuffer>>.None()}
    if name_len==2 && view[19]==46u8 && view[20]==46u8 {return Option::<Managed<ByteBuffer>>.None()}
    let item=bytes_new()
    if !bytes_reserve(item,name_len) {return io.Error.Allocation}
    var at:i64=0
    while at<name_len {
        if !bytes_push(item,view[19+at]) {return io.Error.InvalidResult}
        at+=1
    }
    return Option::<Managed<ByteBuffer>>.Some(give item)
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

// Queries Linux x86_64 POSIX stat(2) metadata (144-byte struct stat:
// st_mode u32 at offset 24 -> raw[3] low 32 bits; st_size i64 at offset 48 -> raw[6]).
pub fn metadata(path:&[u8])->io.Error!Metadata {
    let name=io_c_path(path)?
    var raw=[0u64,0u64,0u64,0u64,0u64,0u64,
             0u64,0u64,0u64,0u64,0u64,0u64,
             0u64,0u64,0u64,0u64,0u64,0u64]
    var status:i32=-1
    unsafe{status=stat(&bytes_view(name)[0],&mut raw)}
    if status!=0 {return io.Error.Read}
    let mode=u32(raw[3]%4294967296u64)
    // S_IFMT = 0o170000 (bits 12..15); S_IFDIR = 0o040000 (4); S_IFREG = 0o100000 (8).
    let kind=(mode/4096u32)%16u32
    if raw[6]>9223372036854775807u64 {return io.Error.InvalidResult}
    let size=i64(raw[6])
    return Metadata{size:size,is_dir:kind==4u32,is_file:kind==8u32}
}

// Returns true iff path exists and refers to a directory.
pub fn is_dir(path:&[u8])->bool {
    match metadata(path) {
        .err(_)=>{return false},
        .ok(meta)=>{return meta.is_dir}
    }
}

// Returns true iff path exists and refers to a regular file.
pub fn is_file(path:&[u8])->bool {
    match metadata(path) {
        .err(_)=>{return false},
        .ok(meta)=>{return meta.is_file}
    }
}

// Returns the size in bytes reported by stat(2) for path.
pub fn file_size(path:&[u8])->io.Error!i64 {
    let meta=metadata(path)?
    return meta.size
}

// Creates a single directory with mode 0o755 via POSIX mkdirat(AT_FDCWD, ...).
pub fn mkdir(path:&[u8])->io.Error!void {
    let name=io_c_path(path)?
    var status:i32=-1
    // AT_FDCWD = -100, mode 0o755 = 493 on Linux x86_64.
    unsafe{status=mkdirat(-100,&bytes_view(name)[0],493u32)}
    if status!=0 {return io.Error.Write}
    return ()
}

// Creates a directory and all missing parent components iteratively.
// Succeeds if the target directory already exists; fails if a component is a non-directory.
pub fn mkdir_all(path:&[u8])->io.Error!void {
    if path.len()==0 || path.len()==9223372036854775807 {return io.Error.InvalidPath}
    var check:i64=0
    while check<path.len() {
        if path[check]==0u8 {return io.Error.InvalidPath}
        check+=1
    }
    var at:i64=1
    while at<=path.len() {
        if at==path.len() || path[at]==47u8 {
            if path[at-1]!=47u8 {
                let prefix=&path[0..at]
                if !is_dir(prefix) {
                    let name=io_c_path(prefix)?
                    var status:i32=-1
                    var err:i32=0
                    unsafe{
                        zil_glibc_errno_exchange(0)
                        status=mkdirat(-100,&bytes_view(name)[0],493u32)
                        err=zil_glibc_errno_exchange(0)
                    }
                    if status!=0 {
                        // EEXIST = 17 on Linux x86_64: allow concurrent directory creation.
                        if !(err==17 && is_dir(prefix)) {return io.Error.Write}
                    }
                }
            }
        }
        at+=1
    }
    if !is_dir(path) {return io.Error.Write}
    return ()
}

// Removes an empty directory via POSIX rmdir(2).
pub fn remove_dir(path:&[u8])->io.Error!void {
    let name=io_c_path(path)?
    var status:i32=-1
    unsafe{status=rmdir(&bytes_view(name)[0])}
    if status!=0 {return io.Error.Write}
    return ()
}

// Renames a file or directory via POSIX renameat(AT_FDCWD, ..., AT_FDCWD, ...).
pub fn rename(from:&[u8],to:&[u8])->io.Error!void {
    let c_from=io_c_path(from)?
    let c_to=io_c_path(to)?
    var status:i32=-1
    unsafe{status=renameat(-100,&bytes_view(c_from)[0],-100,&bytes_view(c_to)[0])}
    if status!=0 {return io.Error.Write}
    return ()
}

// Reads directory entry names (skipping "." and "..") into an owned Store.
// The POSIX DIR stream is always closed by DirStream's Drop on every exit path.
pub fn read_dir(path:&[u8])->io.Error!Store<Managed<ByteBuffer>> contract {result(create)} {
    let name=io_c_path(path)?
    var ptr=c_null()
    unsafe{ptr=opendir(&bytes_view(name)[0])}
    if ptr==c_null() {return io.Error.Open}
    let dir=DirStream{ptr:ptr}
    let entries:Store<Managed<ByteBuffer>>=store_new()
    let scratch=bytes_new()
    loop {
        var entry=c_null()
        var err:i32=0
        unsafe{
            zil_glibc_errno_exchange(0)
            entry=readdir(dir.ptr)
            err=zil_glibc_errno_exchange(0)
        }
        if entry==c_null() {
            if err!=0 {return io.Error.Read}
            break
        }
        // Linux x86_64 struct dirent: d_ino(8) + d_off(8) + d_reclen(2) + d_type(1) = 19 bytes.
        var hdr=[0u8,0u8,0u8,0u8,0u8,0u8,0u8,0u8,0u8,0u8,
                 0u8,0u8,0u8,0u8,0u8,0u8,0u8,0u8,0u8]
        unsafe{memcpy(&mut hdr[0],entry,19usize)}
        let reclen:i64=i64(hdr[16])+i64(hdr[17])*256
        if reclen<=19 {return io.Error.InvalidResult}
        if !ensure_scratch_len(scratch,reclen) {return io.Error.Allocation}
        copy_from_c(scratch,entry,usize(reclen))
        match extract_dirent(scratch,reclen)? {
            Option.None()=>{},
            Option.Some(item)=>{
                match store.push_growing(entries,item) {
                    .err(_)=>{return io.Error.Allocation},
                    .ok(_)=>{}
                }
            }
        }
    }
    return give entries
}
