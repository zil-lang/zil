#!/usr/bin/env python3
"""Independent source probes for task163; no production source modifications."""
import argparse
import hashlib
import json
import pathlib
import subprocess

PARSER = argparse.ArgumentParser()
PARSER.add_argument('--cli', default='/workspace/scratch/39be3faa4442/targets/review163/debug/zil')
PARSER.add_argument('--out', default='/workspace/scratch/39be3faa4442/run-logs/review163/standalone')
PARSER.add_argument('--only', default='')
PARSER.add_argument('--combo', action='store_true')
ARGS = PARSER.parse_args()
ROOT = pathlib.Path(ARGS.out)
ROOT.mkdir(parents=True, exist_ok=True)

RESOURCE = '''extern "C" fn putchar(x:i32)->i32;
resource R{code:i32}impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}
'''
POSITIVE = {
 'package_refill_loop_cleanup': ({'main': RESOURCE + '''package P{manages a:R;manages b:R;n:i32}
impl P{fn finish(self:Self)->i32=>.n}
fn make()->impl FnMut()->i32{var p=P{a:R{code:65},b:R{code:66},n:1};return move()=>{let n=p.finish();p=P{a:R{code:67},b:R{code:68},n:n+1};return n}}
fn main()->i32{var f=make();var sum=0;for i in 0..2{sum+=f()};return sum-3}'''.strip()}, b'ABCDCD', 0),
 'local_and_imported_resource_identity': ({
   'main': '''import model;resource R{n:i32}impl R(self:Self){fn read()->i32=>.n+100;fn finish(give self:Self)->i32=>.n+100}
fn main()->i32{let a=R{n:1};let b=model.make();if a.read()!=101||b.read()!=42{return 1};return (give b).finish()-42}''',
   'model': '''pub resource R{n:i32}impl R(self:Self){pub fn read()->i32=>.n;pub fn finish(give self:Self)->i32=>.n}pub fn make()->R contract{result(create)}{return R{n:42}}''',
 }, b'', 0),
 'local_and_imported_package_identity': ({
   'main': '''import model;resource R{n:i32}package P{manages r:R}impl P{fn finish(self:Self)->i32=>.r.n+100}
fn main()->i32{let a=P{r:R{n:1}};let b=model.make();return a.finish()+b.finish()-143}''',
   'model': '''pub resource R{n:i32}pub package P{manages r:R}impl P{pub fn finish(self:Self)->i32=>.r.n}pub fn make()->P=>P{r:R{n:42}}''',
 }, b'', 0),
 'method_effect_worklist_with_alias_arguments': ({'main': '''resource R{n:i32}
impl R(self:Self){fn update(other:R){other.n+=1};fn read()->i32=>.n}
''' + '\n'.join(f'fn step{i}(a:R,b:R){{step{i+1}(a,b)}}' for i in range(17)) + '''
fn step17(a:R,b:R){let alias=a;alias.update(b)}
fn make()->impl FnMut()->i32{let a=R{n:0};let b=R{n:40};return move()=>{step0(a,b);return a.read()+b.read()}}
fn main()->i32{var f=make();if f()!=41||f()!=42{return 1};return 0}'''}, b'', 0),
 'same_origin_nominal_aliases_are_readonly': ({'main': '''resource R{n:i32}impl R(self:Self){fn read()->i32=>.n}
fn select(a:R,b:R,flag:bool)->i32{let chosen=if flag{a}else{b};return chosen.read()}
fn make()->impl Fn(bool)->i32{let a=R{n:20};let b=R{n:22};return move(flag:bool)=>select(a,b,flag)}
fn main()->i32{let f=make();return f(true)+f(false)-42}'''}, b'', 0),
 'bytebuffer_local_methods_do_not_import_extensions': ({
   'main': '''import extension;impl ByteBuffer(self:Self){fn size()->i64=>bytes_len(self);fn append(v:u8)->bool=>bytes_push(self,v)}
resource R{n:i32}impl R(self:Self){fn size()->i64=>100}
fn main()->i32{let b=bytes_new();bytes_reserve(b,4);b.append(42u8);return i32(b.size())-1}''',
   'extension': '''impl ByteBuffer(self:Self){pub fn size()->i64=>99}''',
 }, b'', 0),
 'receiver_and_argument_debts_precede_unrelated_local_cleanup': ({'main': RESOURCE + '''enum E{Fail}
impl R{fn finish(give self:Self,give other:R,n:i32)->i32=>.code+other.code+n}
fn fail()->E!i32=>E.Fail
fn make()->impl FnOnce()->E!i32{let a=R{code:65};return move()=>{let b=R{code:66};let c=R{code:67};return (give a).finish(give b,fail()?)}}
fn main()->E!i32{let f=make();return f()?}'''}, b'BAC', 1),
 'readonly_callable_field_method_argument': ({'main': '''fn answer(n:i32)->i32=>n+1
resource R{call:fn(i32)->i32;n:i32}
fn read(r:R)->i32=>r.call(r.n)
fn make()->impl Fn()->i32{let r=R{call:answer,n:41};return move()=>read(r)}
fn main()->i32{let f=make();return f()+f()-84}'''}, b'', 0),
 'package_callable_copy_field_remains_shared_fn': ({'main': '''extern "C" fn putchar(x:i32)->i32;
fn answer(n:i32)->i32=>n+1
resource R{n:i32}impl Drop for R{fn drop(self:Self){unsafe{putchar(65)}}}
package P{manages r:R;call:fn(i32)->i32}
fn make()->impl Fn()->i32{let p=P{r:R{n:41},call:answer};return move()=>p.call(p.r.n)}
fn main()->i32{let f=make();return f()+f()-84}'''}, b'A', 0),
}
NEGATIVE = {
 'imported_builtin_extension_is_not_ambient': {
   'main': 'import extension;fn main()->i32{let b=bytes_new();return i32(b.size())}',
   'extension': 'impl ByteBuffer(self:Self){pub fn size()->i64=>bytes_len(self)}',
 },
 'borrowed_descriptor_cannot_feed_give_receiver': {'main': '''resource R{n:i32}impl R{fn finish(give self:Self)->i32=>.n}
fn main()->i32{let r=R{n:42};let alias=r;return (give alias).finish()}'''},
 'old_byte_view_after_method_effect_chain': {'main': '''impl ByteBuffer(self:Self){fn grow()->bool=>bytes_reserve(self,64);fn through()->bool=>self.grow()}
fn wrapper(b:ByteBuffer){b.through()}
fn main()->i32{let b=bytes_new();bytes_reserve(b,8);bytes_push(b,42u8);let view=bytes_view(b);wrapper(b);return i32(view[0])}'''},
 'old_package_byte_view_after_owned_method': {'main': '''package P{manages b:ByteBuffer}impl P{fn finish(self:Self)->i64=>bytes_len(.b)}
fn main()->i32{let b=bytes_new();bytes_reserve(b,8);bytes_push(b,42u8);let p=P{b:give b};let view=bytes_view(p.b);p.finish();return i32(view[0])}'''},
 'private_resource_method_through_factory': {
   'main': 'import model;fn main()->i32{let r=model.make();return r.hidden()}',
   'model': 'pub resource R{n:i32}impl R(self:Self){fn hidden()->i32=>.n}pub fn make()->R contract{result(create)}{return R{n:42}}',
 },
 'private_package_method_through_factory': {
   'main': 'import model;fn main()->i32{let p=model.make();return p.hidden()}',
   'model': 'resource R{n:i32}pub package P{manages r:R}impl P{fn hidden(self:Self)->i32=>.r.n}pub fn make()->P=>P{r:R{n:42}}',
 },
 'nominal_wrong_static_receiver_cannot_pass_by_layout': {
   'main': 'import model;resource R{n:i32}fn main()->i32{let r=R{n:42};return model.read(r)}',
   'model': 'pub resource R{n:i32}pub fn read(r:R)->i32=>r.n',
 },
 'owned_mutation_cannot_promise_shared_fn': {'main': '''resource R{n:i32}impl R(self:Self){fn set(n:i32){.n=n}}
fn make()->impl Fn()->i32{let r=R{n:0};return move()=>{r.set(42);return r.n}}fn main(){}'''},
}

