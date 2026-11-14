extern "C" fn thread_register(cb:extern "C" fn(c_handle,i32)->i64,ctx:c_handle)->c_handle contract{provides(static_callbacks(),thread_async_callback_register());effects(ffi);};
extern "C" fn thread_request(token:c_handle) contract{provides(static_callbacks(),thread_async_callback_request_close());effects(ffi);};
extern "C" fn thread_drain(token:c_handle)->i32 contract{provides(static_callbacks(),thread_async_callback_poll_drain());effects(ffi);};
extern "C" fn thread_race()->i32 contract{effects(ffi);};
extern "C" fn thread_release()->i32 contract{effects(ffi);};
extern "C" fn thread_complete(failed:i32)->i32 contract{effects(ffi);};
fn install<F:FnMut(i32)->i32 + ClosedEnvironment + Transfer + ThreadCallable>(f:F)->AsyncRegistration<F> contract{result(create);} {
    unsafe {return c_register_async(thread_register,thread_request,thread_drain,f)}
}
fn main()->i32 { return 0 }
