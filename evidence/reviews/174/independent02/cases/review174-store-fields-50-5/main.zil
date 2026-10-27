
extern "C" fn putchar(x:i32)->i32;
resource Empty {}
impl Drop for Empty { fn drop(self:Self) { unsafe { putchar(101) } } }
resource Mark { code:i32 }
impl Drop for Mark { fn drop(self:Self) { unsafe { putchar(.code) } } }
package Cell { manages empty:Empty; manages tag:Mark; n:i64; wide:[i32;3] }
package State { manages tag:Mark; manages left:Store<Cell>; manages right:Store<Cell> }
fn cell(code:i32)->Cell => Cell { empty:Empty{},tag:Mark{code:code},n:7,wide:[1,2,3] }
fn empty()->Store<Cell> contract{result(create)} { return store_new() }
fn state()->State {
    let left=empty(); store_reserve(left,2);
    store_push(left,cell(65)); store_push(left,cell(66));
    let right=empty(); store_reserve(right,1); store_push(right,cell(67));
    return State {tag:Mark{code:90},left:give left,right:give right}
}
 fn main()->i32{var current=state();let right=current.right;let detached=give current.left;let replacement=empty();store_reserve(replacement,1);store_push(replacement,cell(68));current.left=give replacement;store_reserve(right,2);store_push(right,cell(69));return i32(store_len(current.right))-2}