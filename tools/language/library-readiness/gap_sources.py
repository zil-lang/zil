"""Frozen source inputs for the bounded language readiness replay.

These strings are unchanged inputs, not a new result.
"""
gaps={
'nested-owning-callback':'resource R{n:i32} fn main()->i32{let a=R{n:3};let f=move()->i32=>{let g=move()->i32=>a.n;return g()};return f()-3}',
'owned-array-elements':'resource R{n:i32} fn main()->i32{let a:[R;2]=[R{n:1},R{n:2}];return 0}',
'owned-error-result':'enum E{Fail} fn make()->E!ByteBuffer contract{result(give)}{return bytes_new()} fn main(){}',
'generic-trait-bound':'trait Read{fn read(self:&Self)->i32;}struct C{x:i32}impl Read for C{fn read()->i32=>.x}fn read<T:Read>(x:&T)->i32=>x.read();fn main(){}',
'owning-method-receiver':'trait Read{fn read(self:&Self)->i32;}resource C{x:i32}impl Read for C{fn read()->i32=>.x}fn main(){}',
'array-of-capturing-callbacks':'fn make(n:i32)->impl FnMut()->i32{var x=n;return move()->i32=>{x+=1;return x}}fn main(){let handlers=[make(1),make(2)]}'
}
