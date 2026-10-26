pub struct Cell<T>{pub value:T;}
pub enum Flag<T>{N(T),Off}
pub fn pack(n:i64)->Cell<i64>{return Cell::<i64>{value:n}}
pub fn unpack(n:Cell<i64>)->i64{return n.value}
pub fn on()->Flag<bool>{return Flag::<bool>.N(true)}
