extern "C" fn reg(cb:extern "C" fn(c_handle,i32)->i64,ctx:c_handle)->c_handle contract{provides(static_callbacks(),scoped_callback_register());effects(ffi);};
extern "C" fn close(token:c_handle) contract{provides(static_callbacks(),scoped_callback_close());effects(ffi);};
extern "C" fn pump(value:i32)->i32 contract{provides(static_callbacks());effects(ffi);};
fn build_panel()->i32{return 2}
fn dispatch(value:i32)->i32{unsafe{return pump(value)}}
fn main()->i32{let seed=10;var f=move(value:i32)->i32=>value+seed;let body=()->i32=>{let event=build_panel();let first=dispatch(event);let next=build_panel();return first+dispatch(next+1)};unsafe{let result=c_with_callback(reg,close,&mut f,body);if !result.0{return 1};if f(4)!=14{return 2};return result.1-25}}
