#!/usr/bin/env python3
"""Deterministic source generators, not hand-written IR or mathematical graphs."""
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
OUT = ROOT / 'tests/perf/impl055'
HEADER = 'resource Token { value:i32 }\nfn read(t:Token)->i32 {return t.value}\n'

def long_segment(n):
    lines = [HEADER, 'fn main()->i32 {', 'let t=Token{value:7}; let alias=t; var n:i32=0;']
    lines += ['n=n+1;' for _ in range(n)]
    return '\n'.join(lines + ['return read(alias)+n;', '}']) + '\n'

def branches(n):
    lines = [HEADER, 'fn examine(flag:bool)->i32 {', 'let t=Token{value:7}; let alias=t; var n:i32=0;']
    lines += [f'if flag {{n+=read(alias)+{i}}} else {{n+={i}}}' for i in range(n)]
    return '\n'.join(lines + ['return n;', '}', 'fn main()->i32 {return examine(true)}']) + '\n'

def wide(n):
    lines = [HEADER, 'fn main()->i32 {']
    lines += [f'let t{i}=Token{{value:{i}}};' for i in range(n)]
    lines += ['let read_all=()=>'+ '+'.join(f't{i}.value' for i in range(n))+';', 'var n:i32=0;']
    lines += ['n+=1;' for _ in range(32)]
    return '\n'.join(lines+['return read_all()+n;', '}'])+'\n'

def main():
    OUT.mkdir(parents=True, exist_ok=True)
    generated = {'tiny.zil':'fn main()->i32 {return 7}\n',
                 **{f'straight_{n}.zil':long_segment(n) for n in [16,128,512]},
                 **{f'branches_{n}.zil':branches(n) for n in [4,32]},
                 **{f'wide_{n}.zil':wide(n) for n in [4,16,64]},
                 'correlation.zil': 'resource T{n:i32}\nenum E{Ok(i32),Err(i32)}\nfn f(flag:bool)->E{let a=T{n:1};let b=T{n:2};let fa=()=>a.n;let fb=()=>b.n;if flag{return E.Ok(fa())}return E.Err(fb())}\nfn main()->i32{return 0}\n',
                 'exits.zil': HEADER+'enum Result {Ok(i32), Err(i32)}\nfn transaction(flag:bool)->Result {let t=Token{value:7};let f=()=>t.value;if flag {return Result.Err(f())}return Result.Ok(f())}\nfn main()->i32 {return 0}\n',
                 'loop.zil':HEADER+'fn main()->i32 {let t=Token{value:7};let alias=t;var n:i32=0;while n<4 {n+=1;if n==2 {continue}if n==3 {break}read(alias)}return read(alias)+n}\n'}
    for name, text in generated.items(): (OUT/name).write_text(text)

if __name__ == '__main__': main()
