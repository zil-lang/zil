extern "C" fn registration_register(cb:extern "C" fn(c_handle,i32)->i64,ctx:c_handle)->c_handle contract{provides(static_callbacks(),scoped_callback_register());effects(ffi);};
extern "C" fn registration_close_backend(token:c_handle) contract{provides(static_callbacks(),scoped_callback_close());effects(ffi);};
fn worker(seed:i32)->i32{task_yield();return seed}
fn install<F:FnMut(i32)->i32 + ClosedEnvironment>(f:F)->Registration<F> contract{result(create);}{unsafe{return c_register(registration_register,registration_close_backend,f)}}
fn main()->i32{var task=task_start(worker,37);let f=move(value:i32)->i32=>{if task_valid(task){return value}return 0};unsafe{let owner=c_register(registration_register,registration_close_backend,f);registration_close(give owner);}return 0}
