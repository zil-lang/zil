fn keep<F:Fn()->i32>(f:F)->F=>f;fn main()->i32{let f=move()=>7;let g=keep(f);return g()-7}