if ARGS.combo:
    POSITIVE = {
      'nested_borrowed_resource_methods_update_owned_outer': ({'main': RESOURCE + '''
impl R(self:Self){fn add(n:i32){.code+=n};fn read()->i32=>.code}
fn make()->impl FnMut(i32)->i32{let r=R{code:40};return move(n:i32)=>{let inner=(delta:i32)=>{r.add(delta);return r.read()};return inner(n)}}
fn main()->i32{var f=make();if f(1)!=41||f(1)!=42{return 1};return 0}'''}, b'*', 0),
      'three_levels_owned_resource_method_transfer': ({'main': RESOURCE + '''
impl R{fn finish(give self:Self)->i32=>.code}
fn make()->impl FnOnce()->i32{let r=R{code:65};return()=>{let middle=()=>{let inner=move()=>(give r).finish();return inner()};return middle()}}
fn main()->i32{let f=make();return f()-65}'''}, b'A', 0),
      'nested_package_method_consumption_then_outer_refill': ({'main': RESOURCE + '''
package P{manages a:R;n:i32}impl P{fn finish(self:Self)->i32=>.n}
fn make()->impl FnMut()->i32{var p=P{a:R{code:65},n:1};return move()=>{let inner=move()=>p.finish();let n=inner();p=P{a:R{code:66},n:n+1};return n}}
fn main()->i32{var f=make();return f()+f()-3}'''}, b'ABB', 0),
      'inner_package_refill_repeatable_outer_creation_once': ({'main': RESOURCE + '''
package P{manages a:R;n:i32}impl P{fn finish(self:Self)->i32=>.n}
fn make()->impl FnOnce()->i32{var p=P{a:R{code:65},n:1};return move()=>{var inner=move()=>{let n=p.finish();p=P{a:R{code:66},n:n+1};return n};return inner()+inner()}}
fn main()->i32{let f=make();return f()-3}'''}, b'ABB', 0),
      'nested_owned_receiver_preparation_error_cleanup': ({'main': RESOURCE + '''
enum E{Fail}impl R{fn finish(give self:Self,give other:R,n:i32)->i32=>.code+other.code+n}
fn fail()->E!i32=>E.Fail
fn make()->impl FnOnce()->E!i32{let a=R{code:65};return move()=>{let b=R{code:66};let inner=move()->E!i32=>{let c=R{code:67};return (give a).finish(give b,fail()?)};return inner()?}}
fn main()->E!i32{let f=make();return f()?}'''}, b'BAC', 1),
    }
    NEGATIVE = {
      'nested_method_write_cannot_promise_fn': {'main': '''resource R{n:i32}impl R(self:Self){fn add(){.n+=1}}
fn make()->impl Fn()->i32{let r=R{n:0};return move()=>{let inner=()=>r.add();inner();return r.n}}fn main(){}'''},
      'old_descriptor_after_nested_owned_method': {'main': '''resource R{n:i32}impl R{fn finish(give self:Self)->i32=>.n}
fn main()->i32{let r=R{n:42};let alias=r;let outer=()=>{let inner=move()=>(give r).finish();return inner()};let n=outer();return alias.n-n}'''},
      'nested_package_conditional_refill_cannot_promise_fnmut': {'main': '''resource R{n:i32}package P{manages r:R}impl P{fn finish(self:Self)->i32=>.r.n}
fn make()->impl FnMut(bool)->i32{var p=P{r:R{n:42}};return move(flag:bool)=>{let inner=move()=>p.finish();let n=inner();if flag{p=P{r:R{n:n+1}}};return n}}fn main(){}'''},
    }

