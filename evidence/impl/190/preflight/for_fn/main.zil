fn batch<F:Fn()->i32>(f:&F,n:i32)->i32{var sum=0;for i in 0..n{sum+=f()}return sum}fn main()->i32{let x=2;let f=()=>x;return batch(&f,3)-6}
