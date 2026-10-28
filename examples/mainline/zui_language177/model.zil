extern "C" fn putchar(value: i32) -> i32;

pub resource Ticket { pub code: i32 }
impl Drop for Ticket { fn drop(self: Self) { unsafe { putchar(.code) } } }
pub package Item { manages ticket: Ticket; manages data: ByteBuffer; pub code: i32 }
pub fn item(code: i32) -> Item {
    return Item { ticket: Ticket { code: code }, data: bytes_new(), code: code }
}

pub resource Controller { pub count: i32 }
pub trait Read { fn read(self: &Self) -> i32; }
impl Read for Controller { fn read() -> i32 => .count }
