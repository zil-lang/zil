extern "C" fn putchar(code:i32)->i32;

pub enum Error { Allocate; Logical }
pub resource Mark { code:i32 }
impl Drop for Mark { fn drop(self:Self) { unsafe { putchar(.code) } } }

pub fn marker(code:i32)->Mark contract { result(create) } => Mark { code:code }
pub fn emit(code:i32) { unsafe { putchar(code) } }

pub package Item { lead:u8; manages mark:Mark; wide:i64 }
pub package Other { manages mark:Mark; count:i32 }
pub fn item(code:i32)->Item => Item { lead:u8(3), mark:marker(code), wide:123456789 }
pub fn other(code:i32)->Other => Other { mark:marker(code), count:7 }

pub fn score_first(s:Store<Item>)->bool {
    let a=store_at(s,0);let b=store_at(s,1);
    return a.lead==u8(3) && a.wide==123456789 && b.wide==123456789
}
pub fn score_second(s:Store<Other>)->bool {
    let a=store_at(s,0);let b=store_at(s,1);
    return a.count==7 && b.count==7
}
