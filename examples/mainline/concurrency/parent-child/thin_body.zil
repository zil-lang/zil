fn worker(value:i32)->i32 { return value+2 }
fn thin()->i32 { return 7 }
fn main()->i32 {
    let parent=()->i32=>{let result=thread_scope(worker,5,thin);return result.1+result.2}
    return parent()
}
