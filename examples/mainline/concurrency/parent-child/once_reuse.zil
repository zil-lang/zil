resource Mark { code:i32 }
impl Drop for Mark { fn drop(self:Self) {} }
fn worker(value:i32)->i32 { return value+2 }
fn main()->i32 {
    let mark=Mark{code:9}
    let body=move()->i32=>mark.code
    let parent=move()->i32=>{
        let result=thread_scope(worker,5,body)
        return result.1+result.2
    }
    let first=parent()
    return first+parent()
}
