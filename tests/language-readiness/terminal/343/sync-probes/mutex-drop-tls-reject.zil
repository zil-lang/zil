extern "C" fn local_release(code:i32) contract{effects(ffi);};
resource Affine {code:i32}
impl Drop for Affine {fn drop(self:Self) {unsafe {local_release(.code)}}}
fn worker(cell:Mutex<Affine>)->i32 {let guard=mutex_lock(cell);mutex_unlock(guard);return 0}
fn main()->MutexCreateError!i32 {
    let value=Affine{code:7}
    let cell=mutex_create(give value)?
    let (created,child,parent)=thread_scope(worker,cell,()->i32=>0)
    if !created {return 1} return child+parent
}
