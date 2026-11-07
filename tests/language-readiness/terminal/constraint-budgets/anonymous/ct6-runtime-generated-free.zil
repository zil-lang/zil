import std.syntax as syntax;
#emit quote<ItemList>{
 fn $(syntax.export("result"))<T:Copy,#N:i32>(marker:T)->i32{return #(N+1)}
};
fn main()->i32{return result::<bool,7>(true)}
