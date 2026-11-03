fn worker(input:&i32)->i32 { return *input+2 }
fn main()->i32 {
    var value=9
    let input=&value
    let parent=()->i32=>{
        let body=()->i32=>*input
        let result=thread_scope(worker,input,body)
        return result.1+result.2
    }
    let result=parent()
    value=13
    return result-20
}
