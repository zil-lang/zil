extern "C" fn scope_register(callback:extern "C" fn(c_handle,i32)->i64,userdata:c_handle)->c_handle
contract{provides(static_callbacks(),scoped_callback_register());effects(ffi);};
extern "C" fn scope_close(token:c_handle)
contract{provides(static_callbacks(),scoped_callback_close());effects(ffi);};
extern "C" fn driver_configure(mode:i32);
extern "C" fn driver_pump(value:i32)->i32
contract{provides(static_callbacks());effects(ffi);};
extern "C" fn driver_roots()->i32;
extern "C" fn driver_body()->i32;
extern "C" fn driver_hit(label:i32);
pub fn configure(mode:i32){unsafe{driver_configure(mode)}}
pub fn roots()->i32{unsafe{return driver_roots()}}
pub fn hit(label:i32){unsafe{driver_hit(label)}}
fn pump(value:i32)->i32{unsafe{return driver_pump(value)}}
fn body_tick()->i32{unsafe{return driver_body()}}
pub fn run<F:FnMut(i32)->i32>(f:&mut F)->(bool,i32){
    let body=()->i32=>{
        body_tick()
        if pump(2)!=4{return 121}
        if pump(3)!=6{return 122}
        return 3
    }
    unsafe{return c_with_callback(scope_register,scope_close,f,body)}
}
