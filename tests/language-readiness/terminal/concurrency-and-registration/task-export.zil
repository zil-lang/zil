pub fn worker(seed:i32)->i32 {
    let small:u8=1
    let medium:u16=1
    let wide=i64(seed)
    let choose=true
    task_yield()
    if choose {task_yield()} else {task_yield()}
    return i32(wide)+i32(small)+i32(medium)
}
extern "C" fn task_external()->i32 contract{effects(ffi);};
fn main()->i32 {unsafe{return task_external()}}
