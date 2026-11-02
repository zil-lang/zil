import api
fn main()->i32{return api.consume(api.E.V(api.R{n:2}),()=>3)-5}