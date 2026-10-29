extern "C" fn putchar(code:i32)->i32;

pub enum Error { Failed }

pub resource Mark { code:i32 }
impl Drop for Mark {
    fn drop(self:Self) { unsafe { putchar(.code) } }
}

pub fn marker(code:i32)->Mark contract { result(create) } => Mark { code:code }
pub fn emit(code:i32) { unsafe { putchar(code) } }

pub package Item {
    lead:u8;
    manages mark:Mark;
    wide:i64;
}

pub fn item(code:i32)->Item => Item { lead:u8(1), mark:marker(code), wide:7 }
pub fn score(store:Store<Item>)->i32 {
    let view=store_at(store,0);
    return i32(view.wide)
}
