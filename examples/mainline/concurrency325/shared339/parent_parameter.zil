fn worker(input:&i32)->i32 { return *input+2 }
fn call(input:&i32)->i32 {
    let body=()->i32=>*input
    let result=thread_scope(worker,input,body)
    return result.1+result.2
}
fn main()->i32 {
    var value=9
    let parent=(input:&i32)->i32=>call(input)
    let result=parent(&value)
    value=13
    return result-20
}
