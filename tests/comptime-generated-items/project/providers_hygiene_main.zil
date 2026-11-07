import common.io as io
syntax use providers.expressions as inc
syntax use providers.scaling as twice
syntax use providers.types as array4
syntax use providers.statements as once
syntax use providers.items as defs

#emit defs!{from_provider}
fn next(count:&mut i32)->i32 { *count+=1; return 65 }
fn main()->i32 {
    var count=0;
    let tmp=90;
    let support=999;
    #emit once!{next(&mut count)}
    io.emit(tmp);
    let value=inc!{41};
    let scaled=twice![21];
    let values:#splice(array4!{i32})=[1,2,3,4];
    if count!=1 || tmp!=90 || support!=999 { return 1 }
    if value!=42 || scaled!=42 || values[3]!=4 || from_provider()!=7 { return 2 }
    io.emit(48+value/10);
    io.emit(48+value%10);
    return 0
}
