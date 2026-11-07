#[align(16)]
struct Vertex {x:f32;y:f32;z:f32;w:f32}
extern "C" fn upload(vertices:&[Vertex;1])->i32 contract{provides(noescape(vertices));effects(ffi);};
fn main()->i32 {let data=[Vertex{x:0.0f32,y:0.0f32,z:0.0f32,w:1.0f32}];unsafe{return upload(&data)}}
