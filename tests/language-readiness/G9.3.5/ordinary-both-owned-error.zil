struct Cell { n:u64; tag:i32 }
fn make(ok:bool)->Store<u64>!Store<Cell> contract{result(create)} {
    if ok {
        let records:Store<Cell>=store_new()
        store_reserve(records,1)
        store_push(records,Cell{n:18446744073709551610u64,tag:7})
        return give records
    }
    let numbers:Store<u64>=store_new()
    store_reserve(numbers,1)
    store_push(numbers,18446744073709551613u64)
    return give numbers
}
fn result()->u64 {
    var observed:u64=0u64
    match make(true) {
        .ok(records)=>{
            let cell=store_at(records,0)
            if cell.tag!=7 { return 1u64 }
            observed=cell.n
        },
        .err(_)=>{return 2u64}
    }
    if observed!=18446744073709551610u64 { return 3u64 }
    match make(false) {
        .ok(_)=>{return 4u64},
        .err(numbers)=>{return *store_at(numbers,0)}
    }
}
