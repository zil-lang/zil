fn main()->i32 {
    let (ok,value)=atomic_scope(7,(cell:&AtomicI32)->i32=>{
        let (first,changed)=atomic_compare_exchange(cell,0,9)
        if first!=7 || changed || atomic_load(cell)!=7 {return 2}
        let (second,won)=atomic_compare_exchange(cell,7,11)
        if second!=7 || !won || atomic_load(cell)!=11 {return 3}
        return 0
    })
    if !ok {return 1} return value
}
