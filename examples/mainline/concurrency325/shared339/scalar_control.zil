fn read(input:&i32)->i32 { return *input+2 }
fn worker(input:i32)->i32 { return input+2 }
fn main()->i32 {
    var value=9
    let input=&value
    let body=()->i32=>*input
    let(created,child,parent)=thread_scope(worker,value,body)
    if !created { return 2 }
    value=13
    if child!=11 || parent!=9 || value!=13 { return 1 }
    return 0
}
