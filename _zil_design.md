# ZIL 语言规范与参考（ZIL Language Specification）

> 规范版本：**`ZIL-LANG-1.5`** · 本文是 ZIL 源语言语义与形式安全模型的唯一文字权威源。本规范定义终态语言要求，不代表全部能力均已在当前发行版中实现；当前实际可用范围与 profile 限额见 [`docs/guide/supported-language.md`](docs/guide/supported-language.md)。

## 0. 规范权威、附件边界与写作约定

1. **四层文档分工**：
   - **语言规范与参考（本文件）**：定义 ZIL 源语言语法、类型系统、表达式与控制流、所有权/借用/协议合同、清理、编译期计算、并发/异步/FFI 及附录 A 形式语义模型。任何符合本规范的编译器实现（含未来自举重写实现）都必须遵守本文件。
   - **规范附件（[`spec/README.md`](spec/README.md)）**：[`spec/grammar.ebnf`](spec/grammar.ebnf)（文法产生式）、[`spec/attributes.json`](spec/attributes.json)（属性注册表）、[`spec/diagnostics.json`](spec/diagnostics.json)（稳定诊断码）以及正文具名引用的语言细则附件。若细则与本文件冲突，以本文件为准并同步修订附件。
   - **编译器架构与内部合同**：[`docs/architecture/compiler.md`](docs/architecture/compiler.md)（`ZIL-ARCH` / `ZIL-LOWERING`）、[`spec/semantic-kernel.md`](spec/semantic-kernel.md)、[`spec/source-lowering.md`](spec/source-lowering.md)、[`spec/interfaces.json`](spec/interfaces.json) 与 [`policy/dependencies.json`](policy/dependencies.json)，规定主编译器的内部表示、可替换分析算法、缓存与 LLVM 后端合同。
   - **设计决定、指南与验证**：设计动机与竞品基线见 [`docs/decisions/design-goals.md`](docs/decisions/design-goals.md)；使用导览见 [`docs/guide/`](docs/guide/quickstart.md)；产品门与符合性资产见 [`validation/README.md`](validation/README.md) 与 [`conformance/index.json`](conformance/index.json)；任务协调位于独立 `goal` 分支。
2. **规范用语**：

| 术语 | 规范含义 |
| --- | --- |
| **必须（MUST）** | 任何符合本规范的实现都必须满足的硬性义务 |
| **不得（MUST NOT）** | 明确禁止的行为或实现捷径 |
| **应当（SHOULD）** | 推荐遵循的约束；除非有明确技术理由，否则不应偏离 |
| **可以（MAY）** | 允许但非强制的可选实现策略 |
| **未指定（Unspecified）** | 规范不保证具体结果，但仍须保持内存与类型安全 |
| **实现定义（Implementation-defined）** | 具体行为由目标或实现选择，但必须保持一致并文档化 |

3. **诊断与失败分类**：
   - **语法错误（Syntax Error）**：源码不满足词法或 [`spec/grammar.ebnf`](spec/grammar.ebnf) 产生式。
   - **类型错误（Type Error）**：名称解析、类型检查、trait 一致性或编译期约束不满足。
   - **安全拒绝（`SafeReject`）**：静态来源、借用、初始化、访问权限、责任守恒或协议核验未通过；不得生成可执行产物。
   - **实现未支持（`Unsupported`）**：规范定义但当前实现或目标 profile 尚未支持的能力；必须显式拒绝，不得静默降级。
   - **运行期失败（Runtime Failure）**：已通过静态安全核验的程序在运行期触发的确定性失败（checked 算术溢出、切片/Store 越界、显式 `abort`、分配失败返回错误分支）。

---

<a id="ZIL-TARGET"></a>

## 1. 语言范围、安全保证与核心模型

### 1.1 安全保证与可信边界

ZIL 在显式可信计算基（TCB）边界下提供静态内存安全与数据竞争自由：

- **保证范围**：类型与初始化有效性、边界与对齐、对象及子对象存活、防重复回收、防悬垂与地址失效、防相交可变访问、以及防非原子数据竞争。
- **零默认托管开销**：普通值保持内联布局；语言不默认引入 GC、引用计数（RC）、世代句柄、隐式锁、隐式堆装箱或全局对象表。稳定图、独立可删身份、持续地址窗口与共享拥有按各自显式协议提供。
- **明确不保证**：任意程序终止、无死锁、无显式资源泄漏、硬件准时响应或业务事务正确。显式泄漏不得导致借入栈帧或短命 allocator 的生命周期被虚假延长。
- **可信基础**：规则核验器、可信原语、FFI/驱动适配、降低与原生后端属于声明的可信基础；安全不得以任何“少量 safe UB”折算。详细设计目标与 Rust 基线原则见 [`docs/decisions/design-goals.md`](docs/decisions/design-goals.md)。

<a id="ZIL-CORE"></a>

### 1.2 核心概念：值、资源与描述（非规范性概览）

ZIL 将“访问一个资源的描述（`Desc`）”与“持有该资源的清理责任（`Managed`）”分离：普通描述可以自由复制，而实际执行资格与唯一清理责任不得由复制产生。

```zil
fn inspect_then_destroy(read: Ref<Node>, end: Ref<Node>) {
    let value = read.value
    report(value)
    end.destroy()
}

let node = Node.create(alloc, 7)?
let alias = node
inspect_then_destroy(node, alias)
// inspect(node.value) // 安全拒绝：目标有效事实已被 destroy 撤销
```

上例中：
- `Node.create` 建立新资源实例 `r`、初始化事实、执行资格与唯一清理责任 `d`；`node` 与 `alias` 均以 `r` 为来源。
- 顺序执行允许两参数同源：先读 `read.value`，完成后再执行 `end.destroy()` 进入 `Closing` 并履行唯一责任 `d`，作用域退出时不再重复释放。
- 若顺序反置为先 `destroy` 后 `read`，有序合同要求两操作不相交，同源调用将被静态拒绝。
- 若资源引用需要返回到调用方之外，其来源必须受调用方输入保护，或随完整责任载体转交；局部栈地址不得凭描述返回自动延长寿命。

---

<a id="ZIL-SYNTAX"></a>

## 2. 词法结构与源面语法

完整文法产生式由 [`spec/grammar.ebnf`](spec/grammar.ebnf) 固定；本节规定词法规则、语句终止与源面歧义消除。

### 2.1 词法、注释、标识符与换行

- **编码与注释**：源码为 UTF-8。支持单行注释 `//` 与可嵌套块注释 `/* ... */`。`#` 是编译期指令族前缀，不是注释符。
- **标识符**：`IDENT` 遵循 Unicode `XID_Start` / `XID_Continue` 加 `_`；生命周期标识符写 `'name`（`LIFEIDENT`）。
- **语句终止与续行**：分号 `;` 与逻辑换行 `NEWLINE` 均可终止语句。行末出现二元运算符、`,`、`.`、`=`、`->`、`=>`、前缀 `&`/`give` 或 `|>` 时自动续行；行首 `(`、`[`、`.` 不向上一行反向拼接。`return` 与其操作数必须在同一逻辑行开始；普通具名函数没有隐式尾表达式返回。
- **泛型实参**：类型位置写 `T<A, B>`；值与表达式位置统一写 `make::<u8, 32>(...)` 与 `Buffer::<u8>.new(...)`，消除 `<` 与比较运算的歧义。

### 2.2 字面量与运算符优先级

