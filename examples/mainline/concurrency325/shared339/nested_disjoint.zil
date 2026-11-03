fn worker(input:&i32)->i32 { return *input+2 }
fn main()->i32 {
    var value=9
    var other=3
    let input=&value
    var writer=()->i32=>{other=13;return other}
    let body=()->i32=>*input+writer()
    let(created,child,parent)=thread_scope(worker,input,body)
    if !created { return 2 }
    value=17
    if child!=11 || parent!=22 || other!=13 { return 1 }
    return 0
}
