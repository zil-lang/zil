extern "C" fn register_scope(cb:extern "C" fn(c_handle,i32)->i64,ctx:c_handle)->c_handle
contract{provides(static_callbacks(),scoped_callback_register());effects(ffi);};
extern "C" fn close_scope(token:c_handle)
contract{provides(static_callbacks(),scoped_callback_close());effects(ffi);};
fn plain(ctx:c_handle,value:i32)->i64{return i64(value)}fn main()->i32{unsafe{let token=register_scope(c_callback(plain),c_null());close_scope(token)}return 0}
