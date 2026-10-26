# ZIL 收尾语法速读 · draft08

本页以当前可运行用法为主。规范由 `_zil_design.md` 及具名附件定义；精确实现提交、审核和执行成绩见 [施工状态](IMPLEMENTATION-STATUS.md) 与[本轮报告](../evidence/REPORT.md)。当前语言保持 1.3/A，内部 schema/wire 为 5；完整语言仍 NotSatisfied，产品门 NotRun，B0 Doing。

## 1. 从一个完整程序开始

[动态整数流应用](../examples/mainline/stream/README.md) 从真实 stdin 读入数据，经五个模块的公开 Option/Result、泛型函数、独占状态更新和默认写捕获处理，再通过显式 C ABI 封装输出：

```sh
cargo build --offline --locked -p zil-cli
printf '10 20 -3\n' | ./target/debug/zil run examples/mainline/stream/main.zil
```

输出为 `3 0 45`，分别是有效 token 数、无效 token 数和变换后的总和。普通 `run` 与 `run --json` 都转交 stdin；JSON 模式将程序输出放在 `run.stdout`。本例使用普通 `main` 和真实 I/O，其输入边界与 Rust 对照见同目录说明。

[多模块批数据应用](../examples/mainline/app/README.md) 继续覆盖目录导入、CT record/array、用户泛型 enum、共享投影与返回、函数指针数组、未注解高阶闭包、FnMut 和独立环境副本：

```sh
./target/debug/zil run examples/mainline/app/main.zil --entry result
./target/debug/zil run examples/mainline/app/main.zil
```

完整结果为 `475000738663`；普通 `main` 自检退出 0。相同操作顺序的 Rust 对照保存在同目录，O0/O2 行为已对照，不据此声称性能领先。`--entry` 用原生测试 wrapper 输出返回值，不是语言内置打印函数。

## 2. 值、聚合与接收者

当前标量为 `i32`、`i64`、`bool`、`()`；`let` 不可写，`var` 可写。普通有符号算术检查溢出；`+%`、`-%`、`*%` 是显式 wrapping。分支、循环、递归、短路逻辑和整数范围 `for n in lo..hi` 已进入原生链。

有限 Copy struct、tuple 和定长 array 支持构造、值传参/返回、投影读取与赋值。动态索引检查负数和上界。名义 struct 不因字段布局相同而相等。

```zil
struct Point { x: i64; y: i64 }
impl Point(self: &Self) {
    fn sum() -> i64 => .x + .y
    fn shifted(dx: i64) -> Point => Point { x: .x + dx, y: .y }
}
impl Point {
    fn origin() -> Self => Point { x: 0, y: 0 }
}
```

组默认补首参数；`.x` 与 `self.x` 使用同一 receiver。值接收者、静态方法、`&Self` 与 `&mut Self` 接收者已接通。对已有位置的 `p.sum()` 可按声明自动共享借入一次；独占接收者要求对应的可写位置与访问资格。非位置临时接收者的寿命规则未全部实现，资源入口 `Ref<Self>` 与非 Copy 清理仍属于后续权限链。

Copy 记录更新已可执行：

```zil
fn shifted(p: Point) -> Point {
    return Point { ...p, x: p.x + 1 }
}
```

base 和覆盖表达式遵循源码顺序，投影赋值的索引只求值一次。非 Copy base 的移动、覆盖失败清理与自定义 teardown 仍未完成；不能从当前 Copy 机器布局推出它们已经支持。

## 3. 地址：共享返回、独占写入与重借

```zil
fn choose(a: &i64, b: &i64, flag: bool) -> &i64 {
    if flag { return a }
    return b
}
fn read(value: &i64) -> i64 => *value
```

实现从正文推导返回值可能来自 `a` 或 `b`，调用时替换为本次实参来源；无需为本例增加生命周期注记。返回来源独立重算，分支合并不根据未知 flag 猜唯一来源。局部存储和按值参数槽的地址不能逃逸；没有落地来源的递归被拒绝。

一层 `&mut` 到有限 Copy 值已支持跨函数调用、真实间接写和共享/独占重借。[impl042](../examples/impl042/main.zil) 的原生例子返回 42：

```zil
fn bump(p: &mut i32) { *p = *p + 1; }
fn answer() -> i32 {
    var x = 40;
    bump(&mut x);
    let p = &mut x;
    let q = &mut *p;
    *q = *q + 1;
    return x;
}
```

