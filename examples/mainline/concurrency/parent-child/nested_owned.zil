extern "C" fn nested_mode()->i32;
extern "C" fn nested_body(which:i32)->void;
extern "C" fn nested_drop(code:i32)->void;
extern "C" fn nested_inner(created:i32,child:i32,parent:i32)->void;
extern "C" fn nested_after(created:i32,child:i32,parent:i32)->void;
fn mode()->i32 { unsafe { return nested_mode() } }
fn body(which:i32) { unsafe { nested_body(which) } }
fn dropped(code:i32) { unsafe { nested_drop(code) } }
fn inner_result(created:i32,child:i32,parent:i32) { unsafe { nested_inner(created,child,parent) } }
fn after(created:i32,child:i32,parent:i32) { unsafe { nested_after(created,child,parent) } }
resource Mark { code:i32 }
impl Drop for Mark { fn drop(self:Self) { dropped(.code) } }
fn worker(value:i32)->i32 { return value+2 }
fn main()->i32 {
    let mode=mode()
    let mark=Mark{code:79}
    let body=move()->i32=>{
        let active=mark
        body(1)
        let inner_mark=Mark{code:73}
        let inner=move()->i32=>{let active_inner=inner_mark;body(2);return active_inner.code-56}
        let(created,child,parent)=thread_scope(worker,11,inner)
        if created { inner_result(1,child,parent) } else { inner_result(0,child,parent) }
        if !created { return -100 }
        return child+parent+active.code-79
    }
    let outer=move()->i32=>{
        let(created,child,parent)=thread_scope(worker,5,body)
        if created { after(1,child,parent) } else { after(0,child,parent) }
        if !created { return 2 }
        if parent==-100 { return 3 }
        if child!=7 || parent!=30 { return 4 }
        return 0
    }
    return outer()+mode-mode
}
