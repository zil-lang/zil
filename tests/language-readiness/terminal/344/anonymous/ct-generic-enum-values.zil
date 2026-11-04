enum Kind<T:Copy,P:Copy>{Zero,Value(T)}
type Choice=Kind<i32,bool>;
fn choose(t:Type,payload:bool)->Choice{if payload{return Choice.Value(42)}return Choice.Zero}
fn score(value:Choice)->i32{match value{Choice.Value(n)=>{return n},Choice.Zero()=>{return 0}}}
fn result()->i32{
    let value=#(choose(i32,true));
    let empty=#(choose(i32,false));
    return score(value)+score(empty)
}
