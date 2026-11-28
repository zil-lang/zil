# ZIL 语言导览（Language Tour）

> **文档性质**：面向系统程序员的实践教程（从日常写法出发，而非形式规范条款）。本页所有 ```` ```zil ```` 代码块均可通过 [`tools/check_doc_examples.py`](../../tools/check_doc_examples.py) 经真实 `zil` 编译器（含独立验证器 `zil-check` 与 LLVM 23 原生后端）编译并运行通过。
>
> - 完整标准库公开 API：[`docs/guide/stdlib-reference.md`](stdlib-reference.md)
> - 当前开发预览能力与边界：[`docs/guide/supported-language.md`](supported-language.md)
> - 正式语言规范（`ZIL-LANG-1.5`）：[`_zil_design.md`](../../_zil_design.md)
> - 编译器架构与缓存管线：[`docs/architecture/compiler.md`](../architecture/compiler.md)

---

## 1. 程序结构、变量与控制流

ZIL 程序的默认入口是 `fn main() -> i32`，返回值作为进程退出码（`0` 表示成功）。

- `let` 声明**不可变绑定**（默认选择）；`var` 声明**可变绑定**。
- 分支条件（`if` / `while`）不强制圆括号，但分支体必须使用花括号 `{ ... }`。
- `for x in start..end` 遍历半开整数区间 `[start, end)`；`loop` 配合 `break` / `continue` 表达显式循环。
- 语句换行即可分隔（若需跨行续写二元表达式，应将运算符留在行尾或用括号包裹），也可在同一行用分号 `;` 分隔。

```zil
fn sum_even_under(limit: i64) -> i64 {
    var acc: i64 = 0
    for i in 0..limit {
        if i % 2 == 0 {
            acc += i
        }
    }
    return acc
}

fn main() -> i32 {
    let limit: i64 = 10
    let total = sum_even_under(limit)
    // 0 + 2 + 4 + 6 + 8 = 20
    var countdown: i64 = 3
    var steps: i64 = 0
    while countdown > 0 {
        countdown -= 1
        steps += 1
    }
    if total == 20 && steps == 3 {
        return 0
    }
    return 1
}
```

---

## 2. 标量类型、溢出检查算术与显式转换

ZIL 提供精确宽度的 13 种标量类型：

- 有符号整数：`i8`、`i16`、`i32`、`i64`、`isize`
- 无符号整数：`u8`、`u16`、`u32`、`u64`、`usize`
- 浮点与布尔/字符：`f32`、`f64`、`bool`、`char`

关键安全约定：

- **无隐式整数提升或收窄**：二元运算要求两侧类型一致，转换必须显式写成类型构造形式（如 `i64(x)`、`u32(y)`、`f64(z)`）。
- **默认算术带溢出检查**：`+`、`-`、`*`、`/`、`%` 均不允许静默溢出或除零（溢出或除零触发确定性 trap）。
- **显式环绕运算**：需要二进制模运算（如哈希、校验和）时，显式使用 `+%`、`-%`、`*%`。

```zil
fn main() -> i32 {
    let a: i8 = 127i8
    let wrapped_add: i8 = a +% 1i8             // -128i8（显式环绕）
    let wrapped_mul: u32 = 65536u32 *% 65536u32 // 0u32

    let wide: i64 = i64(100i32) + 23i64
    let narrowed: u8 = u8(wide)
    let as_float: f64 = f64(narrowed)

    // 位运算（~ 为按位取反）
    let mask: u32 = (0x00ffu32 & 0x0f0fu32) | 0x0010u32

    if wrapped_add == -128i8 && wrapped_mul == 0u32 && narrowed == 123u8 && i32(as_float) == 123 && mask == 0x001fu32 {
        return 0
    }
    return 1
}
```

---

## 3. 复合类型、`Option`、错误联合 `E!T` 与 `?` 传播

ZIL 内置值类型结构体（`struct`）、带负载枚举（`enum`）、固定长数组 `[T; N]`、元组 `(A, B)`，以及一等错误联合 `E!T` 与可选类型 `Option<T>`（简写 `T?`）：

- 错误函数直接返回 `MyError!Value`：返回 `MyError.Variant` 构造错误，返回普通值构造 `.ok(value)`。
- 在返回兼容错误联合的函数内，后缀 `?` 遇到错误会**立即早退并释放当前作用域资源**。
- 使用 `match` 语句解构错误联合（`.ok(v) => { ... }` / `.err(e) => { ... }`）、`Option`（`Option.Some(v)` / `Option.None()`）和自定义 `enum`。

```zil
enum ParseError {
    Empty,
    Negative,
}

