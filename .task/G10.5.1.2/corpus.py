#!/usr/bin/env python3
"""Paired mechanisms, not claimed to be ports of complete upstream applications."""
from pathlib import Path

UPSTREAM='https://github.com/rust-lang/rustc-perf/tree/c0f257113ccc6e6e0742b318006eef2787c3d4c5/collector/compile-benchmarks'
SPECS=[('ring768','graph',768),('shared1024','graph',1024),('diamond768','graph',768),('trait32','trait',32),('trait128','trait',128),('owned8','resource',8),('owned32','resource',32)]
GROUP_WEIGHTS={'graph':.45,'trait':.25,'resource':.30}

def io_source(lang):
    if lang=='zil':
        return '''extern "C" fn getchar()->i32;
extern "C" fn putchar(x:i32)->i32;
fn read()->i64 { unsafe { return i64(getchar()); } }
fn write(x:i64) { unsafe { putchar(i32(x%256)); putchar(i32(x/256)); } }
fn main()->i32 {
    let seed=read(); let depth=read(); let high=read(); let low=read();
    if seed<0 || depth<0 || high<0 || low<0 {return 2;}
    var acc:i64=seed; var i:i64=0; let count=high*256+low;
    while i<count { acc=(run((acc+i)%65521,i32(depth%12+1))+i)%65521; i+=1; }
    write(acc); return 0;
}
'''
    return '''unsafe extern "C" { fn getchar()->i32; fn putchar(x:i32)->i32; }
fn read()->i64 { unsafe { i64::from(getchar()) } }
fn write(x:i64) { unsafe { putchar((x%256) as i32); putchar((x/256) as i32); } }
fn main() { let seed=read(); let depth=read(); let high=read(); let low=read();
    if seed<0 || depth<0 || high<0 || low<0 {std::process::exit(2);}
    let mut acc:i64=seed; let mut i:i64=0; let count=high*256+low;
    while i<count { acc=(run((acc+i)%65521,(depth%12+1) as i32)+i)%65521; i+=1; }
    write(acc);
}
'''

def graph(name,n,lang,edit):
    rows=[]
    for i in range(n):
        bias=1+i%97+(4 if edit and i==0 else 0)
        if name.startswith('ring'): targets=[(i+1)%n]
        elif name.startswith('diamond'): targets=[] if i>=n//2 else [min(n-1,2*i+1),min(n-1,2*i+2)]
        else: targets=[] if i>=n//2 else [n//2+i%(n//2),n//2+(i*17+1)%(n//2)]
        body=f'if d<=0 {{return (x+{bias})%65521;}} '
        if targets:
            body+=f'let a=f{targets[0]}((x*x+{bias})%65521,d-1); '
            if len(targets)==2: body+=f'let b=f{targets[1]}((a+x+3)%65521,d-1); return (b*31+x+{bias})%65521;'
            else: body+=f'return (a*31+x+{bias})%65521;'
        else: body+=f'return (x*x+{bias})%65521;'
        rows.append(f'fn f{i}(x:i64,d:i32)->i64 {{ {body} }}\n')
    mut='var' if lang=='zil' else 'let mut'
    # Every function is explicitly called, in addition to graph edges. The
    # input-driven nonlinear recurrence prevents constant/affine collapse.
    run=f'fn run(x:i64,d:i32)->i64 {{ {mut} y=x; '
    for i in range(n): run+=f'y=f{i}(y,d); '
    return ''.join(rows)+run+'return y; }\n'

def traits(n,lang,edit):
    rows=[]
    if lang=='zil':
        rows=['trait Eval { fn eval(self:&Self,x:i64)->i64; }\n','fn apply<T:Eval>(v:&T,x:i64)->i64 {return v.eval(x);}\n']
    else:
        rows=['trait Eval { fn eval(&self,x:i64)->i64; }\n','fn apply<T:Eval>(v:&T,x:i64)->i64 {v.eval(x)}\n']
    for i in range(n):
        recv='self:&Self' if lang=='zil' else '&self'
        bias=i+1+(4 if edit and i==0 else 0)
        rows.append(f'struct K{i} {{ bias:i64 }}\nimpl Eval for K{i} {{ fn eval({recv},x:i64)->i64 {{return (x*x+self.bias+{bias})%65521;}} }}\n')
    mut='var' if lang=='zil' else 'let mut'
    rows.append(f'fn run(x:i64,d:i32)->i64 {{ {mut} y=x; ')
    for i in range(n): rows.append(f'let v{i}=K{i}{{bias:{i+1}}}; y=apply(&v{i},y); ')
    rows.append('return (y+i64(d))%65521; }\n' if lang=='zil' else 'return (y+i64::from(d))%65521; }\n')
    return ''.join(rows)