- **整数与浮点字面量**：十进制/进制整数字面量无上下文约束时默认 `i32`，可带显式后缀（`u8`..`usize`、`i8`..`isize`）表达完整值域（含最小负值）。浮点字面量无上下文约束时默认 `f64`，十进制字面量按 IEEE-754 nearest ties-to-even 舍入一次，有限字面量溢出至无穷诊断为错误。
- **字符串字面量**：运行期普通字符串字面量具有程序静态存储期，类型为不可变字节切片 `&[u8]`；允许内嵌 NUL，不追加终止符，不隐式堆分配。拥有可变字符串通过标准库 `std.string` 显式构造。
- **运算符优先级（高 → 低）**：
  1. 后缀：`.`、`::<>`、`()`、`[]`、`?`
  2. 前缀：`!`、`-`、`~`、`*`、`&`、`&mut`、`give`
  3. 乘除余：`*`、`/`、`%`、`*%`
  4. 加减：`+`、`-`、`+%`、`-%`
  5. 移位：`<<`、`>>`
  6. 按位：`&` → `^` → `|`
  7. 范围：`..`
  8. 比较与相等：`==`、`!=`、`<`、`<=`、`>`、`>=`
  9. 逻辑：`&&` → `||`
  10. 管道：`|>`（左结合）
  11. 闭包箭头体：`=>`

### 2.3 绑定、赋值与模式

- **绑定**：`let` 声明不可重绑定局部变量；允许重绑定、写入字段、形成 `&mut` 或对 `Managed` 载体执行整值替换的变量必须声明为 `var`。绑定性与所指目标的外部访问权限分开判定。
- **赋值语句**：`=` 及复合赋值运算符为语句，不产生值。执行顺序固定：先各求值一次左值接收者与索引，再求值右值表达式，准备全部成功后写入目标并结算旧值（右侧已发生的副作用不因失败回滚）。
- **模式**：支持标识符、字面量、通配 `_`、元组 `(a, b)`、具名记录 `S { field: pat }`、记录式/元组式枚举 `E.V { ... }` / `E.V(...)`、`Option`、错误联合 `.ok(pat)` / `.err(pat)` 与引用绑定 `ref pat` / `ref mut pat`。`ref` / `ref mut` 直接取得真实活动位置地址，不复制载荷到临时值。

---

<a id="ZIL-MODULE"></a>

## 3. 模块、作用域与程序入口

### 3.1 模块与可见性

- **文件即模块**：每个 `.zil` 文件构成一个独立模块；模块内声明默认私有，显式 `pub` 导出。
- **导入解析**：`import a.b as alias` 使用点分隔逻辑路径，映射到 `a/b.zil` 或 `a/b/mod.zil`（二者同时存在时诊断为歧义错误），路径不得逃出项目根目录。包级依赖必须无环；模块内允许普通递归调用。
- **标准库信任根**：`std.*` 随编译器以真实 ZIL 源码分发，项目本地同名文件不得劫持 `std.*` 命名空间。

### 3.2 公开接口兼容性与工具一致性

- 模块公开合同区分输入前提、可能效果（may）、必然保证（must）、管理路由与物理 ABI。实现替换不得加强前提、扩大破坏范围、削弱后置保证或破坏布局。
- 名称解析确定，函数本地类型 binder 遮蔽同名外部类型。完整类型证据由独立核验器检查，不因 trait 求解器返回真即直接授信。IDE、`check` 与 `build` 共享同一语言判定语义；自动修复不得重排有副作用语句或收窄真实效果。

<a id="ZIL-SCRIPT"></a>

### 3.3 程序入口与托管扩展边界

- **Native 入口**：Native 源文件顶层仅允许声明，程序从唯一显式 `fn main()` 启动。合法返回类型为 `void`（退出码 `0`）、`i32`（直接作为进程退出码）或错误联合 `E!T`（错误分支先执行确定性清理再以固定退出码 `1` 结束）。
- **托管边界**：托管宿主与 GC 不属于 Native 默认执行模型。请求未实现的 managed profile 必须显式返回 `Unsupported`，不得以 GC 存活替代确定性资源清理。

---

<a id="ZIL-MODES"></a>

## 4. 类型系统与值类别

### 4.1 四个核心类别

ZIL 的全部源语言类型均唯一归属以下四个类别之一，不允许因使用方式反向猜测或暗中切换类别：

| 类别 | 源语言形式 | 赋值与表示语义 |
| --- | --- | --- |
| `Value<T>` | 标量、普通 `struct`、普通 `enum`、元组、数组 | 满足 `Copy` 时按位复制，否则按值移动消费；内联布局 |
| `Managed<R>` | 资源工厂产物、`give` 参数、`manages` 字段、`Managed<R>` | 持有真实资源头与唯一清理责任；仅由工厂建立并经 `give` 转交，不因描述复制再产生责任 |
| `Desc<R>` | 资源名义类型 `R`、`Ref<R>` | 指向同一资源实例的借入描述入口；可自由复制，不持有清理责任 |
| `Address<m, T>` | `&T`、`&mut T`、`&[T]`、`&mut [T]` | 持续有效的内存地址窗口；不可改写为逻辑重查 |

- `resource R { ... }` 声明具名资源类型（裸 `R` 在参数/变量位置默认表示 `Desc<R>`，拥有载体需显式使用 `Managed<R>`、`manages` 或 `give`）；`package P { manages field: R; ... }` 声明含显式责任字段的名义拥有聚合。普通 `struct` 不因字段名或方法名自动变成资源。
- `Managed` 为保留内建类型构造器，用户声明不得重绑。`Managed<R>` 仅接受已登记资源类型 `R`（拒绝重复 `Managed` 包装与非资源实参），不授予 `Copy`、`StoreElement`、`Transfer` 或 `Share`，作为泛型实参（如 `Task<Managed<R>>`、`Poll<Managed<R>>`）时完整保留拥有类别。
- 工厂的 `create`、`give`、`from` 模式在类型与导出接口中静态确定；普通值中含 `Ref` 时复制为浅连接复制。

### 4.2 整数与浮点数值语义

- **十种内建整数**：

| 类型 | 位宽 | 值域 |
| --- | --- | --- |
| `u8` / `u16` / `u32` / `u64` | 8 / 16 / 32 / 64 | `0 .. 2^N - 1` |
| `i8` / `i16` / `i32` / `i64` | 8 / 16 / 32 / 64 | `-2^(N-1) .. 2^(N-1) - 1` |
| `usize` / `isize` | 目标指针位宽 | `0 .. 2^W - 1` / `-2^(W-1) .. 2^(W-1) - 1` |

  - `usize`/`isize` 的类型身份始终独立于 `u64`/`i64`/`i32`（即使目标位宽同为 64 位）；所有宽度按目标配置确定，不取宿主平台位宽。
  - **无隐式宽化**：二元整数运算要求操作数同类型；`+`、`-`、`*` 与一元 `-` 执行 checked 溢出检查（越界触发运行期失败，release 构建不得开启 UB）；显式回绕运算使用 `+%`、`-%`、`*%`（按 `2^N` 取模）。
  - **除余与移位**：除零触发运行期失败；无符号除法/余数/右移采用无符号语义；移位量与左操作数同类型，先检查 `0 <= shift < N`，左移截取低 `N` 位。
  - **显式检查转换**：`u8(x)`..`isize(x)` 为单参数保留数学值的显式检查转换原语（负值转无符号或越界均失败），不得被值或函数声明遮蔽（成员名不受限）。
  - 既有切片与 `OwnedStore` 的长度和索引 `i64` 接口合同保持不变。