struct Range {
    pub lo: i64
    pub hi: i64
}

enum Shape {
    Circle(i64),
    Rect(i64, i64),
}

fn check_positive(n: i64) -> ParseError!i64 {
    if n == 0 {
        return ParseError.Empty
    }
    if n < 0 {
        return ParseError.Negative
    }
    return n
}

fn sum_two_positive(a: i64, b: i64) -> ParseError!i64 {
    let x = check_positive(a)?
    let y = check_positive(b)?
    return x + y
}

fn area_proxy(shape: Shape) -> i64 {
    match shape {
        Shape.Circle(r) => { return r * r * 3 },
        Shape.Rect(w, h) => { return w * h },
    }
}

fn find_first_even(xs: &[i64]) -> Option<i64> {
    for i in 0..xs.len() {
        if xs[i] % 2 == 0 {
            return Option::<i64>.Some(xs[i])
        }
    }
    return Option::<i64>.None()
}

fn main() -> i32 {
    let span = Range { lo: 2, hi: 8 }
    var ok_sum: i64 = -1
    match sum_two_positive(span.lo, span.hi) {
        .ok(v) => { ok_sum = v },
        .err(_) => { ok_sum = -1 },
    }

    var bad_err: i64 = 0
    match sum_two_positive(4, -1) {
        .ok(_) => { bad_err = 0 },
        .err(e) => {
            match e {
                ParseError.Empty() => { bad_err = 1 },
                ParseError.Negative() => { bad_err = 2 },
            }
        },
    }

    let arr: [i64; 4] = [3, 5, 12, 7]
    var found: i64 = -1
    match find_first_even(&arr[..]) {
        Option.Some(v) => { found = v },
        Option.None() => { found = -1 },
    }
    let rect_area = area_proxy(Shape.Rect(4, 5))

    if ok_sum == 10 && bad_err == 2 && found == 12 && rect_area == 20 {
        return 0
    }
    return 1
}
```

---

## 4. 所有权、`package`、`manages` 与显式转移 `give`

ZIL 将「普通可复制值」与「拥有清理责任的资源」在类型语法上显式区分：

- **`struct`**：普通值聚集类型；若全部字段可复制，则可按值拷贝。
- **`resource`**：原生线性/仿射资源句柄（如文件描述符、`TcpStream`），通过 `impl Drop for R` 定义确定性清理。
- **`package` + `manages`**：拥有一个或多个资源成员的组合载体（如封装 `ByteBuffer`、`Store<T>` 或其他 `package`）。
- **`give`**：显式移交受管资源所有权的关键字。当函数参数标注 `give buf: ByteBuffer` 或将受管资源移入 `package` 的 `manages` 字段时，调用点必须写 `give`；整个 `package` 本身作为拥有型值按值移交所有权，旧绑定在移交后静态失效。
- **与只读视图/句柄区分**：借用切片 `&[u8]`（或协议中的描述符 `Desc`）不拥有释放权，不能超过其真实拥有者的存活期。

```zil
package Message {
    manages payload: ByteBuffer
    pub tag: i32
}

fn wrap_message(give buf: ByteBuffer, tag: i32) -> Message {
    return Message { payload: give buf, tag: tag }
}

fn consume_len(msg: Message) -> i64 {
    let view: &[u8] = bytes_view(msg.payload)
    let n = view.len()
    // 函数结束时 msg.payload 自动确定性释放，无须手工 free
    return n
}

