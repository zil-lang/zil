struct Phantom<life a,T:Copy,#N:i32>{n:i32}
fn make<T:Copy,#N:i32,life a>(marker:Type)->i32{return N}
fn middle<life b,#M:i32,U:Copy>(marker:Type)->i32{return make::<U,M,'b>(marker)}
fn forward<#N:i32,life a,T:Copy>(marker:Type)->i32{return middle::<'a,N,T>(marker)}
fn result<life s>(p:&'s i32)->i32{return #(forward::<42,'s,i32>(i32))}
fn main()->i32{let n=0;return result(&n)}