- **IEEE-754 浮点（`f32` / `f64`）**：
  - 分别为 binary32（4 字节对齐）与 binary64（8 字节对齐），类型不同且无隐式宽化。
  - `+`、`-`、`*`、`/` 与一元 `-` 遵守 IEEE-754 nearest ties-to-even 语义，保留渐进下溢、带符号零、无穷与 NaN（非有限结果不触发整数式 checked trap）。若支持浮点余数，定义为向零截断商的 `fmod`。不支持回绕、位运算或移位。
  - 比较遵守 IEEE-754：含 NaN 时 `==` 与有序比较为假、`!=` 为真，`+0.0 == -0.0` 为真。不得以 fast-math、重结合或隐式 FMA 改变结果；FFI 不得暗改安全调用方的浮点舍入环境。
  - **数值互转与 CT 位表示**：`f32(x)` / `f64(x)` 允许向目标精度舍入一次（有限源舍入至无穷为检查失败）；浮点转整数要求源值有限、为数学整数且在目标整数值域内（`-0.0` 转为 `0`，NaN/无穷/小数/越界均在可能产生底层 poison 的转换前检查失败，不得以舍入后的最大整数浮点常量扩大接受范围）。编译期（CT）严格遵循同一目标浮点语义；常量、Wire 与缓存键保存 `u32`/`u64` 原始位，算术 NaN 的 payload 无源语义保证，CT 统一 quiet NaN 且不对普通负零常量做零归一化。

### 4.3 聚合、切片、空值、错误联合与匿名 Sum

- **元组、数组与切片**：`(A, B)` 为位置聚合；`[T; N]` 为定长内联数组。`&[T]` / `&mut [T]` 为 `{ptr, i64 len}` 借入切片窗口，通过 `&a[lo..hi]` / `&mut a[lo..hi]` 创建，先检查 `0 <= lo <= hi <= len`；裸 `[T]` 不可按值传递。
- **`Option<T>`（`T?`）**：`T?` 等价于内建 `Option<T>`（`Option.None` / `Option.Some(v)`），无隐式装箱或隐式 `Some` 包装。
- **`ErrorUnion`（`E!T`）**：错误与成功分支保持独立 tag（即使载荷类型相同）。类型唯一匹配时支持隐式注入，显式构造使用 `error_ok::<E, T>` / `error_err::<E, T>`，模式使用 `.ok(pat)` / `.err(pat)`。后缀 `?` 消费错误联合，成功则取出载荷，失败则沿同类型错误返回出口传播并在退出前清理已建责任。细则见 [`spec/error-union-construction.md`](spec/error-union-construction.md)。
- **匿名 Sum（`A | B`）**：安全带 tag 结构化联合，展平并按完整类型身份规范排序，拒绝重复成员（非 C raw union）。含任一拥有变体的 `enum` 或 Sum 整体非 `Copy`。

### 4.4 `ByteBuffer` 显式拥有字节存储

<a id="ZIL-BYTE-STORAGE"></a>

`ByteBuffer` 为无公开字段的内建资源类型，内联保存 `{ptr, len, cap}` 且恒满足 `0 <= len <= cap <= i64::MAX`，遵循 `Desc`/`Managed` 与 `give`/`create` 规则：

- `bytes_new()`：构造空载体，不分配堆内存；
- `bytes_reserve(b, cap: i64) -> bool`：按需预留容量；失败或负容量时保持原头与原内容不变；
- `bytes_push(b, v: u8) -> bool`：仅在已有容量内追加，不隐式扩容；
- `bytes_truncate(b, len: i64) -> bool`：仅在 `0 <= len <= bytes_len(b)` 时缩短前缀；
- `bytes_len` / `bytes_capacity` 返回 `i64`；`bytes_view` / `bytes_view_mut` 在已初始化前缀 `[0, len)` 上建立共享/独占字节切片窗口。

`reserve`、`push`、`truncate`、`give`、`drop` 及相交写均须排除活动借片冲突（即使本次不扩容或失败）；清理时先结算唯一责任再释放底层缓冲。

---

## 5. 表达式、求值顺序与控制流

### 5.1 固定求值顺序

所有子表达式按确定从左到右顺序求值；接收者先于参数求值；函数实参与记录显式字段按源码出现顺序求值，省略字段按类型声明顺序求默认值。任何优化不得改变可观察副作用、检查失败点或析构顺序。

<a id="ZIL-RECEIVER"></a>

### 5.2 方法调用、接收者默认值与 `.name`

- **接收者解析**（在读取方法体前确定）：显式首参数 `self: Type` / `give self: Type` 优先；其次取固有 `impl Type(self: ReceiverType)` 组默认；trait impl 继承 trait 签名；无接收者则为静态关联函数。
- **消费接收者**：`give self: Self` 必须在调用点显式写 `(give r).method(...)` 或 `R.method(give r, ...)`，不得将普通 `r.method(...)` 静默升级为所有权转交。资源类型的 `self: Self` 默认为描述模式，`self: Ref<Self>` 始终为描述。
- **`.name` 与消歧**：`.name` 在绑定阶段直接等价于 `self.name`，非动态 `this`，也不表示枚举变体或管道输入。`object.method(args)` 降为关联函数调用；多 trait 冲突使用 `Trait.method(receiver, args)` 显式消歧。`Type.method` 为未绑定函数项；需捕获接收者的回调须显式写 `(args) => object.method(args)`。

<a id="ZIL-RECORD"></a>

### 5.3 记录构造与更新

- **名义构造**：`Point { x: 1, y: 2 }` 构造名义记录；存在确定期望类型时可写 `{ x: 1, y: 2 }` 或 `.{ x: 1, y: 2 }`（不引入匿名结构类型）。字段简写 `{ x, y }` 引用同名绑定。
- **记录更新（Spread）**：`Point { ...base, x: 4 }` 先按 Copy/Move 取得完整 `base` 临时值，再按序求值覆盖字段并提交（被覆盖的旧字段按声明序清理一次）。通用 spread 仅适用于无自定义整体 teardown 的普通 `struct`；`resource`、`package` 与含 `manages` 载体必须使用构造器、`replace` 或已核 `decompose`。

<a id="ZIL-PIPE"></a>

### 5.4 管道与元组展开

- **管道（`|>`）**：`left |> target` 左结合，先求值 `left` 一次，再求值 `target` 一次，将前者作为唯一实参调用后者（不自动柯里化、不自动展开元组、不隐式插入 `?` 或 `await`；匿名函数作为右操作数时需加括号）。
- **元组展开**：调用实参支持 `...tuple` 显式展开，按元组索引顺序生成确定个数实参，并逐元素核验传递模式与所有权。

<a id="ZIL-CFG"></a>

### 5.5 控制流、模式匹配与递归

- **控制流语句**：支持 `if` / `if let`、`while` / `while let`、`for pat in expr`、`loop`、`match`、`with x = expr { ... }`、`break`、`continue`、`return`、`defer` 与 `unsafe { ... }`。
- **穷尽与安全匹配**：`match` 必须覆盖所有合法变体；所有判别观察与守卫在字段绑定或移动前完成，匹配失败的分支不得提前消费 scrutinee。
- **分支与循环语义**：不可达源码仍须通过类型检查；实际安全效果仅沿可达路径传播。循环中每轮新建立的载体在跨回边前必须清理、转交至真实 carrier，或由 `Option`/局部槽显式承接；`break` 与错误出口仅按已实际初始化部分清理。
- **递归与配额**：递归调用按已声明签名与状态合同作部分正确性归纳（语言不要求证明运行期终止），但有限执行前缀必须满足全部状态与借用前提，且不得凭递归契约凭空伪造新资源。编译期类型/常量/特化展开受逻辑配额约束，超限报 `ResourceLimit`（不得生成可执行产物，且结果不随墙钟或缓存冷热变化）。

---

## 6. 函数、闭包、泛型与 Trait

### 6.1 函数声明与合同头

