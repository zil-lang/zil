fn worker(value:i32)->i32 { return value+2 }
fn main()->i32 {
    let outer=()->i32=>{
        let body=()->i32=>{
            let inner=()->i32=>17
            let(created,child,parent)=thread_scope(worker,11,inner)
            if !created { return -100 }
            return child+parent
        }
        let(created,child,parent)=thread_scope(worker,5,body)
        if !created { return -100 }
        return child+parent
    }
    if outer()!=37 { return 1 }
    return 0
}
