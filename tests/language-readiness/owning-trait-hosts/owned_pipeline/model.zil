import protocol
import library

pub package Item { manages data:ByteBuffer; tag:i64 }
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
impl protocol.Tag for Cursor {
    fn tag(self:&Self)->i64 {return self.index}
}
impl protocol.Duplicate for Item {
    fn duplicate(self:&Self)->Self {
        let data=bytes_new()
        if bytes_reserve(data,1) {bytes_push(data,bytes_view(self.data)[0])}
        return Item{data:give data,tag:self.tag+1}
    }
    fn try_duplicate(self:&Self,fail:bool)->protocol.Error!Self {
        let data=bytes_new()
        if !bytes_reserve(data,1) {return protocol.Error.Allocation}
        bytes_push(data,bytes_view(self.data)[0])
        if fail {return protocol.Error.Requested}
        return Item{data:give data,tag:self.tag+2}
    }
}

fn byte(item:&Item)->u8 {return bytes_view(item.data)[0]}
fn consume(item:Item)->i64 {return item.tag}

pub fn run()->i32 {
    let items:Store<Item>=store_new()
    if !store_reserve(items,2) {return 1}
    let first=bytes_new()
    if !bytes_reserve(first,1) || !bytes_push(first,65u8) {return 2}
    store_push(items,Item{data:give first,tag:20})
    let second=bytes_new()
    if !bytes_reserve(second,1) || !bytes_push(second,66u8) {return 3}
    store_push(items,Item{data:give second,tag:22})
    var cursor=Cursor{items:give items,index:0}
    match library.next_tagged(&mut cursor,7) {
        Option.None()=>{return 4},
        Option.Some(original)=>{
            let copy:Item=original.duplicate()
            if bytes_len(copy.data)!=1 {return 10}
            if copy.tag!=21 || byte(&copy)!=65u8 {return 11}
            if consume(copy)!=21 {return 12}
            let requested:protocol.Error!Item=original.try_duplicate(true)
            match requested {
                .ok(unexpected)=>{return 13},
                .err(error)=>{match error {
                    protocol.Error.Requested()=>{},
                    protocol.Error.Allocation()=>{return 14}
                }}
            }
            let completed:protocol.Error!Item=original.try_duplicate(false)
            match completed {
                .err(_)=>{return 15},
                .ok(fresh)=>{
                    if bytes_len(fresh.data)!=1 {return 16}
                    if fresh.tag!=22 || byte(&fresh)!=65u8 {return 17}
                    if consume(fresh)!=22 {return 18}
                }
            }
            if original.tag!=20 || byte(original)!=65u8 {return 19}
        }
    }
    match library.next_tagged(&mut cursor,true) {
        Option.None()=>{return 5},
        Option.Some(original)=>{
            let copy:Item=original.duplicate()
            if bytes_len(copy.data)!=1 {return 20}
            if copy.tag!=23 || byte(&copy)!=66u8 {return 21}
            if consume(copy)!=23 {return 22}
            let requested:protocol.Error!Item=original.try_duplicate(true)
            match requested {
                .ok(unexpected)=>{return 23},
                .err(error)=>{match error {
                    protocol.Error.Requested()=>{},
                    protocol.Error.Allocation()=>{return 24}
                }}
            }
            let completed:protocol.Error!Item=original.try_duplicate(false)
            match completed {
                .err(_)=>{return 25},
                .ok(fresh)=>{
                    if bytes_len(fresh.data)!=1 {return 26}
                    if fresh.tag!=24 || byte(&fresh)!=66u8 {return 27}
                    if consume(fresh)!=24 {return 28}
                }
            }
            if original.tag!=22 || byte(original)!=66u8 {return 29}
        }
    }
    match library.next_tagged(&mut cursor,0usize) {
        Option.Some(unexpected)=>{return 6},
        Option.None()=>{}
    }
    return 0
}
