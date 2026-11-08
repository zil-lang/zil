import protocol
import library
pub package Item { manages data:ByteBuffer; value:i64 }
pub package Cursor { manages items:Store<Item>; index:i64 }
impl protocol.LendingIterator for Cursor {
 type Item=Item;
 fn next<life a>(self:&'a mut Self)->Option<&'a Item> {
  if self.index>=store_len(self.items) {return Option::<&'a Item>.None()}
  let index=self.index
  self.index+=1
  return Option::<&'a Item>.Some(store_at(self.items,index))
 }
}

fn byte(item:&Item)->u8 {return bytes_view(item.data)[0]}

pub fn run()->i32 {
 let items:Store<Item>=store_new()
 if !store_reserve(items,2) {return 1}
 let first=bytes_new()
 if !bytes_reserve(first,1) || !bytes_push(first,65u8) {return 5}
 store_push(items,Item{data:give first,value:20})
 let second=bytes_new()
 if !bytes_reserve(second,1) || !bytes_push(second,66u8) {return 6}
 store_push(items,Item{data:give second,value:22})
 var cursor=Cursor{items:give items,index:0}
 var total:i64=0
 match library.next(&mut cursor) {Option.Some(value)=>{total+=value.value;if byte(value)!=65u8{return 7}},Option.None()=>{return 2}}
 match library.next(&mut cursor) {Option.Some(value)=>{total+=value.value;if byte(value)!=66u8{return 8}},Option.None()=>{return 3}}
 match library.next(&mut cursor) {Option.Some(value)=>{return 4},Option.None()=>{}}
 return i32(total-42)
}
