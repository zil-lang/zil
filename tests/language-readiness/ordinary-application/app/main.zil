import model.records
import std.int as number
import std.iter
import std.owning_entries
import std.owning_hash_map
struct Select {minimum:u32}
struct Unwrap {}
impl iter.Predicate<owning_entries.Entry<records.Key,records.Packet>> for Select {
    fn keep(self:&mut Self,value:&owning_entries.Entry<records.Key,records.Packet>)->bool {return value.key.id>=self.minimum}
}
impl iter.Mapper<owning_entries.Entry<records.Key,records.Packet>> for Unwrap {
    type Output:StoreElement=records.Packet;
    fn map(self:&mut Self,value:owning_entries.Entry<records.Key,records.Packet>)->records.Packet {return value.value}
}
fn make(value:&u32)->impl Fn()->u32 {return move()=>*value}
fn relay<F:Fn()->u32>(value:F)->F {return value}
fn run()->void!i32 {
    let text=[52u8,50u8]
    var expected=0u32
    match number.parse_u32(&text[..]){.ok(value)=>{expected=value},.err(_)=>{return 1}}
    let offset=records.echo(21u32)
    let read=relay(make(&offset))
    var map=owning_hash_map.new::<records.Key,records.Packet>()
    var next=20u32
    while next<23u32 {
        let value=records.packet(next)?
        let entry=owning_entries.Entry::<records.Key,records.Packet>{key:records.Key{id:next},value:value}
        match owning_hash_map.insert::<records.Key,records.Packet>(&mut map,entry){.ok(_)=>{},.err(_)=>{return 2}}
        next+=1u32
    }
    let query=records.Key{id:read()}
    match owning_hash_map.get::<records.Key,records.Packet>(&map,&query){Option.Some(found)=>{if records.amount(found)!=21u32{return 3}},Option.None()=>{return 4}}
    let entries=owning_hash_map.into_entries(map)
    let input=iter.into_iter(give entries)?
    let selected=iter.filter(input,Select{minimum:21u32})
    let output=iter.collect(iter.map(selected,Unwrap{}))?
    let items=iter.into_store(output)
    if store_len(items)!=2{return 5}
    let first=store_at(items,0)
    let second=store_at(items,1)
    if records.amount(first)+records.amount(second)!=expected+1u32{return 6}
    return 0
}
fn main()->i32 {
    match run(){.ok(code)=>{return code},.err(_)=>{return 7}}
}
#[test]
fn inventory_pipeline()->bool {return main()==0}
