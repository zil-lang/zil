fn worker(input:&i32)->i32 { return *input }
fn main()->i32 {
    var value=9
    let input=&value
    var writer=()->i32=>{value=13;return value}
    let body=()->i32=>writer()
    let result=thread_scope(worker,input,body)
    return result.1
}
