fn worker(value:i32)->i32 { return value+2 }
fn main()->i32 {
    let value=9
    let outer=(input:&i32)->i32=>{
        let body=()->i32=>*input
        let(created,child,parent)=thread_scope(worker,5,body)
        if !created { return -100 }
        return child+parent
    }
    if outer(&value)!=16 { return 1 }
    return 0
}
