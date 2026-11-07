fn repeat<F:Fn()->i32>(f:&F,count:i32)->i32{var n=0;var sum=0;while n<count{sum=sum+f();n=n+1};return sum}fn main()->i32{let f=move()=>7;return repeat(&f,2)-14}
