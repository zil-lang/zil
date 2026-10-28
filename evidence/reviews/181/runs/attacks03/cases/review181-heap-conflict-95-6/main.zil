extern "C" fn HeapReAlloc(n:i64)->i32;
resource Mark{n:i32}
package Entry{lead:u16;manages mark:Mark;wide:i64;tail:u8}
fn main()->void!i32 {
 let store:Store<Entry> = store_new();
 store_reserve(store,1);
 store_push(store,Entry{lead:u16(2),mark:Mark{n:3},wide:4,tail:u8(5)});
 let e=store_pop(store)?;
 return e.mark.n+i32(e.wide)-7
}
