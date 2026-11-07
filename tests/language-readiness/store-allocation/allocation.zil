struct Item { value:i64; tail:i64 }
fn result()->i64 {
    let items:Store<Item>=store_new()
    if !store_reserve(items,2){return -101}
    store_push(items,Item{value:1234567890123456,tail:17})
    store_push(items,Item{value:2345678901234567,tail:29})
    if store_reserve(items,-1) || store_reserve(items,9223372036854775807){return -102}
    let grown=store_reserve(items,4)
    if store_len(items)!=2 {return -103}
    if grown {if store_capacity(items)!=4{return -104}}
    else {if store_capacity(items)!=2{return -105}}
    let first=store_at(items,0)
    let a=first.value
    let b=store_at(items,1).value
    if first.tail!=17 || store_at(items,1).tail!=29{return -106}
    var observed:i64=0
    match store_pop(items){.ok(value)=>{observed=value.value},.err(_)=>{return -107}}
    if observed!=b || store_len(items)!=1{return -108}
    match store_pop(items){.ok(value)=>{observed+=value.value},.err(_)=>{return -109}}
    if observed!=a+b || store_len(items)!=0{return -110}
    if grown{return observed+200}
    return observed+100
}