函数头顺序固定：`[pub] fn name [Generics](Params) [-> ReturnType] [where ...] [contract { ... }] Body`。正文形式为 `{ ... }`、`=> { ... }`、`=> expr` 或外部声明 `;`。

显式 `contract { ... }` 采用有限声明 DSL：
- `result(create | give | from(...))`
- `requires(read | write | manage | apart | state)`
- `provides(manage | noescape | static_callbacks | on_pending | on_ready)`
- `effects(alloc | io | block | suspend | spawn | ffi | reenter | panic | call | cleanup)`
- `resume(local | transfer)`

有正文函数可自动推导并导出合同；显式 `contract` 是定义方必须满足的静态上界（非用户自授信声明）。无正文函数、`dyn` 槽、FFI 与递归环必须声明足够合同，缺失不得默认视为空效果。

<a id="ZIL-CAPTURE"></a>

### 6.2 闭包、捕获规则与调用能力

闭包写为 `x => expr`、`(x: T) => expr`、`() => { ... }`，可在参数前加 `move`、参数后加 `-> R`。闭包块体末尾不带分号的表达式（含末尾 `if`/`else`）作为返回值。先解析名字与类别，再按正文用途确定捕获；返回、存储或异步目标仅检查所得环境，不反向修改捕获方式：

| 自由路径类别及用途 | 普通闭包环境 | `move` 闭包环境 |
| --- | --- | --- |
| 只读普通 `Value`（含 `Copy`） | 共享借入最小合法位置（不默认快照） | `Copy` 复制；非 `Copy` 移动 |
| 修改 `Value` 或重绑定外部 `var` | 独占借入对应位置，暂停父侧冲突使用 | 修改环境内拥有值，不重绑定原位置 |
| 按值消费非 `Copy` `Value` | 创建时移动进入环境 | 同左 |
| 普通使用 `Desc`（含修改目标） | 复制描述与来源，操作记入 Call 合同 | 同左，不接管目标责任 |
| 普通使用实际 `Managed` | 保存 `Desc`；管理操作另保存 route 与 Hold | 将真实 `Managed` 载体转交环境 |
| 正文含 `give r`（`r` 为 `Managed`） | 创建时转交环境，调用时按正文给出 | 同左 |
| 已有 `Address` | 按需借用/重借已有窗口 | 复制共享地址或移动独占地址，上界不变 |

精确路径规则：普通内联字段可精确捕获并合并同源前缀；`Address` 在首次解引用处截断；`Desc` 在描述值处截断。捕获字段按首次出现源码序排列并按逆序清理；环境构造失败仅清理已移入字段。

闭包调用能力由内建名义协议 `Fn(A...) -> R`（共享环境）、`FnMut(A...) -> R`（独占环境）与 `FnOnce(A...) -> R`（消费环境）表示，支持 `Fn → FnMut → FnOnce` 单向适配（不削弱 Hold、效果与状态前提）。普通调用按 `Fn → FnMut → FnOnce` 静态确定一次模式，检查失败不回退。`FnOnce` 表示至多调用一次，未调用环境仍须准确清理；`Fn` 不隐含纯函数、无重入或线程安全。细则见 [`spec/closure-receiver-borrows.md`](spec/closure-receiver-borrows.md) 与 [`spec/returned-capture-provenance.md`](spec/returned-capture-provenance.md)。

### 6.3 函数指针、Opaque 返回与动态抽象

- **薄函数指针**：`fn(A, B) -> R` 无环境，仅接受无捕获且 ABI 兼容的函数或闭包，不隐式堆装箱。
- **Opaque 返回（`-> impl Trait`）**：在每个定义及泛型实例下对应唯一具体类型，所有可达 `return` 必须同型；导出合同完整保留 Hold、Call、Cleanup、Resume、来源与错误边界。
- **借入动态派发（`&dyn Trait` / `&mut dyn Trait`）**：表示为 `{receiver ptr, immutable vtable ptr}`，保留实际接收者来源、父 loan 与方法合同；`&dyn Trait` 不可调用可变槽。细则见 [`spec/shared-dyn-dispatch.md`](spec/shared-dyn-dispatch.md) 与 [`spec/dyn-implementation-witnesses.md`](spec/dyn-implementation-witnesses.md)。
- **显式拥有动态存储（`OwnedDyn<dyn Trait>`）**：非 `Copy` 拥有载体，由 `owned_dyn::<dyn Trait>(val)` 显式可失败堆分配（返回 `DynCreateError!OwnedDyn<dyn Trait>`，失败时同步清理已消费值恰一次）；表示为 `{payload ptr, immutable vtable ptr}`，析构时按真实具体类型清理 payload 再释放堆块。细则见 [`spec/owned-dyn-storage.md`](spec/owned-dyn-storage.md)。

<a id="ZIL-TRAIT"></a>

### 6.4 泛型、Trait、生命周期与可替换表面语法

- **名义 Trait 与一致性**：支持方法、默认方法体、Supertrait、关联类型/常量与关联合同。`impl` 遵循孤儿规则（trait 或 `Self` 最外层名义定义属于当前包），已发布实现不得重叠；不支持回溯选择、负约束试探或 impl specialization。未实例化泛型定义与默认方法须在定义期按声明约束独立检查。保留 trait（`Copy`、`Transfer`、`Share`、`Drop`、`Fn*`、`Value`、`SyncDrop`、`StoreElement`）排他规则见 [`spec/reserved-module-trait-names.md`](spec/reserved-module-trait-names.md)。
- **关联约束**：关联等式不得循环自证；内联约束与 `where` 约束语义等价。
- **生命周期与 Lending**：`<life a>` 与 `&'a T` 区分环境固定依赖（`Fixed`）与逐调用新鲜依赖（`Invocation`）；可变字段与容器参数保持不变型，禁止缩短表面借期将短命引用写入长命容器。支持函数与 trait 方法 lending、高阶借用函数指针 `for<life a> fn(&'a T) -> &'a T` 与可变存储持有者。细则见 [`spec/trait-method-lending.md`](spec/trait-method-lending.md)、[`spec/mutable-stored-holder.md`](spec/mutable-stored-holder.md) 与 [`spec/semantic-scopes.md`](spec/semantic-scopes.md)。
- **可替换表面语法（`syntax use`）**：`syntax use path as alias` 绑定锁定清单中的已核语法提供者，通过 `alias! { ... }` 调用，无全局 token 抢占或导入顺序依赖。扩展仅能产生未核 AST/CoreAST 并附卫生作用域与源映射，产物须完整经过后续类型、分析与独立安全核验，不得绕过安全核。

---

## 7. 所有权、借用、资源包与访问安全

### 7.1 资源拥有转移（`give`）与构造位置

`give place` 将 `place` 持有的 `Managed` 资源载体与唯一清理责任从当前上下文转移至目标槽（如函数参数 `take(give r)`、返回值 `return give r`、`move` 闭包或 `package` 字段）：

- **合法条件**：`place` 必须指向当前上下文实际持有的有效 `Managed` 载体，且不存在阻塞移动的活动地址窗口或管理记录借用。
- **转移结果**：原位置进入已移出状态；目标位置取得载体与清理责任。既有 `Desc` 不会自动重定向至新资源，其后续能否访问仍取决于实际存储、来源与当前访问资格。
- **非法情形**：
  - 对仅为 `Desc` 的别名执行 `give alias` 必须拒绝（执行资格或管理路由不等同于载体所有权）；
  - 普通非 `Copy` `Value` 与完整 `package` 按值直接移动，不得将 `give` 用作普通 `Value` 的可选移动拼写；
  - 工厂临时值传入借入参数时，调用方在完整表达式期间持有责任，被调函数不得返回该临时的逃逸裸描述；返回完整载体须使用 `result(create)` 或 `result(give)`。
