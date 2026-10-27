pub trait Position { fn position(self:&Self)->i64; }
pub struct Cursor { pub count:i64 }
impl Position for Cursor { fn position()->i64=>.count }
impl Cursor { pub fn advance(self:&mut Self,n:i64) { .count+=n } }
pub fn make()->Cursor=>Cursor{count:0}
