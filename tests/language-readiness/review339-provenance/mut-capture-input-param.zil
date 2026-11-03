fn worker(input:&i32)->i32{return *input}
fn main()->i32{
    var value=9
    var parent=(input:&i32)->i32=>{
        let body=()->i32=>{value=12;return 0}
        let result=thread_scope(worker,input,body)
        return result.1
    }
    return parent(&value)-9
}