子窗口仍活跃时，父方相交访问受限；最后使用后恢复可用资格。已有独占地址的按值赋值会转交资格，例如 `let q = p` 后不能再用原绑定 `p`。直接函数调用按已解析的地址参数建立相应重借；可返回有已核参数来源的独占地址，局部地址仍不能逃逸。

共享与独占目标均限有限 Copy 值，包括已准入聚合。`&record.field`、`&mut tuple.0`、`&mut array[index]` 和 `&*p` 支持真实地址投影；地址读取字段/索引可自动解引用一层。投影仍保守保留整个对象 Hold 根，所以不能以两个不同字段名字证明独占窗口不相交。嵌套地址、地址字段和资源目标仍未接通。

## 4. 闭包：共享捕获、外部独占捕获与 owned FnMut

```zil
let increment = x => x + 1
let explicit = (x: i32) -> i32 => x + 1
let magnitude = (x: i32) => {
    if x < 0 { 0 - x } else { x }
}
let discard = () => { 42; } // Unit
```

参数与结果有唯一约束时可以省略注记；一个闭包仍只有一个固定签名，不猜多个候选直到安全检查成功。带类型的单参数写 `(x: T)`；零个或多个参数也用括号。

闭包块最后一个无分号表达式为结果，末尾 if/else 分支同理。真实分号丢弃值；继续路径为 Unit 时须与其它路径相容。普通具名函数块使用显式 `return`，箭头体可直接给表达式。

默认只读捕获使用实际共享地址，现已覆盖当前 Copy 聚合和函数指针；它们的所有副本最后使用后才能写原对象。`move` 保存 Copy 值的创建时快照、复制共享地址，或转交独占地址；地址的原存活上界保持不变。

默认修改外部变量会保存对应位置的独占地址。下面是 [mainline_stream 回归](../crates/zil-driver/tests/mainline_stream.rs) 中同时捕获可变 `x` 与只读 `y` 的例子，返回 15：

```zil
fn answer() -> i32 {
    var x = 1
    let y = 2
    var f = () => { x = x + y; x }
    let a = f()
    let b = f()
    return a + b + x + y
}
```

`f` 的两次调用将外部 `x` 依次改为 3、5；最后调用后可以读取 `x`。提前插入相交读写仍会被拒绝。含独占捕获的环境不能复制，`var g = f` 会转交环境并使 `f` 不再可用。调用 FnMut 需要可写闭包位置。

下面是已实现的 owned 环境 FnMut：

```zil
fn result() -> i64 {
    var total: i64 = 0
    var add = move x => { total += x; total }
    let first = add(2)
    var copy = add
    let second = add(3)
    let independent = copy(8)
    return first * 10000 + second * 100 + independent
}
```

`add` 自己的环境依次为 2、5；`copy` 从复制时的 2 独立增加到 10。调用修改真实闭包位置，需要 `var`；不能复制临时环境后丢写入，也不能绕过 `let`。更完整的 record/array/tuple 环境见 [impl030](../examples/impl030/main.zil)。

已有 `&mut` 的默认捕获按正文用途建立共享或独占子窗口；`move` 捕获则转交该地址。目前含独占地址的 `move` 环境即使正文只读，也保守要求 FnMut 和 `var`，完整只读调用视图仍是接受性缺口。FnOnce 的非 Copy 消费、资源清理、捕获已有闭包环境和一般带环境逃逸尚未完成。

### 同一闭包中混合共享与快照

无需新增逐项捕获语法，可先建立引用，再整体 `move`：

```zil
var source: i64 = 40
let borrowed = &source
var snapshot: i64 = 2
let mixed = move () => *borrowed + snapshot
let copied = mixed
snapshot = 99
let first = mixed() + copied()
source = 100 // 所有共享副本最后使用后
```

`mixed`/`copied` 保存原指针和数值 2；不会复制 `source`，也不会延长其存储期。把最后一行移到调用前会违反活跃借入。完整 [mixed-captures](../examples/mixed-captures/main.zil) 输出 283。普通 `() => *borrowed` 也可按用途借用已有地址；重新绑定被捕获的地址仍需要尚未实现的嵌套地址窗口。

## 5. 函数指针和无捕获 lambda

```zil
fn apply(f: fn(i64) -> i64, x: i64) -> i64 => f(x)
fn choose(flag: bool) -> fn(i64) -> i64 =>
    if flag { x => x + 1 } else { x => x * 2 }

fn result() -> i64 {
    let callbacks: [fn(i64) -> i64; 2] = [x => x + 2, x => x * 2]
    let local_apply = (f, x) => f(x)
    return local_apply(callbacks[0], 20) + apply(choose(false), 10)
}
```

