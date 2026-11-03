# 321：Copy tuple 绑定的泛型定义期消费者

依据 ZIL-LANG-1.4 §15.2、ZIL-TRAIT、ZIL-MODES 及 313 的普通 Copy tuple
合同，补齐 Expand 参数化 AST checker 对 tuple binding 的实际消费。原始输入为
`fn identity<T:Copy>(value:T)->T{let(x,_)=(value,());return x}`。

在任何新名字进入作用域前，只检查一次实际 RHS，保留完整符号类型；整个 tuple
（包括 `_` 丢弃字段）必须依据声明约束满足既有结构化 Copy 资格。递归限定为
tuple/name/discard，各层 exact arity、同 pattern 重名、原始 let/var 可写资格及
非 Copy/Address/ref/literal/constructor 的关闭边界保持。未用定义同样检查，
不以 `i32` 等实际特化替代定义期证明；不开放对 T 的数值或字段操作。

只扩 `generic_check.rs` 的具名消费者，不改 320 的 `generics.rs`、Core/Typed
schema、ABI 或默认预算。新增 pattern/type/name 遍历和绑定增长先计费；旧非 tuple
绑定路径不新增费用。交付包括原源 Verify/native、未用定义正反例、既有泛型组合
回归及 Expand 的 N/N−1/fresh 边界，完整语言与产品四目标不因此完成。

## 实际限定结果

源码 checkpoint 为 `6cb0af8a58492ec37689e6128826770cfe5cc1e4`。后续补齐 Unit
的两种原有源表示：`let()=zero(value)` 的显式 `->void` 与 `()` 同为零 arity。
首个补充探针曾实际得到 `E-GENERIC-ARGUMENT`/tuple type 拒绝（0/1），修后保留
原源为未用定义正控。新增名字 set/locals 两个索引的比较和克隆预费，原源 Expand
边界从首轮 354 变为最终 357；旧非 tuple 绑定路径和默认限额不改。

同一 `target321`、jobs1、Rust 1.99.1、LLVM/clang 23.1.4，执行：

```text
source /workspace/.zil-env/env.sh
export CARGO_TARGET_DIR=/workspace/.zil-env/target321 CARGO_BUILD_JOBS=1
cargo test -p zil-driver --features llvm-api --test review194_composition --test generics --test generic_regressions -- --nocapture
git diff --check
```

```json
{
  "review194_composition": {"passed": 9, "failed": 0, "new321_tests": 4},
  "generics": {"passed": 8, "failed": 0},
  "generic_regressions": {"passed": 13, "failed": 0},
  "new321_native": {"backend": "Clang", "optimization": ["O0", "O2"], "executions": 8},
  "original_source_verify": "Pass",
  "unused_positive_definitions": 4,
  "unused_negative_definitions": 18,
  "original_expand_budget": {"N": 357, "N_result": "Success", "N_minus_one": "ResourceLimit", "fresh_N": "Success"},
  "diff_check": "Pass",
  "formal_full_suite": "NotRun",
  "native_windows": "NotRun",
  "rust_product_comparison": "NotRun"
}
```

原 identity/result 字节直接进入 Verify；native 只另附 main 调用 result。其它
实际源验证嵌套/显式符号类型、逐叶 var、RHS 单次求值、跨模块 Copy/F helper。
未用定义反例包含各层 arity、重复名字、只知 T:Copy 而无 tuple 形状、discard 中
StoreElement/FnOnce/Address、错误类型、let 写入、把具体 i32 当 T、对 T 数值操作、
reserved receiver/primitive 名字以及 ref/ref mut/literal 模式。符号资格只来自
声明 Copy 约束，不用某次实例代替；最终具体 tuple 仍经既有 Types/Verify 核。

Rust 四目标反思：本片增加普通泛型组合并保留定义期安全反例；复用原内联 tuple
机器表示，没有新增运行时分配。未做同语义 Rust 运行/内存、维护工作量或编译
成本配对，未建立产品级突破。非 Copy tuple、Address/ref 模式、一般拥有解构、
完整语言与既有 life/Self 未恢复成果仍未完成；不改 schema/ABI、320 写集或产品门。
