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
