import protocol
import library

pub package Item { manages data:ByteBuffer; tag:i64 }
pub package Cursor { manages items:Store<Item>; index:i64 }
pub package Reply { manages data:ByteBuffer; tag:i64 }

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
fn make_reply(item:&Item,count:i64)->protocol.Error!Reply {
    let data=bytes_new()
    if !bytes_reserve(data,1) {return protocol.Error.Allocation}
    bytes_push(data,byte(item))
    return Reply{data:give data,tag:item.tag+count}
}
fn dispatch<F:FnMut(&Item)->protocol.Error!Reply>(handler:&mut F,item:&Item)->protocol.Error!Reply {
    return handler(item)
}
fn consume_reply(reply:Reply)->i64 {return reply.tag}

fn consume_replies(give replies:Store<Reply>)->i64 {
    var total:i64=0
    while store_len(replies)>0 {
        match store_pop(replies) {
            .err(_)=>{return -1},
            .ok(reply)=>{total+=consume_reply(reply)}
        }
    }
    return total
}

pub fn run()->i32 {
    let replies:Store<Reply>=store_new()
    let items:Store<Item>=store_new()
    if !store_reserve(items,2) {return 1}
    let first=bytes_new()
    if !bytes_reserve(first,1) || !bytes_push(first,65u8) {return 2}
    store_push(items,Item{data:give first,tag:20})
    let second=bytes_new()
    if !bytes_reserve(second,1) || !bytes_push(second,66u8) {return 3}
    store_push(items,Item{data:give second,tag:22})
    var cursor=Cursor{items:give items,index:0}
    var count:i64=0
    var handler=move(item:&Item)->protocol.Error!Reply=>{
        count+=1
        return make_reply(item,count)
    }
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
            match dispatch(&mut handler,original) {
                .err(_)=>{return 30},
                .ok(reply)=>{
                    if bytes_len(reply.data)!=1 {return 31}
                    if reply.tag!=21 || bytes_view(reply.data)[0]!=65u8 {return 32}
                    match library.collect(replies,reply) {
                        .err(returned)=>{
                            if bytes_len(returned.data)!=1 || bytes_view(returned.data)[0]!=65u8 {return 55}
                            if consume_reply(returned)!=21 {return 53}
                            if consume_replies(give replies)!=0 {return 57}
                            return 50
                        },
                        .ok(_)=>{}
                    }
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
            match dispatch(&mut handler,original) {
                .err(_)=>{return 40},
                .ok(reply)=>{
                    if bytes_len(reply.data)!=1 {return 41}
                    if reply.tag!=24 || bytes_view(reply.data)[0]!=66u8 {return 42}
                    match library.collect(replies,reply) {
                        .err(returned)=>{
                            if bytes_len(returned.data)!=1 || bytes_view(returned.data)[0]!=66u8 {return 56}
                            if consume_reply(returned)!=24 {return 54}
                            if consume_replies(give replies)!=21 {return 58}
                            return 51
                        },
                        .ok(_)=>{}
                    }
                }
            }
            if original.tag!=22 || byte(original)!=66u8 {return 29}
        }
    }
    match library.next_tagged(&mut cursor,0usize) {
        Option.Some(unexpected)=>{return 6},
        Option.None()=>{}
    }
    if consume_replies(give replies)!=45 {return 52}
    return 0
}