fn main() -> i32 {
    let buf: ByteBuffer = bytes_new()
    bytes_push(buf, 122u8) // 'z'
    bytes_push(buf, 105u8) // 'i'
    bytes_push(buf, 108u8) // 'l'
    let m = wrap_message(give buf, 7)
    let tag = m.tag
    let n = consume_len(m)
    // 此处 buf 和 m 均已移交所有权，静态上不可再使用
    if tag == 7 && n == 3 {
        return 0
    }
    return 1
}
```

---

## 5. 借用、切片窗口与显式来源标注 `<life a>`

ZIL 采用「共享只读（`&T`）与独占可变（`&mut T`）互斥」规则，由编译器基于**真实来源（Origin）**进行静态证明：

- 同一时刻可以存在多个 `&T` 只读借用，或唯一一个 `&mut T` 独占借用。
- 固定数组 `[T; N]` 通过显式切片语法 `&arr[..]` / `&mut arr[..]` 或区间 `&arr[lo..hi]` 构造切片视图。
- 当函数从多个借用参数中选择并返回其中一个借用时，使用 `<life a>`（或 `<life a, life b>`）显式声明返回借用的来源归属，调用方据此保留对应的父级 loan。

```zil
fn pick_larger<life a>(left: &'a i64, right: &'a i64) -> &'a i64 {
    if *left >= *right {
        return left
    }
    return right
}

fn scale_in_place(items: &mut [i64], factor: i64) {
    for i in 0..items.len() {
        items[i] = items[i] * factor
    }
}

fn main() -> i32 {
    var nums: [i64; 3] = [2, 5, 3]
    scale_in_place(&mut nums[..], 4)
    // nums 现为 [8, 20, 12]
    let top = pick_larger(&nums[0], &nums[1])
    let top_val = *top
    // 借用 top 最后一次使用后，可再次独占修改 nums
    nums[1] += 1
    if top_val == 20 && nums[1] == 21 && nums[2] == 12 {
        return 0
    }
    return 1
}
```

---

## 6. 闭包、`move` 捕获与 `dyn Trait` 动态派发

ZIL 同时支持零开销静态分发闭包与显式 `dyn Trait` 动态派发：

- **闭包写法**：`(x: i64) -> i64 => expr` 或带块体 `() -> i64 => { ... }`；加 `move` 按值捕获外部状态（如 `move (v: i64) -> i64 => v + base`）。
- **高阶函数约束**：通过 `Fn` / `FnMut` / `FnOnce` 区分只读可重入、可变可重入与一次性消费闭包。
- **动态派发（`OwnedDyn<dyn Trait>`）**：ZIL 不做隐式堆装箱；需要运行时多态对象时，显式调用 `owned_dyn::<dyn Trait>(value)`（若实现包含借入上下文则显式标注 `Context<'a>`），返回可失败的 `DynCreateError!OwnedDyn<dyn Trait>`，并在离开作用域时自动执行具体类型的析构与堆释放。

```zil
trait Metric {
    fn score(self: &Self) -> i64;
}

struct Linear {
    base: i64
}

struct Doubled {
    base: i64
}

impl Metric for Linear {
    fn score(self: &Self) -> i64 {
        return self.base
    }
}

impl Metric for Doubled {
    fn score(self: &Self) -> i64 {
        return self.base * 2
    }
}

fn apply_twice<F: FnMut(i64) -> i64>(x: i64, f: &mut F) -> i64 {
    return f(f(x))
}

fn make_metric(doubled: bool, base: i64) -> DynCreateError!OwnedDyn<dyn Metric> {
    if doubled {
        return owned_dyn::<dyn Metric>(Doubled { base: base })
    }
    return owned_dyn::<dyn Metric>(Linear { base: base })
}

fn eval_dyn(doubled: bool, base: i64) -> DynCreateError!i64 {
    let target: OwnedDyn<dyn Metric> = make_metric(doubled, base)?
    return target.score()
}

fn main() -> i32 {
    var base: i64 = 10
    let shift = move (v: i64) -> i64 => v + base
    base = 999 // 不影响 shift 已按值捕获的 base=10

    var step: i64 = 3
    var adder = (v: i64) -> i64 => {
        step += 1
        return v + step
    }
    let cl_res = apply_twice(1, &mut adder) // (1+4)+5 = 10

    var r1: i64 = 0
    match eval_dyn(false, 7) {
        .ok(v) => { r1 = v },
        .err(_) => { return 2 },
    }
    var r2: i64 = 0
    match eval_dyn(true, 9) {
        .ok(v) => { r2 = v },
        .err(_) => { return 3 },
    }

    if shift(5) == 15 && cl_res == 10 && r1 == 7 && r2 == 18 {
        return 0
    }
    return 1
}
```