- `close` / `destroy` 通过借入管理路由履行原载体的清理责任，不等同于 `give`。

<a id="ZIL-ACCESS"></a>

### 7.2 访问安全、强引用窗口与 `OwnedStore` 借用

- **一般访问规则**：每次真实读取必须满足合法类型、已初始化、范围/对齐、目标存活与映射有效、读资格及无冲突写窗口或外部使用者；写入另须排除相交共享窗口并维持类型/变体/来源上界。
- **普通 `Desc` vs 强地址窗口（`&T` / `&mut T`）**：
  - 普通 `Desc` 复制仅传播来源，读写短权限在使用点核验：同源“先读后写”可合法，“先失效后读”必须拒绝；无保活承诺的 `Desc` 可因关闭变为失效，仅存在于变量中不自动阻止关闭。
  - `&T` 允许并存读，窗口存续期间禁止任何相交写、移空或重分配（即使物理地址未变）。
  - `&mut T` 独占目标足迹并暂停父侧冲突资格；重借形成父子窗口层次，禁止把同一 `&mut` 复制成两份独立可变权限。
  - 仅真实不同字段、不交区间、独立分配或受检分区可构成不交证明；不同变量名、不同泛型参数名或不同未知索引不构成不交证明。一次重分配后新窗口成立，旧切片/地址窗口保持失效（撤销历史 `H` 不可被后续重建抵消）。
- **`OwnedStore<T>` 共享与独占元素窗口**：
  - `store_at(store, index: i64)` 与 `store_at_mut(store, index: i64)` 先按从左到右求值接收者与索引，在运行期核验 `0 <= index < len`（越界进入 checked trap，禁止按 capacity 读未初始化尾部），再形成同步不逃逸的共享/独占元素地址窗口。
  - 窗口存续期间阻挡相交 `reserve`、`push`、`pop`、`give`、`drop` 与父壳移动（即使剩余容量充足或操作可能失败）。独立不交字段（如同时读 `state.items` 元素并写独立字段 `state.stats`）在证明真实足迹不交后允许并存。
  - `store_at_mut` 允许读写元素内 `Copy` 字段及其投影，以及直接具体回调字段（`Fn` 需共享窗口，`FnMut` 需独占窗口；`FnOnce` 须先 `pop` 取得拥有元素后调用），不得复制完整 `package`、掏空 `manages` 字段或制造部分初始化孔洞。

<a id="ZIL-PACKAGE"></a>

### 7.3 资源包、责任形状与拥有组合

- **显式管理根**：`package P { manages field: R; ... }` 显式声明真实责任字段；允许包含多个指向同一目标的 `Ref`，但禁止重复持有同一资源的管理责任。包移动保持内部资源实例并更新外壳投影；`replace` 更换字段实例时，旧值捕获不得重定向去关闭新资源。
- **打包、拆包与部分移动**：打包遵守“可失败准备 → 逐字段初始化 → 无失败提交”，中途失败仅清理本侧已得责任。`give` 不扫描可达图，不隐式接管包外 `Ref`。带用户析构的聚合禁止随意掏空字段，部分移动须通过已核 `decompose`。打开包引入的局部存在名字彼此不自动不交。
- **拥有形状与清理顺序**：任意拥有组合——`Managed`、`Product`（含嵌套 `Package`）、`Sum`（`Enum` / `ErrorUnion`）、定长数组、`OwnedStore` 初始化前缀 `[0, len)`、`Closure` 与 `OwnedDyn`——均遵守确定布局与清理：
  - `Product` 按字段声明序清理；数组与 `OwnedStore` 按 `[0, len)` 清理；`Sum` 仅清理活动分支；`Closure` 按捕获声明序清理；无用户 `Drop` 不得跳过内建 `ByteBuffer`/`OwnedStore` 释放。递归 by-value 布局拒绝。
  - `OwnedStore<T>` 恒维持前缀不变量：`store_reserve` 失败保留原头与元素；`store_push` 满容量或失败完整返还元素；`store_pop` 先缩短前缀再转交末项。
  - 完整 `Product`/`Sum` 的 `&T`/`&mut T` 要求全部子责任完整；整值赋值与字段替换按“先源序准备 RHS → 清理旧值 → 无失败发布新值”执行。
  - 细则见 [`spec/ordinary-values-and-cleanup.md`](spec/ordinary-values-and-cleanup.md)、[`spec/ordinary-store-ownership.md`](spec/ordinary-store-ownership.md)、[`spec/store-growth.md`](spec/store-growth.md) 与 [`spec/pod-store-elements.md`](spec/pod-store-elements.md)。

<a id="ZIL-ROUTE"></a>

### 7.4 来源集合与管理路径

当函数从多条路径返回资源描述时（例如 `if flag { a } else { b }` 导出 `from(a | b)`），来源集合仅描述可能访问目标，不自动提供可执行的清理管理路由：

- **访问接口与管理接口分离**：普通返回 `Ref<Node>` 仅发布来源与访问条件；需要通过返回值执行关闭/管理的接口须在公开合同中声明 `manage(result)`（由公开接口固定 ABI，不随调用方是否关闭而暗改 `Ref` 布局或已公布 `struct` 布局）。
- **路径形式与持久化边界**：管理路径仅由有限静态结构组成——单一静态来源、调用 ABI 携带的同步 `BindingRoute`、包内当前投影、与数据同步的选择见证、或受检 `Store` 管理准入。跨存储或类型擦除持久化管理路径必须使用带明确布局与 `Hold` 合同的借入管理视图 / 闭包环境，或内部拥有完整责任的包；仅保存裸 `Ref` 不承诺保存管理路线（库可封装该视图，但其字段与借入记录成本必须可见）。管理路由不凭空创造管理权限，仍须当前上下文具备授权，且借入清理记录会约束记录自身存活与移动。

---

## 8. 协议合同与确定性清理

<a id="ZIL-CONTRACT"></a>

### 8.1 操作效果与状态协议

- **有序关闭协议**：对同一资源的顺序操作遵循先用后关允许、先关后用拒绝的语义；无正常返回的路径（如死循环或 panic 前缀）仍须满足已发生的关闭与使用顺序，禁止以“不返回”掩盖二次关闭。
- **一般有限状态协议**：每个状态协议声明有限状态集、角色、投影、合法原语及逐结果边转换。所有可能达到的状态均须满足操作前提，状态转换与初始化/地址权限/清理同步保持（单纯修改标志字段不得授信）。多角色涉及状态变更时，同源声明为单一角色联合检查，其余相关角色须证明不交（`apart`）或匹配接口预定义的已核 alias-case。
- **可组合高阶合同**：高阶回调的效果在 `effects(call(f))` 处按实参合同实例化，不在回调创建点提前生效；保存回调的 Hold、实际 Call 与 Cleanup 分别组合。可能被多次调用的回调须证明状态与责任可重复满足前提。未知外部、动态目标或未声明组合保守处理或拒绝，绝不退化为空效果或运行时借用检查。

<a id="ZIL-CLEANUP"></a>

### 8.2 确定性清理、`Drop` 与同步 `defer`

