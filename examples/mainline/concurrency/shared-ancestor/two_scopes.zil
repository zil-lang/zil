fn worker(input:&i32)->i32 { return *input+2 }
fn main()->i32 {
    var first=7
    var second=11
    let left=&first
    let first_result=thread_scope(worker,left,()->i32=>*left)
    first=9
    let right=&second
    let second_result=thread_scope(worker,right,()->i32=>*right)
    second=13
    return first_result.1+first_result.2+second_result.1+second_result.2-40
}
