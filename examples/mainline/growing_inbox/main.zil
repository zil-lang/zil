// Headless composition: queued byte events -> stateful trait-method callback ->
// layout byte output -> graceful drain or explicit cancel. No renderer/async claim.
import queue
trait Layout {
    fn advance(self:&mut Self,amount:i64);
    fn position(self:&Self)->i64;
}
struct Cursor {column:i64;row:i64;events:i64}
impl Layout for Cursor {
    fn advance(amount:i64) {
        .column+=amount
        .row+=.column/80
        .column=.column%80
        .events+=1
    }
    fn position()->i64 {return .row*80+.column}
}
fn layout()->impl FnMut(i32)->i64 {
    var cursor=Cursor{column:0,row:0,events:0}
    return move(event:i32)->i64=>{
        if event == -1 {return cursor.events}
        cursor.advance(i64(event%11)+1)
        return cursor.position()
    }
}
fn main()->i32 {
    var events=queue.make(3,4096)
    var apply=layout()
    let output=bytes_new()
    if !bytes_reserve(output,6144) {return 1}
    var n:i32=0
    while n<4096 {if events(n%256)!=i64(n)+1 {return 2};n+=1}
    var position:i64=0
    n=0
    while n<6144 {
        if n==2048 {
            var more:i32=0
            while more<2048 {if events(more%256)!=2049+i64(more) {return 3};more+=1}
            if events(-2)!=4096 || events(9)!=-1 {return 4}
        }
        let event=events(-1)
        if event!=i64(n%256) {return 5}
        position+=i64((n%256)%11)+1
        let computed=apply(i32(event))
        if computed!=position {return 6}
        if !bytes_push(output,u8(computed%251)) {return 7}
        n+=1
    }
    if events(-4)!=0 || events(-3)!=0 || events(-1)!=-4 || apply(-1)!=6144 {return 8}
    let view=bytes_view(output)
    if view.len()!=6144 {return 9}
    n=0;position=0
    while n<6144 {position+=i64((n%256)%11)+1;if view[i64(n)]!=u8(position%251) {return 10};n+=1}
    var discarded=queue.make(0,64)
    n=0
    while n<33 {discarded(n);n+=1}
    if discarded(-5)!=33 || discarded(-4)!=0 || discarded(-3)!=0 || discarded(-6)!=1 {return 11}
    if discarded(1)!=-1 || discarded(-1)!=-4 {return 12}
    return 0
}
