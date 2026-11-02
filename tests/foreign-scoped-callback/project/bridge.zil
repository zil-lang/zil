extern "C" fn scope_register(callback:extern "C" fn(c_handle,i32)->i64,userdata:c_handle)->c_handle
contract{provides(static_callbacks(),scoped_callback_register());effects(ffi);};
extern "C" fn scope_close(token:c_handle)
contract{provides(static_callbacks(),scoped_callback_close());effects(ffi);};
extern "C" fn driver_configure(mode:i32);
extern "C" fn driver_pump(value:i32)->i32
contract{provides(static_callbacks());effects(ffi);};
extern "C" fn driver_outer(value:i32)->i32
contract{provides(static_callbacks());effects(ffi);};
extern "C" fn driver_reenter()->i32
contract{provides(static_callbacks());effects(ffi);};
extern "C" fn driver_roots()->i32;
extern "C" fn driver_body()->i32;
extern "C" fn driver_hit(label:i32);
extern "C" fn driver_drop(label:i32);
extern "C" fn driver_factory(label:i32);
extern "C" fn driver_zero()->i32;
extern "C" fn getchar()->i32;
pub fn read()->i32{unsafe{return getchar()}}
pub fn configure(mode:i32){unsafe{driver_configure(mode)}}
pub fn pump(value:i32)->i32{unsafe{return driver_pump(value)}}
pub fn outer(value:i32)->i32{unsafe{return driver_outer(value)}}
pub fn reenter()->i32{unsafe{return driver_reenter()}}
pub fn roots()->i32{unsafe{return driver_roots()}}
pub fn body()->i32{unsafe{return driver_body()}}
pub fn hit(label:i32){unsafe{driver_hit(label)}}
pub fn dropped(label:i32){unsafe{driver_drop(label)}}
pub fn factory(label:i32){unsafe{driver_factory(label)}}
pub fn zero()->i32{unsafe{return driver_zero()}}
pub fn run<F:FnMut(i32)->i32,B:FnOnce()->i32>(f:&mut F,body:B)->(bool,i32){
    unsafe{return c_with_callback(scope_register,scope_close,f,body)}
}
