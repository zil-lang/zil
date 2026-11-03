fn worker(input:&i32)->i32{return *input}
fn main()->i32{
    let value=9
    let input=&value
    let result=thread_scope(worker,input,()->i32=>0)
    return result.1-9
}
