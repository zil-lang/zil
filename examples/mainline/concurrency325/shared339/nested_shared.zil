fn worker(input:&i32)->i32 { return *input+2 }
fn main()->i32 {
    var value=9
    let input=&value
    let body=()->i32=>{
        let result=thread_scope(worker,input,()->i32=>*input)
        return result.1+result.2
    }
    let result=thread_scope(worker,input,body)
    value=13
    return result.1+result.2-31
}
