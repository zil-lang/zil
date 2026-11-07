import observer
pub resource Mark { pub code:i32 }
impl Drop for Mark { fn drop(self:Self) { observer.dropped(.code) } }
pub package Work { manages mark:Mark; manages data:ByteBuffer; pub mode:i32; pub ready:bool }
pub fn prepare(mode:i32)->impl FnOnce()->i32 {
    let mark=Mark{code:68}
    let data=bytes_new()
    let ready=bytes_reserve(data,1)
    if ready { bytes_push(data,42u8) }
    let work=Work{mark:give mark,data:give data,mode:mode,ready:ready}
    observer.prepared()
    return move()->i32=>{
        let current=work
        observer.body()
        if !current.ready || bytes_len(current.data)!=1 { return -9 }
        let view=bytes_view(current.data)
        if view[0]!=42u8 { return -8 }
        if current.mode==1 { return -7 }
        return 17
    }
}
