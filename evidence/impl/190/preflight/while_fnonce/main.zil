fn batch<F:FnOnce()->i32>(f:F,n:i32)->i32{var i=0;var sum=0;while i<n{sum+=f();i+=1}return sum}fn main()->i32{let f=()=>2;return batch(f,3)-6}