`fn(A)->R` 是没有环境的薄函数指针。具名函数、函数工厂返回、record/tuple/array 中的函数值都可间接调用；省略注记的局部高阶闭包可从实际函数指针约束推导完整签名。

明确 `fn` 上下文中的无捕获 lambda 生成真实普通函数入口，适用于绑定、赋值、参数、返回、聚合和分支。带词法捕获的 lambda 转换报 `E-CAPTURED-FN-POINTER`；不隐式装箱、擦除环境或建立 trampoline。既有具体闭包值再转换为指针尚未开放。

当前间接签名限有限纯 Copy 参数/结果，不能含地址或资源；直接函数调用的共享/独占借入合同另行支持。显式薄函数内再创建无捕获薄函数可以运行；一般嵌套带环境闭包和 `impl Fn`/dyn 返回不是同一个能力。

`Fn(A)->R`、`FnMut(A)->R`、`FnOnce(A)->R` 是规范约束拼写；一般 trait/bound 求解仍未完成，不能因为局部调用工作就宣称这些通用接口全部实现。

## 6. 普通泛型、enum 与透明别名

```zil
enum Option<T> { None, Some(T) }
struct Cell<T> { value: T; }
type Row = Cell<i64>
type Maybe = Option<Row>

fn make() -> Maybe => Maybe.Some(Row { value: 42 })
fn result() -> i64 {
    if let Maybe.Some(row) = make() { return row.value }
    return 0
}
```

这些都是普通用户声明，`Option`/`Result` 名称没有内置能力。原生 enum 使用 tag 和有限载荷布局；支持 Unit/tuple 变体、`match` 穷尽与冗余检查，以及 `if let`/`while let`。条件值每次只求值一次，模式绑定只在成功分支可用，循环每次回边重新求条件。record 变体和借入/资源载荷尚未实现。

普通函数、struct 与 enum 可以使用类型参数，公开声明现已可跨模块导入。实例化前仍检查模板，包括未使用模板；未知 `T` 不从某次 `i64` 实例获得数值或 Copy 能力。唯一上下文不足时使用 `::<...>`，例如 `RowTemplate::<i64> { value: 42 }`；空载荷构造不能凭 enum 名猜类型。

例如把 [impl043](../examples/impl043/main.zil) 的公共 Option 和构造函数放在同目录的 `model.zil`：

```zil
pub enum Option<T> { None, Some(T) }
pub fn some<T>(value: T) -> Option<T> { return Option.Some(value) }
```

消费文件使用自然模块路径，下面的 `result` 返回 42：

```zil
import model

fn result() -> i64 {
    let n: i64 = 42
    let value: model.Option<i64> = model.some(n)
    match value {
        model.Option.Some(v) => { return v }
        model.Option.None() => { return 0 }
    }
}
```

模板实例保留定义模块作用域与名义身份；两个导入别名不会制造新类型，不同模块同名类型也不会合并。公开记录可包含私有字段并作为值跨模块传递，但外部私有字段访问或构造仍拒绝；通过定义模块的公开函数操作这些字段。

普通 `type Alias = Type` 是透明别名，不引入新名义身份，公开普通别名也可跨模块使用。前向引用允许，循环/重复/错误参数数量/未知目标会诊断，未用别名也检查。别名可用于签名、字段、构造、模式与方法接收者；`Option<i32>` 的别名模式不能匹配 `Option<i64>`。泛型形参与局部值的遮蔽分别处理，不靠文本替换。

泛型别名、带属性/bound 的别名、一般 where/trait/生命周期/const 约束和泛型 impl 仍未接通。跨模块构造目前不支持省略带默认值的字段，仍明确 Unsupported；完整显式构造继续检查字段可见性。更多程序见 [impl035](../examples/impl035/main.zil)、[impl039](../examples/impl039/main.zil) 与 [impl043](../examples/impl043/main.zil)。

## 7. 编译期表达式和管道

`#(...)` 可以执行已准入的纯直接调用、递归、分支、短路与循环，返回标量或有限 Copy tuple/array/record。主应用使用：

```zil
let data = #(seed.build())
let weights = #(seed.weights())
```

