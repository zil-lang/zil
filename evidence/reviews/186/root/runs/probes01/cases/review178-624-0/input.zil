fn bad<T:StoreElement>(give source:Store<T>)->i64 { var alias=source; let alias2=alias; let moved=give source; return store_len(alias2) }
fn main() {}
