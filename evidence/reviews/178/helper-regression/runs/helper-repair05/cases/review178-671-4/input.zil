package Owner<T:StoreElement>{manages items:Store<T>}fn bad<T:StoreElement>(owner:Owner<T>)->i64{var next=owner;let first=next.items;let second=first;next.items=store_new();return store_len(second)}
fn main() {}