- **关闭状态机**：资源关闭按固定四步执行：(1) 核验管理资格、绑定与无冲突使用；(2) 进入 `Closing`，禁止普通重入；(3) 执行已核 teardown 与字段清理；(4) 按正常、可恢复失败或 panic 提交 `Discharged` 或合法剩余义务状态。
- **清理记录自保护**：若清理记录位于即将释放的内存块内，必须在最终不可失败 dealloc 前提交记录或将剩余责任移入仍存活的清理激活，禁止 free 后回写已释放内存。外部 `close` 返回不确定错误时须转入隔离/不确定状态，不得自动重试 free/close；内存 dealloc 成功后责任不可复活。
- **清理顺序与 `Drop` Trait**：局部按成功构造逆序清理，聚合按固定字段序清理，部分构造仅清理已初始化部分。用户资源清理通过内建 `Drop` trait 显式绑定：`impl Drop for R { fn drop(self: Self) { ... } }`（`Self` 按资源默认类别为 `Desc`，仅由持有清理义务的激活在 `Closing` 期调用，同一资源至多绑定一次，不得按普通方法名 `close`/`destroy` 推断）。自动 `Drop` 不得挂起或返回可恢复错误，允许其合同声明的 I/O、分配、阻塞或 panic，且使用上下文必须允许对应效应。允许数据环，但不保证互相读取的析构环自动排序（应先在 `Live` 期显式 shutdown）。
- **普通同步 `defer { ... }`**：
  - 在声明点按普通闭包捕获规则建立零参 `Unit` 一次性清理动作，名字与来源在注册点绑定；共享捕获保持原来源与活借用至动作实际执行（不将 `Copy` 捕获一律改为声明时快照）。动作自身选择一次消费调用，不引入 `defer move` 语法或隐藏分配。
  - 环境构造并注册完成后，正常落出、`return`、`?` 错误传播、`break` 与 `continue` 均按与局部构造共享的逆序履行该作用域内已注册动作（`A`、`defer(A)`、`B` 的退出顺序为 `B` → 动作 → `A`）；未执行到注册点的路径执行零次；每次循环迭代独立结算；返回值先准备再清理后转交；trap/abort 不承诺 unwind 履行。
  - 清理正文限于同步 `Unit`，禁止向外 `return`、`?`、跨清理边界 `break`/`continue` 或挂起（正文自身内部循环的局部 `break`/`continue` 合法）；嵌套 `defer` 暂不开放。

---

## 9. 编译期计算与属性

<a id="ZIL-COMPTIME"></a>

### 9.1 `#` 编译期指令族

`#` 为显式编译期指令族的统一前缀（不提供可变全局编译器状态，也不额外放宽安全规则），分发形式由紧随其后的定界符或关键字决定（必须在同一逻辑行）：

| 形式 | 位置与语义 |
| --- | --- |
| `#(expr)` | 表达式或类型位置的编译期求值；`Syntax` 结果仅作数据，不自动插入 |
| `#{ stmts; yield value }` | 编译期块表达式；`yield` 仅结束本块，不得 `return` 外层运行函数 |
| `#[name(args)]` | 附着于下一项/字段/变体/形参的属性（见 §9.2） |
| `fn f<T, #N: usize>(...)` | 编译期常量泛型参数（使用 `N` 时不重复写 `#`） |
| `#if (cond) { ... } else { ... }` | 声明或语句位置编译期条件裁剪（不新增运行期词法作用域）；两支须词法/语法合法，仅物化并检查选中分支 |
| `#for (x in values) { ... }` | 声明或语句位置有限迭代生成；按迭代序物化，私有声明获卫生身份，公开重名报错 |
| `#emit expr` | 显式插入带卫生与类别信息的声明或语句列表；产物完整重检 |
| `#splice(expr)` | 显式物化表达式或类型 `Syntax` |

拒绝裸 `#call()`、`#[1, 2]`（编译期数组写 `#([1, 2])`）、空 `#[]` 与内属性 `#![...]`。

- **编译期数据类型与反射**：`Type`、`Syntax<Expr|Type|StmtList|ItemList>`、`Ident` 与 `TokenTree` 仅存在于编译期域，无运行期 ABI 或地址。模板构造使用 `quote<Category>{ ... }` 与 `$(ct_expr)` 洞；只读反射遵守可见性（形状查询依赖声明，布局查询另依赖 target/ABI）。
- **确定性执行与阶段隔离**：编译期执行遵循同一目标数值与安全规则，受逻辑步数、临时字节、生成节点与递归配额限制（超限报 `ResourceLimit`，不降级至运行期）。纯编译期禁止访问网络、时钟、系统随机、未声明环境变量或任意文件，禁止泄出临时地址、运行期资源身份或责任。
- **阶段依赖与封闭 `impl` 集**：阶段顺序固定为 `TokensReady → ConfigReady → ShapeReady → ImplIndexReady → LayoutReady → ContractReady → VerifiedCTBody`。查询仅读取已完成层级；trait 求解不得回入生成器修改当前 `impl` 集，依赖环报错。所有编译期生成产物均为未核 AST，必须完整通过类型、分析与独立安全核验。

<a id="ZIL-ATTRIBUTE"></a>

### 9.2 `#[...]` 属性与阶段规则

- **语法与附着**：每段仅含单个属性：`#[path]`、`#[path(args)]` 或 `#[doc = string]`，附着于紧随其后的声明、字段、变体或形参，合法目标与重复规则由 [`spec/attributes.json`](spec/attributes.json) 固定。不使用内属性 `#![...]`，裸 `[Attr]` 保留给数组/切片语法。属性参数自动处于编译期求值位置。
- **分类与阶段**：
  - **配置裁剪**（`cfg`）：在名称、类型与生成前基于封闭配置快照裁剪；被裁剪项仍须词法/定界合法，不进入后续解析。重复 `cfg` 取逻辑与。
  - **句法变换**（显式导入的具名扩展）：仅消费定界 `Syntax`，输出未核 CoreAST，不得读取未完成类型。
  - **类型辅助生成**（`derive`）：读取冻结的原声明形状，仅追加生成项，不改写原类型字段；产物汇总后统一检查重名与 impl 冲突。
  - **表示约束**（`repr`、`align`）：布局与目标对齐检查（`align` 须为合法二次幂），失败拒绝；`repr(C)` 不替代对外部 C 头文件的手工核对。
  - **生成提示**（`inline`）：尽力提示，不得改变语言语义或安全接受集；`#[inline(never)]` 与其他 `inline` 冲突。
  - **元数据与诊断**（`doc`、`allow`、`test`）：`doc` 按源码序拼接；`allow` 仅接受已注册 lint，绝不能关闭安全错误码。
- **安全边界**：不存在 safe `#[alive]`、`#[unique]`、`#[noalias]` 或 `#[trusted]` 以绕过静态核验；第三方属性须通过构建输入预注册清单声明阶段、查询接口与预算；自定义属性默认不保留至运行期。

---

## 10. 并发、异步任务与外部接口（FFI）

<a id="ZIL-CONCURRENCY"></a>

### 10.1 并发与内存访问规则

- **执行资格与数据竞争自由**：读资格可分份，任何非原子写要求无任何相交读/写持有者。任意两个未同步相交访问若有一方为写，即要求同一份不可复制的执行资格同时存在于两处，静态规则禁止该状态。复制描述不复制或拆分执行资格；同步交接必须经真实 happens-before 建立。
- **跨线程资格**：`Transfer`（跨线程安全迁移）与 `Share`（跨线程安全共享）由数据、闭包环境、Cleanup、Resume、allocator、TLS 与 driver 合同联合决定；`Pinned` 不推导线程安全。仅在父线程存活且不由子线程操作/清理的对象不得伪装为子线程载荷。
- **结构化线程（`thread_scope`）**：`thread_scope(worker, payload, body)`（语法为普通调用，语义保留不可遮蔽）创建真实 OS 线程执行静态 `worker`，父线程执行 `FnOnce` `body` 并在返回前强制 `join`（创建失败时清理已准备 `body` 环境且不运行 `worker`；`join` 无法保证静止时固定 `abort`）。共享借入载荷在创建至 `join` 期间保持活动共享窗口，阻挡父侧相交写、移动或失效扩容；`create`/`join` 建立真实 happens-before。
- **互斥锁与原子原语**：`Mutex<T>` / `mutex_scope` 基于真实 OS 锁保护临界区，`MutexGuard` 遵循仿射借用与跨线程限制（不可跨越 `thread_scope` 迁移），锁/解锁失败固定 `abort`。`SyncCell` / `atomic_scope` 提供真实硬件原子 load/store/fetch_add 与强 `compare_exchange`，显式核验 `Relaxed`/`Acquire`/`Release`/`AcqRel`/`SeqCst` 内存序矩阵，不使外围非原子字段自动安全。细则见 [`spec/affine-mutex.md`](spec/affine-mutex.md)。