ROWS = []
def prepare(name, files):
    directory = ROOT / name
    directory.mkdir(parents=True, exist_ok=True)
    for module, text in files.items():
        (directory / (module + '.zil')).write_text(text + '\n')
    return directory

for name, (files, stdout, exitcode) in POSITIVE.items():
    if ARGS.only and ARGS.only not in name: continue
    directory = prepare(name, files)
    for opt in ('O0', 'O2'):
        exe = directory / opt
        command = [ARGS.cli, 'build', str(directory / 'main.zil'), '--opt', opt, '-o', str(exe), '--emit-dir', str(directory / (opt + '-phases')), '--json']
        result = subprocess.run(command, capture_output=True, timeout=60)
        (directory / (opt + '-build.stdout')).write_bytes(result.stdout)
        (directory / (opt + '-build.stderr')).write_bytes(result.stderr)
        report = json.loads(result.stdout)
        row = {'name': name, 'optimization': opt, 'kind': 'positive', 'command': command, 'status': report['status'], 'phase': report['phase'], 'diagnostics': report['diagnostics'], 'verification_runs': report['verification_runs']}
        if result.returncode == 0 and report['status'] == 'Success':
            native = subprocess.run([str(exe)], capture_output=True, timeout=15)
            (directory / (opt + '-stdout')).write_bytes(native.stdout)
            (directory / (opt + '-stderr')).write_bytes(native.stderr)
            row.update(exit_code=native.returncode, stdout=native.stdout.decode(errors='replace'), pass_result=native.returncode == exitcode and native.stdout == stdout and report['verification_runs'] == 1)
        else:
            row['pass_result'] = False
        ROWS.append(row)
        print(json.dumps(row), flush=True)

for name, files in NEGATIVE.items():
    if ARGS.only and ARGS.only not in name: continue
    directory = prepare(name, files)
    command = [ARGS.cli, 'check', str(directory / 'main.zil'), '--emit-dir', str(directory / 'phases'), '--json']
    result = subprocess.run(command, capture_output=True, timeout=60)
    (directory / 'check.stdout').write_bytes(result.stdout)
    (directory / 'check.stderr').write_bytes(result.stderr)
    report = json.loads(result.stdout)
    row = {'name': name, 'kind': 'negative', 'command': command, 'status': report['status'], 'phase': report['phase'], 'diagnostics': report['diagnostics'], 'pass_result': result.returncode != 0 and report['status'] in ('SafeReject', 'Unsupported') and report['phase'] != 'parse'}
    ROWS.append(row)
    print(json.dumps(row), flush=True)

(ROOT / 'results.json').write_text(json.dumps(ROWS, indent=2) + '\n')
assert all(row['pass_result'] for row in ROWS), 'independent source probes failed; see results.json'
