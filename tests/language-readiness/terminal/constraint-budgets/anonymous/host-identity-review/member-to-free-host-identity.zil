fn make<T:Copy,#N:i32,life a>(marker:Type)->i32{return N}
struct First{fn forward<T:Copy,#N:i32,life a>(marker:Type)->i32{return make::<T,N,'a>(marker)}}
struct Second{fn forward<T:Copy,#N:i32,life a>(marker:Type)->i32{return make::<T,N,'a>(marker)+1}}
fn result<life s>(p:&'s i32)->i32{return #(First.forward::<i32,42,'s>(i32))+ #(Second.forward::<i32,42,'s>(i32))}
fn main()->i32{let n=0;return result(&n)}