<a id="ZIL-ASYNC"></a>

### 10.2 异步任务、取消、外部登记与 FFI

- **Direct vs Suspend 与拥有 `Task<T>`**：无挂起函数使用普通 Direct ABI；可能挂起的函数（显式 `effects(suspend)`、`resume(...)` 或可达 `task_yield()`）使用显式 SABI（`init`/`poll`/`request_cancel`/`poll_close`/`finish_destroy`）。同步入口、普通函数指针、裸 OS 线程 worker 与 FFI 回调不得直接进入 Suspend 图或隐式阻塞。无环子帧按并存区间静态布局；挂起递归环须使用显式间接存储，不隐式堆装箱。
  - `task_create(worker, args...) -> TaskCreateError!Task<T>` 显式可失败创建拥有任务（分配失败清理已转交输入恰一次）；
  - `task_poll(&mut task) -> TaskPollState!Poll<T>` 推进任务，`Ready(T)` 仅移出结果一次；
  - `task_request_cancel(&mut task)` 幂等请求取消；
  - `task_poll_close(&mut task) -> Poll<()>` 按创建序排空子帧与外部根；
  - `task_finish_destroy(give task)` 仅消费已达 `Closed` 的任务头（未 `Closed` 固定中止）。Direct 默认 `Drop` 仅在已证 `Closed` 或关闭必同步完成时合法。细则见 [`spec/task-owned-aggregates.md`](spec/task-owned-aggregates.md)。
- **异常与 `PendingExit`**：不得先展开直接调用栈再声称其局部缓冲已移交异步 draining；发布给外部的地址必须位于稳定帧、稳定包或结构化同步作用域覆盖的存储内。Suspend 边界将 panic 转为 `PendingExit(Panic)`，与 `Error`/`Cancel` 共用确定性收口（主体异常优先，其次按创建序选择未观察子错误），清理期二次 panic 按固定策略进入 Faulted/abort。
- **外部用户与登记对象**：全部外部使用者（callback、timer、waker、DMA、OS 登记）均纳入静止条件；`active == 0` 单独不证明未来无新回调，须取得提供者 no-future / drain 保证后方能回收。
  - **同线程登记**：`unsafe c_register(register, close, callback)` 产出 `Registration<F>`，`registration_valid` 只读观察，`registration_close(give owner)` 强关闭后销毁环境。
  - **跨线程可轮询登记**：`unsafe c_register_async(register, request_close, poll_drain, callback)` 产出 `AsyncRegistration<F>`，绑定 `Open`/`Closing`/`Closed` 三态协议，经 `async_registration_request_close`、`async_registration_poll_close`（`Ready` 证明 `Closed`）与 `async_registration_finish_destroy` 守住回收边界；`FnMut`、`ClosedEnvironment`、`Transfer`、`SyncDrop` 与密封 `ThreadCallable` 各自独立核验。细则见 [`spec/threaded-foreign-registration.md`](spec/threaded-foreign-registration.md)。
- **设备与 C FFI**：CPU release/acquire 不替代 DMA 映射与设备屏障。FFI 调用与登记必须位于 `unsafe` 块内；`provides(noescape(...))`、`provides(static_callbacks())` 与 `effects(ffi)` 均为显式 `unsafe` 承诺，不得跨界 unwind、`longjmp` 或挂起。`c_handle`（`c_null()`）、`c_wstr`（静态 UTF-16 NUL 终结只读指针）、`extern "C" fn(...) -> T` 与标量类型按目标 C ABI 传递。`unsafe c_with_callback(register, close, &mut f, body)` 在稳定栈槽上运行同步外部回调循环，关闭确保 no-future 后释放独占借用，无隐式堆分配。

---

<a id="ZIL-ACCEPT"></a>

## 11. 目标相关语义与规范拒绝边界

### 11.1 目标相关行为

- `usize`/`isize` 位宽、类型对齐与 C FFI 调用约定由目标配置（`TargetProfile`）决定；当前原生支持 Linux x86_64 SysV ABI 与 Windows x64 MSVC/PE ABI。
- 优化级别（`O0`/`O2`）、并行编译单元数与缓存冷热不得改变任何语言语义、安全接受集、溢出/越界检查或析构顺序。编译器内部降低与 LLVM 目标合同见 [`docs/architecture/compiler.md`](docs/architecture/compiler.md)（`ZIL-LOWERING` / `ZIL-ARCH`）。

### 11.2 永久拒绝边界与规范验收族

`SafeReject`、`ResourceLimit`、`Unsupported` 与基础设施失败属于不同结果类别；未完成必需核验不得生成可执行产物。支持矩阵可声明某实现尚未覆盖的 profile，但不得将原始安全边界删为非目标，也不得通过隐式 RC、锁、堆盒、全局表或延期关闭将错误例伪装为成功。

以下规范验收族由正反例测试夹具实现；产品量化门槛与独立测试资产边界见 [`validation/README.md`](validation/README.md) 与 [`validation/product-gates.json`](validation/product-gates.json)：

| 验收 ID | 必须成立的正例与最近反例 | 依据规则 |
| --- | --- | --- |
| V01 | 普通值、Copy/Move/Describe/Create/Give 明确；拒绝未转交责任复制与临时描述逃逸 | `ZIL-MODES` |
| V02 | 选择/字段/闭包来源完整；拒绝非精确单候选强更新与跨轮身份合并 | `ZIL-ORIGIN` |
| V03 | 同源先读后关接受，先关后读拒绝；隐藏 `Drop` 与不返回前缀不遗漏 | `ZIL-CONTRACT` |
| V04 | 跨模块选择返回的 payload/route/authority 对应；缺 route、错记录、释放后写 flag 拒绝 | `ZIL-ROUTE` |
| V05 | 稳定环、包返回、移动与内部回调成立；旧字段捕获不重定向新实例 | `ZIL-PACKAGE` |
| V06 | 自动捕获、`move`/`give`、函数值、opaque 返回、短回调与容器合法；拒绝隐式夺责、错误重调、丢失 Hold 与悬垂返回 | `ZIL-CAPTURE` |
| V07 | 分支/`loop`/递归与错误出口保持责任；拒绝未初始化发布与非法回边 | `ZIL-CFG` |
| V08 | `#(...)`/`#{...}`/`#[...]` 唯一解析；错误附着、非法阶段、重复/冲突属性拒绝 | `ZIL-COMPTIME`, `ZIL-ATTRIBUTE` |
| V09 | 生成的常量/类型/impl 统一重检；生成—求解环、伪造能力、未声明输入拒绝 | `ZIL-COMPTIME` |
| V10 | 泛型/关联投影/lending 组合合法；候选等式自证、未完成布局与隐式派发拒绝 | `ZIL-TRAIT` |
| V11 | 接收者补入、词法点号、记录/管道/展开与 Native 入口确定；同义前端保持绑定、求值、清理与错误 | `ZIL-SYNTAX`, `ZIL-RECEIVER`, `ZIL-RECORD`, `ZIL-PIPE`, `ZIL-SCRIPT`, `ZIL-TRAIT` |
| V12 | 稳定追加、分区地址与精确失效；跨 realloc、变体切换与 detach 的旧窗口拒绝 | `ZIL-ACCESS` |
| V13 | 一般 `Drop` 与部分构造准确，记录自身寿命合法；重入关闭与漏失败清理拒绝 | `ZIL-CLEANUP` |
| V14 | 静态帧、显式递归间接、`PendingExit` 与结果一次转交；提前展开/回收拒绝 | `ZIL-ASYNC` |
| V15 | 发布前登记、准入线性化、全部用户静止；`cancel`/`active=0`/旧通知不得单独授予释放 | `ZIL-ASYNC`, `ZIL-CONCURRENCY` |
| V16 | 共享读、不交写、真实同步与迁移；普通原子/Pin 不授予额外权限 | `ZIL-CONCURRENCY` |
| V17 | 同一已核 LIR 保持布局、清理、指针与同步；C/LLVM profile 不混同 | `ZIL-LOWERING` |
| V18 | 缓存依赖准确、损坏重建、CLI/IDE 一致；不可反序列化伪造安全授权 | `ZIL-MODULE`, `ZIL-ARCH` |
| V19 | 静态路径无强制动态管理层，构造/清理/转换全部计费，Rust 基线同语义 | `ZIL-TARGET`, `ZIL-LOWERING` |
| V20 | 冷检查/构建/增量/元程序/核验/宿主构建分别测量并记录真实工作量 | `ZIL-TARGET`, `ZIL-ARCH` |
| V21 | 演进对照同时计入应用、库、适配与可信代码；拥有输入与借入描述不错比 | `ZIL-TARGET` |
| V22 | 空测试、零命中、错误基线、过期证据、未支持或超时均不得记为通过 | `ZIL-ACCEPT` |

