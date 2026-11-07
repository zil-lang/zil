fn scalar(value:i32)->i32 { return value+2 }
fn worker(value:i32)->i32 {
    let body=()->i32=>3
    let result=thread_scope(scalar,value,body)
    return result.1+result.2
}
fn main()->i32 {
    let parent=()->i32=>{
        let body=()->i32=>7
        let result=thread_scope(worker,5,body)
        return result.1+result.2
    }
    return parent()
}
