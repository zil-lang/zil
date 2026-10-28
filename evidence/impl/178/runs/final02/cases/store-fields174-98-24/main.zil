
extern "C" fn putchar(x:i32)->i32;
resource R { code:i32 }
impl Drop for R { fn drop(self:Self) { unsafe { putchar(.code) } } }
package Item { manages item:R; n:i32 }
package State { manages tag:R; manages items:Store<Item>; n:i32 }
fn item(n:i32)->Item => Item { item:R{code:n}, n:n }
fn empty()->Store<Item> contract{result(create)} { return store_new() }
fn state(tag:i32,a:i32,b:i32)->State {
    let store=empty(); store_reserve(store,2);
    store_push(store,item(a)); store_push(store,item(b));
    return State {tag:R{code:tag},items:give store,n:4}
}
fn relay(state:State)->State => state
 package Bad{manages tag:R;states:[State;2]} fn main()->i32=>0