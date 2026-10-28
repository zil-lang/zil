// UTF-8 scalar validation. No allocation, normalization or grapheme segmentation.
pub struct Bounds { pub first:i64; pub last:i64; pub count:i64; pub error:i64 }

fn next(bytes:&[u8], at:i64)->i64 {
    let first=i64(bytes[at])
    if first<128 { return at+1 }
    var width:i64=0
    if first>=194 && first<=223 { width=2 }
    else if first>=224 && first<=239 { width=3 }
    else if first>=240 && first<=244 { width=4 }
    else { return -1 }
    if bytes.len()-at<width { return -1 }
    let second=i64(bytes[at+1])
    if second<128 || second>191 { return -1 }
    if (first==224 && second<160) || (first==237 && second>159) ||
       (first==240 && second<144) || (first==244 && second>143) { return -1 }
    var part:i64=2
    while part<width {
        let byte=i64(bytes[at+part])
        if byte<128 || byte>191 { return -1 }
        part+=1
    }
    return at+width
}

// Scan the ENTIRE input even if both requested boundaries are already found.
// error reports the first byte of the malformed scalar; -1 means valid UTF-8.
pub fn locate(bytes:&[u8], begin:i64, end:i64)->Bounds {
    var first:i64=-1
    var last:i64=-1
    var count:i64=0
    var at:i64=0
    while at<bytes.len() {
        if count==begin { first=at }
        if count==end { last=at }
        let following=next(bytes,at)
        if following<0 { return Bounds{first:first,last:last,count:count,error:at} }
        at=following
        count+=1
    }
    if count==begin { first=at }
    if count==end { last=at }
    return Bounds{first:first,last:last,count:count,error:-1}
}
