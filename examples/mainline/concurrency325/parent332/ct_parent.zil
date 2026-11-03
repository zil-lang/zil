fn worker(value:i32)->i32 { return value+2 }
fn main()->i32 {
    let parent=()->i32=>{let result=#(thread_scope(worker,5,()->i32=>7));return 0}
    return parent()
}
