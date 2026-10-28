fn batch<F:Fn()->i32>(f:&F,n:i32)->i32{var i=0;while i<n{i+=1}return i}fn main()->i32{let f=()=>2;return batch(&f,3)-3}
