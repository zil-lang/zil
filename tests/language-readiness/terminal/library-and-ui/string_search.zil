import std.string
import std.store

fn comparison()->string.Error!i32 {
    let text=string.from_utf8("A🙂中\0🙂B",64)?
    let copy=string.clone(&text,64)?
    let less=string.from_utf8("A🙂中\0🙂A",64)?
    let greater=string.from_utf8("A🙂中\0🙂C",64)?
    if string.compare(&text,&copy)!=0 {return 1}
    if string.compare(&less,&text)!=-1 {return 2}
    if string.compare(&greater,&text)!=1 {return 3}
    return 0
}

fn searching()->string.Error!i32 {
    let text=string.from_utf8("A🙂中\0🙂B",64)?
    if string.find(&text,"🙂")?!=1 {return 4}
    if string.find_from(&text,"🙂",2)?!=4 {return 5}
    if string.find_from(&text,"🙂",5)?!=-1 {return 6}
    if string.find(&text,"中\0")?!=2 {return 7}
    if string.find(&text,"")?!=0 {return 8}
    if string.find_from(&text,"",6)?!=6 {return 9}
    if string.find(&text,"none")?!=-1 {return 10}
    return 0
}

fn failures()->string.Error!i32 {
    var text=string.from_utf8("A🙂中\0🙂B",64)?
    let continuation=[159u8,153u8,130u8]
    match string.find(&text,&continuation[..]) {
        .ok(_)=>{return 11},
        .err(_)=>{}
    }
    match string.find_from(&text,"B",7) {
        .ok(_)=>{return 12},
        .err(_)=>{}
    }
    match string.find_from(&text,"B",-1) {
        .ok(_)=>{return 13},
        .err(_)=>{}
    }
    match string.append(&mut text,"!",0) {
        .ok(())=>{return 14},
        .err(_)=>{}
    }
    if string.find_from(&text,"B",5)?!=5 {return 15}
    return 0
}

fn collected_positions()->string.Error!i32 {
    let text=string.from_utf8("A🙂中\0🙂B",64)?
    let hits:Store<i64>=store.new()
    if !store.reserve(hits,2) {return 16}
    store.push(hits,string.find(&text,"🙂")?)
    store.push(hits,string.find_from(&text,"🙂",2)?)
    if store.len(hits)!=2 || *store_at(hits,0)!=1 || *store_at(hits,1)!=4 {return 17}
    match store.push(hits,8) {
        .ok(())=>{return 18},
        .err(value)=>{if value!=8 {return 19}}
    }
    return 0
}

fn main()->string.Error!i32 {
    let first=comparison()?
    if first!=0 {return first}
    let second=searching()?
    if second!=0 {return second}
    let third=failures()?
    if third!=0 {return third}
    return collected_positions()
}
