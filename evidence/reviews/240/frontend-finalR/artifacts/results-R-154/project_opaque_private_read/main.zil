import api
fn main()->i32{var e=api.make();e=api.relay(e);match e{api.E.V(p)=>{return p.tag}}}