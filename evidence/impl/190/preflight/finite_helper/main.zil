fn batch<F:Fn()->i32>(f:&F)->i32{return f()+f()}fn main()->i32{let x=2;let f=()=>x;return batch(&f)-4}
