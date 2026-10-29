
extern "C" fn putchar(c:i32)->i32;
resource Empty{}
impl Drop for Empty{fn drop(self:Self){unsafe{putchar(90)}}}
package Item{manages marker:Empty;tag:u8}
fn main()->void!i32{let s:Store<Item>=store_new();if !store_reserve(s,2){return ()};store_push(s,Item{marker:Empty{},tag:7});store_push(s,Item{marker:Empty{},tag:9});let item=store_pop(s)?;return i32(item.tag)-9}
