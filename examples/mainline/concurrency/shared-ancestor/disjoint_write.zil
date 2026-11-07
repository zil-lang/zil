fn read(input:&i32)->i32 { return *input+2 }
fn worker(input:&i32)->i32 { let alias=input;return read(alias) }
fn main()->i32 {
    var value=9
    var other=3
    let input=&value
    let body=()->i32=>{other=13;return *input+other}
    let(created,child,parent)=thread_scope(worker,input,body)
    if !created { return 2 }
    value=17
    if child!=11 || parent!=22 || value!=17 || other!=13 { return 1 }
    return 0
}
