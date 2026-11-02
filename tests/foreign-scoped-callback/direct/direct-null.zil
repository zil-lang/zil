extern "C" fn reg(cb:extern "C" fn(c_handle,i32)->i64,ctx:c_handle)->c_handle contract{provides(static_callbacks(),scoped_callback_register());effects(ffi);};
extern "C" fn close(token:c_handle) contract{provides(static_callbacks(),scoped_callback_close());effects(ffi);};
extern "C" fn body_called()->i32 contract{effects(ffi);};
fn main()->i32 {
    var f=(value:i32)->i32=>value+1;
    let body=()->i32=>{unsafe{return body_called()}};
    unsafe {
        let result=c_with_callback(reg,close,&mut f,body);
        if result.0 || result.1 != 0 { return 1 }
    }
    return 0
}
