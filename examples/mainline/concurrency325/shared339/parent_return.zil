fn same(input:&i32)->&i32 { return input }
fn worker(input:&i32)->i32 { return *input+2 }
fn main()->i32 {
    var value=9
    let input=same(&value)
    let body=()->i32=>*input
    let(created,child,parent)=thread_scope(worker,input,body)
    if !created { return 2 }
    value=13
    return child+parent+value-33
}
