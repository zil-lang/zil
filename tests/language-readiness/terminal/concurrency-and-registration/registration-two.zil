extern "C" fn registration_register(cb:extern "C" fn(c_handle,i32)->i64,ctx:c_handle)->c_handle contract{provides(static_callbacks(),scoped_callback_register());effects(ffi);};
extern "C" fn registration_close_backend(token:c_handle) contract{provides(static_callbacks(),scoped_callback_close());effects(ffi);};
extern "C" fn registration_clobber()->i32 contract{effects(ffi);};
extern "C" fn registration_fire_at(index:i32,value:i32)->i32 contract{effects(ffi);};
extern "C" fn registration_note(code:i32) contract{effects(ffi);};
extern "C" fn registration_reenter()->i32 contract{effects(ffi);};
resource Mark { code:i32 }
impl Drop for Mark { fn drop(self:Self) { unsafe { registration_note(.code) } } }
fn install<F:FnMut(i32)->i32 + ClosedEnvironment>(f:F)->Registration<F> contract{result(create);} {
    unsafe { return c_register(registration_register,registration_close_backend,f) }
}
fn main()->i32 {
    let first_mark=Mark{code:37}; var first_count=0;
    let first=install(move(value:i32)->i32=>{ first_count+=1; unsafe{if registration_reenter()!=0{return -100}}return value+first_mark.code+first_count });
    if !registration_valid(first){return 3}
    let second_mark=Mark{code:47}; var second_count=0;
    let second=install(move(value:i32)->i32=>{ second_count+=1; unsafe{if registration_reenter()!=0{return -100}}return value+second_mark.code+second_count });
    if !registration_valid(second){return 3}
    unsafe {
        if registration_clobber()!=0{return 4}
        if registration_fire_at(0,5)!=44{return 5}
        if registration_fire_at(1,5)!=54{return 6}
        if registration_fire_at(0,3)!=43{return 7}
        if registration_fire_at(1,10)!=60{return 8}
    }
    registration_close(give first);
    registration_close(give second);
    return 0
}
