struct Factory{fn constant<#N:i32>(marker:Type)->i32{return N}}
fn main()->i32{return #(Factory.constant::<42>(i32))}
