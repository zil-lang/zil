extern "C" fn register_scope(cb:extern "C" fn(c_handle,i32)->i64,ctx:c_handle)->c_handle
contract{provides(static_callbacks(),scoped_callback_register());effects(ffi);};
extern "C" fn close_scope(token:c_handle)
contract{provides(static_callbacks(),scoped_callback_close());effects(ffi);};
extern "C" fn bad(cb:extern "C" fn(c_handle,i32)->i32,ctx:c_handle)->c_handle contract{provides(static_callbacks(),scoped_callback_register());effects(ffi);};fn main()->i32{var f=(value:i32)->i32=>value;let body=()->i32=>0;unsafe{let result=c_with_callback(bad,close_scope,&mut f,body);return result.1}}
