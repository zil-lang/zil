struct Sample { total:i64; count:i64; }
struct Stage { transform:fn(i64)->i64; }

fn add(x:i64)->i64=>x+10
fn double(x:i64)->i64=>x*2
fn choose(flag:bool)->fn(i64)->i64=>if flag {add} else {double}
fn apply(f:fn(i64)->i64,x:i64)->i64=>f(x)
fn finish(s:Sample)->Sample=>Sample{...s,total:s.total+s.count}
fn process(f:fn(Sample)->Sample,s:Sample)->Sample=>f(s)
fn run(stages:[fn(i64)->i64;2],start:i64)->i64 {
    var value=start
    for i in 0..2 {value=stages[i](value)}
    return value
}
fn result()->i64 {
    var selected=choose(true)
    let first=apply(selected,5)
    selected=double
    var stage=Stage{transform:selected}
    let second=stage.transform(first)
    stage.transform=add
    var stages:[fn(i64)->i64;2]=[stage.transform,selected]
    stages[1]=choose(false)
    let sample=process(finish,Sample{total:run(stages,second),count:3})
    let pair=(selected,stage.transform)
    return sample.total*100+pair.0(4)+pair.1(2)
}
