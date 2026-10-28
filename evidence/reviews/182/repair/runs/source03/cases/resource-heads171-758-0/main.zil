
trait Read{fn read(self:&Self)->i32;}
resource C{x:i32}
impl Read for C{fn read()->i32=>.x}
impl C{fn twice(self:&Self)->i32=>.x+.x}
fn inspect(p:&C)->i32=>p.read()+p.twice()
fn main()->i32{let c=C{x:7};return inspect(&c)-21}
