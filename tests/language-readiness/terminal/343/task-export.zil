pub fn worker(seed:i32)->i32 {
    task_yield()
    task_yield()
    return seed+2
}
extern "C" fn task_external()->i32 contract{effects(ffi);};
fn main()->i32 {unsafe{return task_external()}}
