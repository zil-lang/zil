fn twice<F:Fn()->i32>(a:&F,b:&F)->i32=>a()+b();fn main()->i32{let f=move()=>7;return twice(&f,&f)-14}
