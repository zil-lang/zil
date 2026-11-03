fn worker(value:i32)->i32 { return value+2 }
fn main()->i32 {
    var count=0
    let body=()->i32=>{count+=1;return count}
    let(created,child,parent)=thread_scope(worker,5,body)
    if !created { return 2 }
    if child!=7 || parent!=1 || count!=1 { return 3 }
    return 0
}
