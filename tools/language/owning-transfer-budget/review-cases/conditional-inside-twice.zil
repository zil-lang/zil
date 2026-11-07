resource Owner{n:i32}
fn base(give owner:Owner)->impl Fn()->i32{return move()=>owner.n}
fn wrap<F:FnOnce()->i32>(f:F,flag:bool)->impl FnOnce()->i32{return move()=>{let g=if flag{f}else{f};return g()+g()}}
fn main()->i32{let result=wrap(base(Owner{n:7}),true);return result()-7}
