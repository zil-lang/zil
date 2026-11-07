type Context=i32;
fn first<life Context,T:Copy>(value:&'Context Context,tag:T)->&'Context Context{return value}
fn main()->i32{
    let value:Context=42;
    return *first::<bool>(&value,true)-42
}
