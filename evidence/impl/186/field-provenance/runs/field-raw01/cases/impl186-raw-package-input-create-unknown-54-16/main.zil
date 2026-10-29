
resource R{n:i32}package P{manages r:R;pub n:i32}
package Pair{manages old:R;manages fresh:R}enum E{Fail}
fn package_input(p:P)->R contract{result(give)}{let moved=p;return give moved.r}
fn construct_old(give old:R)->R contract{result(give)}{let p=P{r:give old,n:7};let moved=p;return give moved.r}
fn replace_old(give old:R)->R contract{result(give)}{var r:R=R{n:1};r=give old;return give r}
fn mixed(give old:R,flag:bool)->E!R contract{result(give)}{var r:R=R{n:1};if flag{r=give old};return give r}
fn unknown_package()->P=>P{r:R{n:1},n:7}
fn unknown()->R contract{result(give)}{let p=unknown_package();return give p.r}
fn fields(give old:R)->E!R contract{result(create)}{let p=Pair{old:give old,fresh:R{n:2}};let moved=p;return give moved.fresh}
fn main(){}
