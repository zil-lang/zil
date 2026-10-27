
extern "C" fn putchar(n:i32)->i32;
resource R{code:i32}impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}
package Cell{manages owner:R;samples:[i32;3];tag:i32}
package State{manages head:R;manages left:Store<Cell>;manages right:Store<Cell>}
fn empty()->Store<Cell>contract{result(create)}{return store_new()}
fn cell(n:i32)->Cell=>Cell{owner:R{code:n},samples:[1,2,3],tag:n}
fn state()->State{let a=empty();let b=empty();store_reserve(a,8);store_reserve(b,2);store_push(a,cell(65));store_push(b,cell(66));return State{head:R{code:90},left:give a,right:give b}}
fn relay(s:State)->State=>s
fn id(x:&i32)->&i32=>x
fn inspect(view:&Cell,other:Store<Cell>)->i32{store_reserve(other,7);let point=id(&view.samples[1]);return *point+view.tag}
fn main()->i32{let s=state();let view=store_at(s.left,0);let moved=relay(s);return view.tag}