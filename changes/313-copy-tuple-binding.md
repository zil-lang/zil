# 313：有限 Copy tuple 普通绑定

## 提案

依据 `_zil_design.md` §15.2 与 `spec/grammar.ebnf` 的 Binding/Pattern，
将原先 Unsupported 的普通 `let/var` tuple pattern 贯通到现有 Copy 聚合指令。
允许嵌套 tuple、名字、`_`、显式完整类型和 Unit pattern；每层必须精确匹配
真实 tuple arity，名字不能在同一 pattern 内重复。RHS 在任何 pattern 名字进入
作用域前求值一次，所有名字随后进入原词法作用域，var 资格逐叶保留。

Core 新增不可信 TupleBinding，携带 CorePattern、mutable、可选完整类型与必需
initializer。Types 检查完整 Copy 资格及每层形状，再用既有聚合投影、局部存储
发布叶绑定；不改 TypedIR、Wire、机器 ABI 或默认预算。不用隐藏块吞掉作用域，
也不以伪造条件/自赋值编码形状约束。ref/ref mut、literal/constructor 普通绑定、
非 Copy/Address tuple 继续拒绝；全 `_` 不能绕过完整资格检查。

最近反例覆盖少/多元素、嵌套 arity、非 tuple/空 pattern、重复名字、类型不符、
let 写入及 ref；原 pending/runtime-tuple-binding.zil 字节原样通过实际入口测试。
限定测试放已有 aggregates.rs，不建新 integration target。

## 实际结果

2026-11-03，本轮独立 target313、jobs=1、Rust 1.99.1、clang 23.1.4：

- `cargo check -p zil-driver --offline` 通过。
- `cargo test -p zil-driver --test aggregates --offline -- --nocapture`：16/0，
  包含 6 个新增测试；3 个新增真实源各执行默认/O2，原 pending 文件字节不改。
  13 个最近拒绝输入通过；新投影经 TypedIR/Wire 独立核验及伪造下标拒绝。
  Types 的 analysis 精确边界 N=51，通过 N、fresh，N−1=50 返回 ResourceLimit。
- `git diff --check` 通过；未跑全量、参考后端、Windows 或同语义 Rust 性能配对。

实际 Core 消费者扩写经 root 授权：zil-types 的 lib/tuple_bindings、inference、
aggregates、callable_fields、closures/package_effects、deferred、helper_plan、
incremental、managed_slots、names、product_replacement、resource_effects、retained、
scalar_shapes、state_origins、store_templates；zil-ct 的 materialize/source；
zil-driver 的 ct_program。它们仅增加新节点的类型/RHS/pattern 遍历；不改变旧规则。
标量增量 fingerprint 与 CT unchanged 快路遇到新节点回退正常检查，不跳过 RHS。

Rust 四目标：有限 Copy/权限拒绝和独立核验有本轮实证，完整安全未完成；机器结果
与内联 extractvalue 有证，未建立 Rust 运行/内存比较；普通嵌套绑定与闭包组合推进，
框架维护收益未测；新增工作有 51/50/fresh 边界，未建立编译成本优势。实质突破为
原 Unsupported 源字节原样进入实际 O0/O2；不升级 B0、完整语言/ZUI 或产品门。
