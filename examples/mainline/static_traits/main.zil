// Headless layout/display-list consumer. Concrete trait dispatch only.
trait Measure {
    fn width(self:&Self)->i64;
    fn height(self:&Self)->i64;
}
struct Label { columns:i64; line:i64 }
struct Panel { w:i64; h:i64 }
impl Measure for Label {
    fn width()->i64 { return .columns*8 }
    fn height()->i64 { return .line }
}
impl Measure for Panel {
    fn width()->i64 { return .w }
    fn height()->i64 { return .h }
}
trait Position { fn move_to(self:&mut Self,x:i64,y:i64); }
struct Rect { x:i64; y:i64; w:i64; h:i64 }
impl Position for Rect { fn move_to(x:i64,y:i64) { .x=x; .y=y; } }
fn emit_number(out:ByteBuffer,n:i64)->bool {
    var value=n
    var shift:i64=0
    while shift<8 { if !bytes_push(out,u8(value%256)){return false}; value=value/256; shift+=1 }
    return true
}
fn append(out:ByteBuffer,r:&Rect)->bool {
    return emit_number(out,r.x)&&emit_number(out,r.y)&&emit_number(out,r.w)&&emit_number(out,r.h)
}
fn main()->i32 {
    let label=Label{columns:7,line:16}
    let panel=Panel{w:24,h:16}
    let out=bytes_new()
    if !bytes_reserve(out,16384){return 1}
    var row:i64=0
    while row<32 {
        var column:i64=0
        while column<16 {
            var width=label.width()
            if column%2==1 {width=panel.width()}
            var r=Rect{x:0,y:0,w:width,h:label.height()}
            r.move_to(column*64,row*24)
            if !append(out,&r){return 2}
            column+=1
        }
        row+=1
    }
    if bytes_len(out)!=16384{return 3}
    let view=bytes_view(out)
    // Independently determined final rectangle: (960,744,24,16).
    if view[16352]!=192u8 || view[16353]!=3u8{return 4}
    if view[16360]!=232u8 || view[16361]!=2u8{return 5}
    if view[16368]!=24u8 || view[16376]!=16u8{return 6}
    return 0
}
