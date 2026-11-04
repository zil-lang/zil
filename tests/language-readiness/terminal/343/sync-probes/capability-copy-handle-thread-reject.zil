struct Opaque {handle:c_handle}
fn worker(value:Opaque)->i32 {if value.handle==c_null() {return 0}return 1}
fn main()->i32 {
    let (created,child,parent)=thread_scope(worker,Opaque{handle:c_null()},()->i32=>0)
    if !created {return 2}return child+parent
}
