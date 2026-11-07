struct Marker { n:u64 }

fn make()->Store<u64> contract{result(create)} {
    let values:Store<u64>=store_new()
    store_reserve(values,1)
    store_push(values,11u64)
    return give values
}

fn reader()->impl FnOnce()->u64 {
    let packet=(make(),Marker{n:3u64},[5u64,7u64])
    return move()=>{
        let (values,marker,bytes)=packet
        return *store_at(values,0)+marker.n+bytes[0]
    }
}

fn result()->u64 {
    let read=reader()
    return read()
}