CT 的 raw 程序必须经过独立纯性与 CFG/类型核验，VM 只运行已核对象；结果再回到普通 Core 和正常检查链。整个项目累计步骤，同时约束深度、存活槽位与聚合展开量。越界/溢出产生诊断，预算耗尽为 ResourceLimit，不回退运行时；没有运行地址、资源、间接函数、enum、闭包、Type/Syntax、反射、生成或外部 I/O 权限。

管道保持一条规则：`left |> target` 先求 left，再求 target，各一次，再作一元调用。`x |> f(a)` 要求 `f(a)` 本身得到一元 callable，不自动插参。规范中的 `...tuple` 用于实参展开，但当前调用实参的 tuple spread 仍为 Unsupported；Copy tuple 的值与布局支持没有自动接通这条糖。记录更新的 `{ ...base, field: value }` 是另一条已实现的 Copy 记录路径。

## 8. 模块、原生 I/O 与候选语法

使用点路径 `import a.b as name`。入口所在目录是文件适配器导入根，`a.b` 对应 `a/b.zil` 或 `a/b/mod.zil`，两者只能存在一个。目录不自动合并子文件，必须显式 `mod.zil`；导入不执行顶层脚本。模块隐私与名义身份参与调用和类型检查。

原生 I/O 的当前入口是私有 `extern "C"` 声明与普通 ZIL 包装器。[流应用的 io.zil](../examples/mainline/stream/io.zil) 使用以下已测试代码：

```zil
extern "C" fn getchar() -> i32;
extern "C" fn putchar(value: i32) -> i32;

pub fn read() -> i32 {
    unsafe { return getchar() }
}

pub fn write(value: i32) -> i32 {
    unsafe { return putchar(value) }
}
```

当前 C ABI 支持 `i32`/`i64` 参数及 `i32`/`i64`/Unit 返回。外部声明保持私有，跨模块通过公开普通函数导出；外调必须位于调用函数自身的词法 `unsafe` 中，外围 `unsafe` 不替闭包正文授权。指针/聚合/可变参数 FFI、通用回调和完整 I/O 错误与资源模型尚未完成。`unsafe` 不取消普通类型、初始化与借用核验；CT 仍不允许外部 I/O。

问号目前保持 A 方案：`&T?` 是 `(&T)?`，对可空值的地址写 `&(T?)`。前缀 `?T` 与其它结合方案仍作全场景候选比较。普通用户 `Option<T>` 已有机器执行，不表示 `T?`、`?.`、`??` 或错误传播 `?` 已实现。`@` 与逐项捕获覆盖语法同样还在候选记录中，本轮没有用它们阻断主线。

## 9. 接续时如何判断“已完成”

完整资源/Desc/Managed、资源与并发上的独占权限、清理失败路径、一般捕获环境逃逸、trait 求解、挂起/并发、SABI 与完整元编程尚未完成。当前有限 Copy 目标的一层独占地址与局部捕获不能代替这些能力。不可达源码按规范仍须检查类型形状；普通 Builder 的部分路径对此仍报 Unsupported，应作为实现缺口修复。

真实验收需要同一源程序经过类型、候选、独立 verify、LLVM 与 native，并覆盖最近反例。conformance 是预期，schema 存在、parser 接受、手填 IR 和测试数量都不能替代端到端能力。先读 [Codex 入口](CODEX-START.md)、当前主树工单与 [并行计划](PARALLEL-PLAN.md)，按 [Rust 四目标](RUST-GOALS.md) 区分实证、未测和未完成。

## 有限同步错误与清理（draft11）

```zil
enum Fault { Failed(i32) }
fn value(fail:bool)->Fault!i32 {
    if fail { return Fault.Failed(9) }
    return 42
}
fn main()->Fault!i32 { return value(false)? }
```

`E!T`的E是错误类型，T是成功类型。返回值已是同一错误联合时直接返回；否则只在恰好匹配一侧时注入，双方同型的隐式注入拒绝。无额外载荷上下文的整数字面量仍默认i32，需要其他类型时先显式标注局部类型。`?`的操作数必须是错误联合，且E与当前函数的E完全相同；成功取T，错误先保留载荷再清理本侧责任并提前返回。

当前E/T限有限Copy类型。普通main的i32成功值作为退出码、void成功为0、错误为1；入口不自动打印错误。资源/拥有闭包会在这些同步出口清理，待提交参数和callee也有实际持责位置。checked trap仍终止进程，未实现panic unwind、取消或拥有错误载荷。完整动态错误程序和15组行为见`examples/mainline/error_cleanup/README.md`。
