fn batch<F:FnMut()->i32>(f:&mut F,n:i32)->i32{var i=0;var sum=0;while i<n{sum+=f();i+=1}return sum}fn main()->i32{var x=0;var f=()=>{x+=1;return x};return batch(&mut f,3)-6}
