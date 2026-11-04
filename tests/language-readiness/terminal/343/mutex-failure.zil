extern "C" fn sync_observe(code:i32);
fn observe(code:i32) { unsafe { sync_observe(code) } }
resource Mark { code:i32 }
impl Drop for Mark { fn drop(self:Self) { observe(.code) } }
fn main()->i32 {
    let mark=Mark{code:68}
    let(initialized,result)=mutex_scope(1,move(cell:&MutexI32)->i32=>{
        let owned=mark
        observe(66)
        return mutex_load(cell)+owned.code-69
    })
    if !initialized { return 2 }
    return result
}
