fn make(value:i32)->impl Fn()->&i32 { return move()=>&value }
fn worker(input:&i32)->i32 { return *input }
fn main()->i32 {
    let owner=make(9)
    let input=owner()
    let result=thread_scope(worker,input,()->i32=>0)
    return result.1
}
