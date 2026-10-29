
resource R{n:i32}package P{manages r:R}enum E{Fail}
fn make<T:StoreElement>()->E!Store<T> contract{result(create)}{let s:Store<T>=store_new();let alias=s;return give s}
fn pass<T:StoreElement>(give x:E!Store<T>)->E!Store<T> contract{result(give)}=>x
fn rewrap<T:StoreElement>(give x:E!Store<T>)->E!Store<T> contract{result(give)}{let s=x?;return give s}
fn wrap<T:StoreElement>(give s:Store<T>)->E!Store<T> contract{result(give)}=>give s
fn main()->E!i32{let pending=pass(make::<P>());let s=rewrap(pending)?;let final=wrap(give s);return 0}
