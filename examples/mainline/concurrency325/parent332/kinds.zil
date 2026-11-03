resource Mark { code:i32 }
impl Drop for Mark { fn drop(self:Self) {} }
fn worker(value:i32)->i32 { return value+2 }
fn main()->i32 {
    let shared=(offset:i32)->i32=>{
        let body=()->i32=>offset
        let(created,child,parent)=thread_scope(worker,5,body)
        if !created { return -100 }
        return child+parent
    }
    if shared(3)!=10 || shared(4)!=11 { return 1 }
    var count=0
    var mutable=(step:i32)->i32=>{
        count+=step
        let body=()->i32=>1
        let(created,child,parent)=thread_scope(worker,5,body)
        if !created { return -100 }
        return count+child+parent
    }
    if mutable(2)!=10 || mutable(3)!=13 || count!=5 { return 2 }
    let mark=Mark{code:9}
    let once=move()->i32=>{
        let body=move()->i32=>mark.code
        let(created,child,parent)=thread_scope(worker,5,body)
        if !created { return -100 }
        return child+parent
    }
    if once()!=16 { return 3 }
    return 0
}