---

## 7. 泛型、Trait 与关联类型

ZIL 支持类型泛型（`T`）、生命周期泛型（`life a`）与编译期常量泛型（`#N: i64`），并提供 trait 约束与关联类型投影（`T.Metric`）：

- 定义期即检查义务完整性：泛型函数与 `impl` 不能依赖「实例化时才偶然满足」的未声明约束。
- 关联类型通过点号投影（如 `Self.Metric`、`T.Metric`）访问。

```zil
trait Measurable {
    type Metric;
    fn measure(self: &Self) -> Self.Metric;
}

struct ScaledBox<T: Copy, #FACTOR: i64> {
    value: T
    base: i64
}

impl<T: Copy, #FACTOR: i64> Measurable for ScaledBox<T, FACTOR> {
    type Metric = i64;
    fn measure(self: &Self) -> i64 {
        return self.base * FACTOR
    }
}

fn measure_any<T: Measurable>(item: &T) -> T.Metric {
    return item.measure()
}

fn main() -> i32 {
    let b = ScaledBox::<bool, 4> { value: true, base: 25 }
    if measure_any(&b) == 100 && b.value {
        return 0
    }
    return 1
}
```

---

## 8. 确定性清理与 `defer`

除类型自身的 `Drop` 析构外，ZIL 提供作用域级的 **`defer { ... }`** 语句：

- 同一作用域内注册的多个 `defer` 按**后进先出（LIFO）**顺序执行。
- 无论函数正常返回、`if` 早退还是 `?` 错误传播离开作用域，已注册的 `defer` 都会在离开前确定性执行一次。
- `defer` 注册与清理路径同样受借用/所有权验证器独立核验（禁止在 `defer` 活跃期间制造非法独占冲突）。

```zil
fn record_event(log: &mut [i64; 4], len: &mut i64, tag: i64) {
    let idx = *len
    log[idx] = tag
    *len = idx + 1
}

fn run_with_defers(early: bool, log: &mut [i64; 4], len: &mut i64) {
    defer { record_event(log, len, 10) }
    defer { record_event(log, len, 20) }
    if early {
        return
    }
    record_event(log, len, 30)
}

fn main() -> i32 {
    var log: [i64; 4] = [0, 0, 0, 0]
    var len: i64 = 0
    run_with_defers(true, &mut log, &mut len)
    // 早退路径跳过 30；后注册的 20 先执行，先注册的 10 后执行
    if len == 2 && log[0] == 20 && log[1] == 10 {
        return 0
    }
    return 1
}
```

---

## 9. 编译期计算 `#(...)`

ZIL 使用显式语法 **`#(expr)`** 触发编译期求值（Comptime）：

- `#(...)` 在编译阶段于受控纯求值环境中执行普通 ZIL 函数，并将结果（标量、元组、定长数组、值结构体等）直接烘焙进目标产物，零运行时计算成本。
- 编译期求值受严格的确定性与资源预算约束，禁止执行 I/O、并发或外部 FFI。

```zil
fn fib(n: i64) -> i64 {
    if n <= 1 {
        return n
    }
    var a: i64 = 0
    var b: i64 = 1
    for _ in 2..(n + 1) {
        let next = a + b
        a = b
        b = next
    }
    return b
}

fn build_table() -> [i64; 5] {
    return [fib(5), fib(6), fib(7), fib(8), fib(9)]
}

fn main() -> i32 {
    let table: [i64; 5] = #(build_table())
    let check: (i64, bool) = #((fib(10), fib(10) == 55))
    if table[0] == 5 && table[4] == 34 && check.0 == 55 && check.1 {
        return 0
    }
    return 1
}
```

