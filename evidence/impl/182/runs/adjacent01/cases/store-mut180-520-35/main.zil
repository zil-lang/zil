
extern "C" fn putchar(n:i32)->i32;
resource R{code:i32}
impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}
struct Point{x:i32;y:i32}
package Cell{manages owner:R;samples:[i32;3];point:Point;tag:i32}
fn cell(n:i32)->Cell=>Cell{owner:R{code:n},samples:[1,2,3],point:Point{x:4,y:5},tag:n}
fn empty()->Store<Cell> contract{result(create)}{return store_new()}
fn read(v:&Cell)->i32=>v.tag
fn bump(v:&mut Cell, delta:i32)->i32{v.tag+=delta;return v.tag}

fn receiver()->Store<Cell> contract{result(create)}{unsafe{putchar(82)};let s=empty();store_reserve(s,1);store_push(s,cell(65));return give s}
fn index()->i64{unsafe{putchar(73)};return 0}
fn rhs()->i32{unsafe{putchar(86)};return 9}
fn main()->i32{store_at_mut(receiver(),index()).tag=rhs();return 0}
