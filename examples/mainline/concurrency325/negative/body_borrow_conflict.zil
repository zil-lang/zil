fn worker(value:i32)->i32 { return value+2 }
fn main()->i32 {
    var count=0
    let view=&count
    let body=()->i32=>{count+=1;return count}
    let result=thread_scope(worker,5,body)
    return *view+result.2
}
