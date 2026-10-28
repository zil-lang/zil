
extern "C" fn putchar(n:i32)->i32;
resource R{code:i32}
impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}
struct Point{x:i32;y:i32}
package Cell{manages owner:R;samples:[i32;3];point:Point;tag:i32}
fn cell(n:i32)->Cell=>Cell{owner:R{code:n},samples:[1,2,3],point:Point{x:4,y:5},tag:n}
fn empty()->Store<Cell> contract{result(create)}{return store_new()}
fn read(v:&Cell)->i32=>v.tag
fn bump(v:&mut Cell, delta:i32)->i32{v.tag+=delta;return v.tag}
fn relay_bump(v:&mut Cell)->i32{return bump(v,2)}fn main()->void!i32{let s=empty();store_reserve(s,8);store_push(s,cell(65));store_push(s,cell(66));let v=store_at_mut(s,0);let first=relay_bump(v);let second=bump(v,3);let shared=read(v);let child=&mut v.samples[2];*child=7;let sample=*child;store_reserve(s,16);let last=store_pop(s)?;return first+second+shared+sample+last.tag-280}