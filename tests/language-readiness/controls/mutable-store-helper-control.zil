resource Mark{n:i32}
package Panel{manages mark:Mark;count:i32}
fn bump(panel:&mut Panel){panel.count+=1}
fn main()->i32{let items:Store<Panel>=store_new();store_reserve(items,1);store_push(items,Panel{mark:Mark{n:3},count:0});bump(store_at_mut(items,0));let current=store_at(items,0);return current.count-1}
