extern "C" fn registration_register(cb:extern "C" fn(c_handle,i32)->i64,ctx:c_handle)->c_handle contract{provides(static_callbacks(),scoped_callback_register());effects(ffi);};
extern "C" fn registration_close_backend(token:c_handle) contract{provides(static_callbacks(),scoped_callback_close());effects(ffi);};
extern "C" fn registration_clobber()->i32 contract{effects(ffi);};
extern "C" fn registration_fire(value:i32)->i32 contract{effects(ffi);};
fn install<F:FnMut(i32)->i32 + ClosedEnvironment>(f:F)->Registration<F> contract{result(create);} {
    unsafe { return c_register(registration_register,registration_close_backend,f) }
}
fn main()->i32 {
    let seed=37;
    let f=move(value:i32)->i32=>value+seed;
    let owner=install(f);
    if !registration_valid(owner) { return 3 }
    unsafe {
        if registration_clobber()!=0 { return 4 }
        if registration_fire(5)!=42 { return 5 }
    }
    registration_close(give owner);
    return 0
}
