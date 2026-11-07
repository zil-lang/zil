fn worker(value:i32)->i32 { return value+2 }
fn main()->i32 {
    var value=9
    let parent=(input:&mut i32)->i32=>{
        let body=()->i32=>*input
        *input=4
        let result=thread_scope(worker,5,body)
        return result.1+result.2
    }
    return parent(&mut value)
}
