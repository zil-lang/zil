fn good<T:StoreElement>()->Store<T> contract{result(create)} { let fresh:Store<T>=store_new(); return give fresh }
fn main() {}
