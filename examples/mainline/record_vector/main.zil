import records
import io

fn word(value:i64) {io.write_byte(u8(value%256));io.write_byte(u8(value/256))}

// Binary exercise protocol. Each reply exposes the whole initialized byte
// prefix, not just a checksum, for differential checking by an independent model.
fn main()->i32 {
    let width=i64(io.read_byte())
    let low=io.read_byte();let high=io.read_byte()
    if width<0 || low<0 || high<0 {return 2}
    let limit=i64(low)+i64(high)*256
    let storage=bytes_new();let item=bytes_new()
    if !bytes_reserve(item,255) {return 3}
    var code=io.read_byte()
    while code>=0 {
        let offset=io.read_byte();let length=io.read_byte()
        if offset<0 || length<0 {return 2}
        let index=i64(offset)-128
        bytes_truncate(item,0)
        var p:i32=0
        while p<length {
            let byte=io.read_byte()
            if byte<0 {return 2}
            bytes_push(item,u8(byte));p+=1
        }
        var ok=false;var selected:u8=0u8
        if code==0 {ok=records.push(storage,width,bytes_view(item),limit)}
        else if code==1 {ok=records.insert(storage,width,index,bytes_view(item),limit)}
        else if code==2 {ok=records.set(storage,width,index,bytes_view(item))}
        else if code==3 {ok=records.remove(storage,width,index)}
        else if code==4 {ok=records.swap_remove(storage,width,index)}
        else if code==5 {ok=records.pop(storage,width)}
        else if code==6 {ok=records.clear(storage,width)}
        else if code==7 {
            let view=records.at(storage,width,index)
            ok=width>0 && view.len()==width
            if ok {selected=view[0]}
        }
        else if code==8 {ok=records.reserve(storage,width,index,limit)}
        else {return 2}
        if ok {io.write_byte(1u8)}else{io.write_byte(0u8)}
        io.write_byte(selected)
        word(bytes_len(storage));word(bytes_capacity(storage))
        let data=bytes_view(storage)
        var at:i64=0
        while at<data.len() {io.write_byte(data[at]);at+=1}
        code=io.read_byte()
    }
    return 0
}
