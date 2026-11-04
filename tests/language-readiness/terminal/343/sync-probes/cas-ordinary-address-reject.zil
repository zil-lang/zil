fn main()->i32 {
    let cell=7
    let (old,changed)=atomic_compare_exchange(&cell,7,8)
    if changed {return old} return 0
}
