fn next<life s>(slice:&'s [i64], flag:bool)->Option<&'s i64> {
    if flag { return Option::<&'s i64>.None(); }
    return Option::<&'s i64>.Some(&slice[0]);
}

fn result(flag:bool)->i64 {
    let values:[i64;1]=[42];
    let item=next(&values[..],flag);
    match item {
        Option.Some(value)=>{return *value},
        Option.None()=>{return 0}
    }
}

fn worker(seed:i32)->i32 {task_yield(); return seed}

fn main()->i32 {
    let values:[i64;1]=[17];
    let item=next(&values[..],true);
    match item {
        Option.Some(value)=>{return i32(*value-17+result(false)-42)},
        Option.None()=>{return i32(result(false)-42)}
    }
}
