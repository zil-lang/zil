import worker
extern "C" fn shared_mode()->i32;
extern "C" fn shared_body(input:&i32) contract { provides(noescape(input)); effects(ffi); };
extern "C" fn shared_drop(code:i32);
extern "C" fn shared_after(created:i32,child:i32,parent:i32);
fn mode()->i32 { unsafe { return shared_mode() } }
fn notify(input:&i32) { unsafe { shared_body(input) } }
fn dropped(code:i32) { unsafe { shared_drop(code) } }
fn after(created:i32,child:i32,parent:i32) { unsafe { shared_after(created,child,parent) } }
resource Mark { code:i32 }
impl Drop for Mark { fn drop(self:Self) { dropped(.code) } }
fn main()->i32 {
    let mode=mode()
    var value=9
    let input=&value
    let mark=Mark{code:68}
    let body=move()->i32=>{
        let active=mark
        notify(input)
        if mode==1 { return -7 }
        return *input+active.code-68
    }
    let(created,child,parent)=thread_scope(worker.compute,input,body)
    value=13
    if created { after(1,child,parent) } else { after(0,child,parent) }
    if !created { return 2 }
    if value!=13 || child!=11 { return 9 }
    if parent==-7 { return 1 }
    if parent!=9 { return 8 }
    return 0
}