def resources(n,lang,edit):
    rows=[]
    if lang=='zil':
        rows=['''package State { manages data:ByteBuffer; sum:i64 }
fn create()->ByteBuffer contract { result(create) } {return bytes_new();}
fn append(value:ByteBuffer,byte:u8)->bool {
    let length=bytes_len(value); let capacity=bytes_capacity(value);
    if length==capacity {var next:i64=8; if capacity>0 {next=capacity*2;} if !bytes_reserve(value,next){return false;}}
    return bytes_push(value,byte);
}
''']
        for i in range(n):
            bias=i+1+(4 if edit and i==0 else 0)
            rows.append(f'''fn make{i}()->impl FnMut(u8)->i64 {{
    var state=State{{data:create(),sum:i64(0)}};
    return move(byte)=>{{
        if byte==0u8 {{state.data=create();state.sum=0;return i64(0);}}
        if !append(state.data,byte) {{return i64(-1);}}
        state.sum+=i64(byte);return (state.sum+bytes_len(state.data)+{bias})%251;
    }};
}}
''')
    else:
        for i in range(n):
            bias=i+1+(4 if edit and i==0 else 0)
            rows.append(f'''fn make{i}()->impl FnMut(u8)->i64 {{
    let mut data=Vec::<u8>::new();let mut sum:i64=0;
    move |byte| {{if byte==0 {{data=Vec::new();sum=0;return 0;}}
        data.push(byte);sum+=i64::from(byte);(sum+data.len() as i64+{bias})%251}}
}}
''')
    mut='var' if lang=='zil' else 'let mut'
    rows.append('fn run(x:i64,d:i32)->i64 { ')
    for i in range(n):rows.append(f'{mut} c{i}=make{i}(); ')
    rows.append(f'{mut} y=x%251; {mut} j:i32=0; while j<d {{ ')
    for i in range(n):
        arg=f'u8((y+{i})%251)' if lang=='zil' else f'((y+{i})%251) as u8'
        rows.append(f'y=c{i}({arg}); if y<0 {{return 65520;}} ')
    rows.append('j+=1; } return y; }\n')
    return ''.join(rows)

def run_model(name,n,x,d,edit):
    if name.startswith('trait'):
        for i in range(n):x=(x*x+2*(i+1)+(4 if edit and i==0 else 0))%65521
        return (x+d)%65521
    if name.startswith('owned'):
        sums=[0]*n;lens=[0]*n;y=x%251
        for _ in range(d):
            for i in range(n):
                b=(y+i)%251
                if b==0:sums[i]=lens[i]=0;y=0
                else:
                    sums[i]+=b;lens[i]+=1;y=(sums[i]+lens[i]+i+1+(4 if edit and i==0 else 0))%251
        return y
    def f(i,x,d):
        bias=1+i%97+(4 if edit and i==0 else 0)
        if d<=0:return (x+bias)%65521
        if name.startswith('ring'):return (f((i+1)%n,(x*x+bias)%65521,d-1)*31+x+bias)%65521
        if i>=n//2:return (x*x+bias)%65521
        a,b=(min(n-1,2*i+1),min(n-1,2*i+2)) if name.startswith('diamond') else (n//2+i%(n//2),n//2+(i*17+1)%(n//2))
        av=f(a,(x*x+bias)%65521,d-1)
        return (f(b,(av+x+3)%65521,d-1)*31+x+bias)%65521
    for i in range(n):x=f(i,x,d)
    return x

def expected(name,n,snapshot,data):
    if len(data)<4:return b''
    x,depth,hi,lo=data[:4];d=depth%12+1
    for i in range(hi*256+lo):x=(run_model(name,n,(x+i)%65521,d,snapshot=='private')+i)%65521
    return bytes([x%256,x//256])

def generate(root):
    root=Path(root); cases=[]
    for name,group,n in SPECS:
        case={'id':name,'group':group,'n':n,'weight':GROUP_WEIGHTS[group]/sum(g==group for _,g,_ in SPECS),
            'kind':'mechanism-preserving synthetic workload, not full upstream port',
            'provenance':UPSTREAM+('/bitmaps-3.2.1' if group=='trait' else '/large-workspace' if group=='graph' else '/serde-1.0.219'),
            'retained':'runtime input, nonlinear calls, all generated definitions directly reachable; trait instantiations or owning mutable closures where labelled',
            'not_retained':'upstream public API, exact data structures, macro system, dependency graph and source size'}
        if group=='resource':case['implementation_basis']='zil examples/mainline/byte_buffer_owned at 82a69812582856b012468db3d019930d8e390f64; actual owned buffers, replacement, allocation and cleanup; not serde'
        for lang in ('zil','rust'):
            for snap in ('base','private'):
                out=root/name/snap/lang;out.mkdir(parents=True)
                body=graph(name,n,lang,snap=='private') if group=='graph' else traits(n,lang,snap=='private') if group=='trait' else resources(n,lang,snap=='private')
                (out/('main.rs' if lang=='rust' else 'main.zil')).write_text(body+io_source(lang))
        cases.append(case)
    return cases
