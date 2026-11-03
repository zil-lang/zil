extern "C" fn host_register(cb:extern "C" fn(c_handle,i32)->i64,ctx:c_handle)->c_handle contract{provides(static_callbacks());effects(ffi);};
extern "C" fn host_close(token:c_handle) contract{provides(static_callbacks());effects(ffi);};
extern "C" fn host_pump(value:i32)->i32 contract{provides(static_callbacks());effects(ffi);};
resource Subscription{token:c_handle}
impl Drop for Subscription{fn drop(self:Self){unsafe{host_close(.token)}}}
fn on_event(context:c_handle,value:i32)->i64{return i64(value+1)}
fn listen(seed:i32)->Subscription contract{result(create)}{let handler=move(context:c_handle,value:i32)->i64=>i64(value+seed);unsafe{return Subscription{token:host_register(c_callback(handler),c_null())}}}
fn main()->i32{let subscription=listen(1);unsafe{return host_pump(41)-42}}
