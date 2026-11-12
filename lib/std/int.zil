// Locale-independent signed decimal text. No whitespace, separators or NUL.
pub enum ParseError { Empty, InvalidDigit, Overflow }
pub struct Decimal { pub bytes:[u8;20]; pub len:i64 }

pub fn parse_i64(source:&[u8])->ParseError!i64 {
    if source.len()==0 {return ParseError.Empty}
    var at:i64=0
    var negative:bool=false
    if source[0]==45u8 {negative=true;at=1}
    if source[0]==43u8 {at=1}
    if at==source.len() {return ParseError.InvalidDigit}
    var lower:i64=-9223372036854775807
    if negative {lower=-9223372036854775808}
    var value:i64=0
    while at<source.len() {
        let digit=source[at]
        if digit<48u8 || digit>57u8 {return ParseError.InvalidDigit}
        let amount=i64(i32(digit)-48)
        if value<lower/10 {return ParseError.Overflow}
        let shifted=value*10
        if shifted<lower+amount {return ParseError.Overflow}
        value=shifted-amount
        at+=1
    }
    if negative {return value}
    return -value
}

pub fn format_i64(value:i64)->Decimal {
    var output=[0u8,0u8,0u8,0u8,0u8,0u8,0u8,0u8,0u8,0u8,
                0u8,0u8,0u8,0u8,0u8,0u8,0u8,0u8,0u8,0u8]
    var current=value
    let negative=current<0
    if current>0 {current=-current}
    var length:i64=0
    loop {
        let digit=i32(-(current%10))
        output[length]=u8(48+digit)
        length+=1
        current=current/10
        if current==0 {break}
    }
    if negative {output[length]=45u8;length+=1}
    var left:i64=0
    var right=length-1
    while left<right {
        let prior=output[left]
        output[left]=output[right]
        output[right]=prior
        left+=1
        right-=1
    }
    return Decimal{bytes:output,len:length}
}

// Full-width unsigned decimal, with an optional leading plus; minus, whitespace
// and separators are rejected. Cutoff checks precede arithmetic at every digit.
pub fn parse_u64(source:&[u8])->ParseError!u64 {
    if source.len()==0 {return ParseError.Empty}
    var at:i64=0
    if source[0]==43u8 {at=1}
    if at==source.len() {return ParseError.InvalidDigit}
    var value:u64=0u64
    while at<source.len() {
        let digit=source[at]
        if digit<48u8 || digit>57u8 {return ParseError.InvalidDigit}
        let amount=u64(digit-48u8)
        if value>1844674407370955161u64 {return ParseError.Overflow}
        if value==1844674407370955161u64 && amount>5u64 {return ParseError.Overflow}
        value=value*10u64+amount
        at+=1
    }
    return value
}
pub fn format_u64(value:u64)->Decimal {
    var output=[0u8,0u8,0u8,0u8,0u8,0u8,0u8,0u8,0u8,0u8,
                0u8,0u8,0u8,0u8,0u8,0u8,0u8,0u8,0u8,0u8]
    var current=value
    var length:i64=0
    loop {
        output[length]=48u8+u8(current%10u64)
        length+=1
        current=current/10u64
        if current==0u64 {break}
    }
    var left:i64=0
    var right=length-1
    while left<right {let prior=output[left];output[left]=output[right];output[right]=prior;left+=1;right-=1}
    return Decimal{bytes:output,len:length}
}
