extern "C" fn registration_register(cb:extern "C" fn(c_handle,i32)->i64,ctx:c_handle)->c_handle contract{provides(static_callbacks(),scoped_callback_register());effects(ffi);};
extern "C" fn registration_close_backend(token:c_handle) contract{provides(static_callbacks(),scoped_callback_close());effects(ffi);};
extern "C" fn registration_clobber()->i32 contract{effects(ffi);};
extern "C" fn registration_fire(value:i32)->i32 contract{effects(ffi);};
extern "C" fn registration_note(code:i32) contract{effects(ffi);};
extern "C" fn registration_reenter()->i32 contract{effects(ffi);};
resource Mark { code:i32 }
impl Drop for Mark { fn drop(self:Self) { unsafe { registration_note(.code) } } }
fn install<F:FnMut(i32)->i32 + ClosedEnvironment>(f:F)->Registration<F> contract{result(create);} {
    unsafe { return c_register(registration_register,registration_close_backend,f) }
}
fn main()->i32 {
    let mark=Mark{code:37};
    var count=0;
    let f=move(value:i32)->i32=>{ count+=1; unsafe { if registration_reenter()!=0 {return -100} } return value+mark.code+count };
    let owner=install(f);
    if !registration_valid(owner) { return 3 }
    unsafe {
        if registration_clobber()!=0 { return 4 }
        if registration_fire(5)!=44 { return 5 }
    }
    registration_close(give owner);
    return 0
}
