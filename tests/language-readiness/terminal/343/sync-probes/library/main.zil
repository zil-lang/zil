import sync_ops
fn worker_atomic(cell:&AtomicI32)->i32 {var i=0;while i<64 {sync_ops.advance_atomic(cell,1);i+=1}return 0}
fn worker_mutex(cell:&MutexI32)->i32 {var i=0;while i<64 {sync_ops.advance_mutex(cell,1);i+=1}return 0}
fn main()->i32 {
    let (aok,a)=atomic_scope(0,(cell:&AtomicI32)->i32=>{
        let (created,child,parent)=thread_scope(worker_atomic,cell,()->i32=>{
            var i=0;while i<64 {sync_ops.advance_atomic(cell,1);i+=1}return 0
        })
        if !created {return 2} return atomic_load(cell)-128+child+parent
    })
    let (mok,m)=mutex_scope(0,(cell:&MutexI32)->i32=>{
        let (created,child,parent)=thread_scope(worker_mutex,cell,()->i32=>{
            var i=0;while i<64 {sync_ops.advance_mutex(cell,1);i+=1}return 0
        })
        if !created {return 3} return mutex_load(cell)-128+child+parent
    })
    if !aok || !mok {return 1} return a+m
}
