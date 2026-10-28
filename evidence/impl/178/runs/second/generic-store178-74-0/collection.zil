
pub fn empty<T:StoreElement>()->Store<T> contract{result(create)}=>store_new()
pub fn reserve<T:StoreElement>(s:Store<T>,n:i64)->bool=>store_reserve(s,n)
pub fn push<T:StoreElement>(s:Store<T>,v:T)->T!void=>store_push(s,v)
pub fn pop<T:StoreElement>(s:Store<T>)->void!T=>store_pop(s)
pub fn len<T:StoreElement>(s:Store<T>)->i64=>store_len(s)
pub fn relay<T:StoreElement>(give s:Store<T>)->Store<T> contract{result(give)}=>give s

pub fn two<T:StoreElement>(s:Store<T>,a:T,b:T)->T!void{push(s,a)?;push(s,b)?;return ()}
pub fn observe<T:StoreElement>(s:Store<T>)->i64{return len(s)}
