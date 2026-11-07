import std.utf8
import std.text_edit
fn malformed(bytes:&[u8])->bool { return utf8.locate(bytes,0,0).error>=0 }
fn main()->i32 {
    let input=[65u8,240u8,159u8,152u8,128u8,228u8,184u8,173u8]
    let bounds=utf8.locate(&input[..],1,2)
    if bounds.count!=3 || bounds.first!=1 || bounds.last!=5 || bounds.error!=-1 { return 11 }
    let scalar=utf8.decode(&input[..],1)
    if scalar.status!=1 || scalar.value!=128512 || scalar.next!=5 { return 12 }
    if utf8.decode(&input[..],2).status!=2 || utf8.decode(&input[..],8).status!=0 { return 13 }
    if utf8.decode(&input[..],-1).status!=2 || utf8.decode(&input[..],9).status!=2 { return 14 }
    let overlong=[192u8,128u8]
    let surrogate=[237u8,160u8,128u8]
    let outside=[244u8,144u8,128u8,128u8]
    let short=[240u8,159u8]
    if !malformed(&overlong[..]) || !malformed(&surrogate[..]) || !malformed(&outside[..]) || !malformed(&short[..]) { return 15 }
    let suffix=[66u8]
    let result=text_edit.replace(&input[..],1,2,&suffix[..],32)
    if result.code!=0 { return 16 }
    let data=text_edit.take(result)
    let view=bytes_view(data)
    if view.len()!=5 || view[0]!=65u8 || view[1]!=66u8 || view[2]!=228u8 { return 17 }
    let failure=text_edit.replace_in_place(data,0,0,&overlong[..],32)
    if failure.code!=2 || bytes_len(data)!=5 { return 18 }
    let growth=text_edit.replace_in_place(data,1,2,&input[1..5],32)
    if growth.code!=0 || bytes_len(data)!=8 { return 19 }
    let last=bytes_view(data)
    if utf8.locate(last,0,0).count!=3 || last[1]!=240u8 || last[5]!=228u8 { return 20 }
    return 0
}
