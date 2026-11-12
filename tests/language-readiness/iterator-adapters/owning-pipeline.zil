import std.iter
extern "C" fn putchar(value:i32)->i32;
resource Mark {code:i32}
impl Drop for Mark {fn drop(self:Self){unsafe{putchar(.code)}}}
package Item {manages mark:Mark; value:i32}
package Output {item:Item; value:i32}
package Wrap {manages mark:Mark}
package Even {manages mark:Mark}
impl iter.Mapper<Item> for Wrap {
    type Output:StoreElement=Output;
    fn map(self:&mut Self,value:Item)->Output {
        let doubled=value.value*2
        return Output{item:value,value:doubled}
    }
}
impl iter.Predicate<Item> for Even {
    fn keep(self:&mut Self,value:&Item)->bool {return value.value%2==0}
}
fn run()->void!i32 {
    let values:Store<Item>=store_new()
    if !store_reserve(values,3){return 1}
    store_push(values,Item{mark:Mark{code:65},value:1})
    store_push(values,Item{mark:Mark{code:66},value:2})
    store_push(values,Item{mark:Mark{code:67},value:3})
    let input=iter.into_iter::<Item>(give values)?
    let selected=iter.filter(input,Even{mark:Mark{code:80}})
    let mapped=iter.map(selected,Wrap{mark:Mark{code:87}})
    let collected=iter.collect(mapped)?
    let output=iter.into_store(collected)
    if store_len(output)!=1{return 2}
    let value=store_at(output,0)
    if value.value!=4 || value.item.value!=2{return 3}
    return 0
}
fn main()->i32 {
    match run(){.ok(code)=>{return code},.err(exhausted)=>{return 4}}
}
