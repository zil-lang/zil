fn main()->i32{let x=2;let f=()=>x;var sum=0;for i in 0..3{sum+=f()}return sum-6}