---

<a id="ZIL-ORIGIN"></a>

## 附录 A：形式语义模型（规范性）

本附录定义 ZIL 内存安全与访问协议的数学语义（“语言是什么”）。任何符合本规范的编译器所采用的静态分析或证书核验算法都必须对本模型保持可靠（sound）；具体可替换的分析算法（如 `One`/`Many` 基数格、预建有限位置图与 RPO 求解）归属 [`docs/architecture/compiler.md`](docs/architecture/compiler.md)。

### A.1 具体状态与静态抽象的可靠性关系

- **具体状态（Concrete State）**：由运行期分配与子对象、字段值及初始化位、动态实例身份、当前执行权限、唯一责任载体、活动地址窗口及外部/任务使用者共同构成（数学模型，不要求运行期维护全局管理表）。
- **静态状态（Abstract State）**：由有限的类型、来源上界、绑定、协议状态集合、访问权限、活动窗口、责任形状与潜在操作效果构成。
- **可靠性条件（Soundness）**：对任意可达具体状态，静态抽象必须满足：(1) 实际目标必被来源候选覆盖；(2) 静态宣称的必然事实（must）不得强于实际保证；(3) 静态记录的可能影响（may）不得小于实际破坏范围。精度损失只允许导致额外安全拒绝，不得制造虚假同源、虚假不交或虚假“已全部更新”事实。

### A.2 来源产生与传播规则

- **参数与外部接口**：从导出合同初始化来源变量与已声明外界；外部函数缺乏来源合同时不具备 safe 直接调用资格。
- **工厂创建**：仅在实际成功控制边引入新鲜实例身份；失败边不发布完整对象。
- **复制与投影**：描述复制保留原来源；字段读取从当前内存位置事实或声明类型上界获取来源。
- **精确写与未知写**：目标为唯一确定精确位置时以右值来源更新该位置；目标为多候选或集合族时仅与旧来源并合，不得覆写全部候选。
- **控制流合流**：合流处并合来源，循环重定义与非同源合流不得沿用旧分支条件。
- **闭包捕获与返回**：环境逐字段遵循同一来源规则；返回来源须逐字段建立等式，禁止从未转交的局部管理位置导出调用后可用的地址。

### A.3 视图事实（`ViewFact`）与投影稳定性

任意使用点的静态语义视图至少包含五项分量（编译期概念，不增加运行期引用字段）：

```text
ViewFact = {
    origins:    有限实际来源上界 (OriginSet),
    projection: 字段 / 固定槽 / 容器元素族 / 变体载荷投影 (Projection),
    requires:   本次使用依赖的有效事实 (Requirements),
    mode:       Desc短访问 | SharedAddress | ExclusiveAddress (AccessMode),
    route:      可选且已核的管理路径 (ResponsibilityRoute)
}
```

- `requires` 可包含：实例存活、字段已初始化、底层存储映射稳定、当前变体匹配、容器窗口受保护及接口明确要求的成员资格（从集合移出但仍合法拥有的对象不自动悬垂；内存字节尚存亦不证明类型、初始化或管理资格有效）。
- **投影依赖的细粒度稳定性**：稳定性是“特定投影对特定事件的依赖”，而非整个类型的单一布尔标志。容器头地址、元素地址与元素内动态字节地址可依赖不同映射；稳定 payload 不允许移动仍被借入的头。
- **实例身份隔离**：同一物理槽替换入新资源时，旧实例描述不得重新指向新实例；私有实现通过有限抽象投影导出，不得扫描下游不可见私有字段猜测不交。

### A.4 可观察操作效果分类与访问判定语义

形式模型至少区分以下七类不可互替的可观察动作（内部语义分类，非源语言 `effects(...)` 新增关键字）：

| 动作 | 必须成立的静态前提 |
| --- | --- |
| `Read` / `Write` | 当前读写资格、实际足迹、初始化与值有效性 |
| `Relocate` | 依赖旧存储映射的地址与投影窗口已结束 |
| `ReplaceInstance` / `ChangeVariant` | 旧实例、旧变体载荷及对应初始化事实不再被依赖 |
| `MoveCarrier` | 真实头、管理记录与外壳投影的地址依赖已结束，且责任合法转交 |
| `EndLifetime` | 实例存活、全部实际使用者静止、`Closing` 与清理责任合法 |
| `RetireMembership` | 满足接口声明的成员/遍历条件（不直接等同于底层释放） |
| `Reenter` / `Suspend` / `ExternalRetain` | 满足当前与未来调用者、帧固定及异步/外部关闭协议义务 |

概念访问判定顺序固定为：

```text
check_access(op, state):
    1. 核验 op 自身所需的来源、初始化、范围、访问资格与责任前提
    2. 对每个可能相交的活动依赖：核验权限冲突与对应事件失效
    3. 核验当前调用激活期间的接收者、实参、重借与外部使用者
    4. 按每条结果控制边更新来源、初始化、失效历史 H 与剩余责任
```

未来使用点判定与活动窗口判定共同生效：变量在调用返回后不再被使用，不消除其在当前调用期间仍作为实参或闭包捕获被使用的冲突；强引用窗口的权限冲突不因“本次写入未触发重分配”而消除。

### A.5 协议状态转换与复合资源守恒

- **状态转移语义**：对有限状态协议的每次原语调用，所有可能当前状态均须满足对应前置条件，并沿每条结果边同步更新状态、初始化、地址权限与剩余责任。
- **分量抽象与责任守恒**：不同资源分量可采用安全笛卡尔抽象而不必枚举联合状态积，但权限与唯一清理责任守恒必须独立维持，禁止因分量状态表面兼容而凭空产生两份执行权或两份清理责任。
- **有序失效闭包**：压缩关系（如顺序关闭关系）必须覆盖全部实际先后发生的“失效—使用”对，不得仅核验出口最终状态而忽略中间非法窗口。
