resource R{code:i32}
package Item{manages token:R;value:i32}
fn apply<F:FnMut(&Item)->i32>(callback:&mut F,item:&Item)->i32{return callback(item)}
fn main()->i32{
 let item=Item{token:R{code:1};value:22}
 var total=0
 var callback=(item:&Item)=>{total+=item.value;return total}
 let result=apply(&mut callback,&item)
 return result-22
}
