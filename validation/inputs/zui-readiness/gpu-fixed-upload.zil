struct Vertex {x:f32;y:f32}
extern "C" fn upload(device:c_handle,vertices:&[Vertex;4])->i32 contract{provides(noescape(vertices));effects(ffi);};
fn main()->i32 {let v=Vertex{x:0.0f32,y:0.0f32};let data=[v,v,v,v];unsafe{return upload(c_null(),&data)}}
