import model;import collection;
fn main()->void!i32{
 let a:Store<model.Item>=collection.empty();collection.reserve(a,1);
 collection.two(a,model.item(65),model.item(66));
 let b:Store<model.Other>=collection.empty();collection.reserve(b,1);collection.push(b,model.other(67));
 let popped=collection.pop(a)?;let moved=collection.relay(give b);
 return popped.n+i32(collection.observe(a))+i32(collection.observe(moved))-66
}