fn take<T:StoreElement>(give source:Store<T>)->Store<T> contract{result(give)}=>give source;fn bad<T:StoreElement>(source:Store<T>)->Store<T> contract{result(give)}=>take(source)
fn main() {}
