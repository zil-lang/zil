extern "C" fn zui_gpu_create(window:c_handle,width:i32,height:i32,result:&mut c_handle)->i32 contract{provides(noescape(result));effects(ffi);};
extern "C" fn zui_gpu_clear_present(device:c_handle,r:f32,g:f32,b:f32,a:f32)->i32 contract{effects(ffi);};
extern "C" fn zui_gpu_release(device:c_handle) contract{effects(ffi);};
fn main()->i32 {var device=c_null();unsafe {let code=zui_gpu_create(c_null(),64,64,&mut device);if code<0{return 1}let presented=zui_gpu_clear_present(device,0.1f32,0.2f32,0.3f32,1.0f32);zui_gpu_release(device);return presented}}
