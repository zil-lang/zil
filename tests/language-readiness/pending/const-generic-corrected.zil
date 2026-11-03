fn identity<#N:i32>(x:i32)->i32{return x}
fn main()->i32{return identity::<2>(7)-7}