---

## 10. 结构化并发 `thread_scope` 与仿射 `Mutex<T>`

ZIL 的原生并发强调**结构化生命周期**与**仿射锁守卫**：

- **`Mutex<T>`**：通过 `mutex_create(init)?` 创建，`mutex_lock(m)` 返回仿射 `MutexGuard`，`mutex_value(&mut guard)` 取得受保护数据的独占借用；`guard` 离开作用域时自动确定性解锁，不可复制也不能逃逸到跨线程或跨挂起边界。
- **`thread_scope(worker, payload, body)`**：创建结构化子线程运行 `worker(payload)`，同时在当前线程运行 `body()`，并在返回前保证子线程已完成汇合；返回三元组 `(created: bool, child_result: i32, parent_result: i32)`。

```zil
fn add_five(cell: Mutex<i64>) {
    var guard = mutex_lock(cell)
    let value = mutex_value(&mut guard)
    *value += 5
}

fn worker(cell: Mutex<i64>) -> i32 {
    add_five(cell)
    return 1
}

fn run_concurrent() -> MutexCreateError!i32 {
    let cell = mutex_create(10i64)?
    let (created, child_rc, parent_rc) = thread_scope(worker, cell, () -> i32 => {
        add_five(cell)
        return 2
    })
    if !created || child_rc != 1 || parent_rc != 2 {
        return 2
    }
    var guard = mutex_lock(cell)
    let final_val = *mutex_value(&mut guard)
    if final_val == 20 {
        return 0
    }
    return 3
}

fn main() -> i32 {
    match run_concurrent() {
        .ok(code) => { return code },
        .err(_) => { return 4 },
    }
}
```

---

## 11. 组合标准库：`std.string`、`std.store` 与 `std.iter`

在 ZIL 中，双引号字符串字面量直接表示 UTF-8 只读字节切片 `&[u8]`。下面展示如何组合使用标准库中的 UTF-8 拥有字符串 `std.string`、动态拥有容器 `std.store` 与 `std.iter`：

```zil
import std.string
import std.store
import std.iter

fn demo() -> string.Error!i64 {
    let raw = string.from_utf8("  alpha,beta,gamma  ", 64)?
    let trimmed = string.trim(&raw, 64)?
    if !string.contains(&trimmed, "beta")? {
        return -1
    }

    let items: Store<i64> = store.new()
    if !store.reserve(items, 4) {
        return -2
    }
    match store.push(items, 10) { .ok(()) => {}, .err(_) => { return -3 } }
    match store.push(items, 20) { .ok(()) => {}, .err(_) => { return -3 } }
    match store.push(items, 30) { .ok(()) => {}, .err(_) => { return -3 } }

    let slice_buf: [i64; 3] = [
        *store_at(items, 0),
        *store_at(items, 1),
        *store_at(items, 2),
    ]
    var add = (a: i64, b: i64) -> i64 => a + b
    let sum = iter.fold(&slice_buf[..], 0i64, &mut add)
    return sum + string.byte_len(&trimmed)
}

fn main() -> i32 {
    match demo() {
        .ok(v) => {
            // sum(10+20+30)=60, len("alpha,beta,gamma")=16 => 76
            if v == 76 { return 0 }
            return 1
        },
        .err(_) => { return 2 },
    }
}
```

---

## 下一步阅读

- **查标准库 API**：[`docs/guide/stdlib-reference.md`](stdlib-reference.md)（自动从 `lib/std/` 源码抽取）
- **查看完整多模块应用**：[`examples/everyday-apps/`](../../examples/everyday-apps)（文本统计、CSV、JSON、CLI Todo、HTTP/1.1 服务与客户端、文件批处理、图依赖分析、异步任务调度）
- **查看当前开发预览范围与工具链安装**：[`docs/guide/supported-language.md`](supported-language.md)、[`docs/guide/build.md`](build.md)
- **查阅语言权威规范与形式语义**：[`_zil_design.md`](../../_zil_design.md)
- **了解编译器内部架构、验证器与增量缓存**：[`docs/architecture/compiler.md`](../architecture/compiler.md)
