extern "C" fn register_scope(cb:extern "C" fn(c_handle,i32)->i64,ctx:c_handle)->c_handle
contract{provides(static_callbacks(),scoped_callback_register());effects(ffi);};
extern "C" fn close_scope(token:c_handle)
contract{provides(static_callbacks(),scoped_callback_close());effects(ffi);};
resource Mark{code:i32}
impl Drop for Mark{fn drop(self:Self){}}
fn main()->i32{let mark=Mark{code:1};var f=move(value:i32)->i32=>value+mark.code;let alias=&f;let body=move()->i32=>alias(1);let result=unsafe{c_with_callback(register_scope,close_scope,&mut f,body)};return result.1}
