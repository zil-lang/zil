# 189 — 显式 Address callback 参数

## 基线与授权

以 `91feaa919bf3f02a36b50bf5e13cbddec4ede87b` 为明确候选来源；主线集成方已授权本片，正式状态和写集由主树 `.task/impl/189.md` 维护。本提案不把该组合候选、187 在途修复或 188 在途嵌套 helper 当作已验收。实施树为 `wt189`，分支 `impl/189-address-parameters14`；最终源码由非作者独立复核。

只读预检证实：已注解 `p: &i32` 仍被推断为标量参数，`*p` 在 Types 拒绝；`p: &mut i32` 被 finite Copy 门拒绝；未使用的共享参数在直接调用约束处发生地址/标量失配；未实例化和跨模块 helper 仍被旧模板门拒绝。普通 Address 函数与 Copy callback 对照通过。探索中的保留解析错误只属于夹具修正，不计作语言失败或验收。

## 准入范围

在现有 Fn/FnMut/FnOnce 环境模式下，callback 参数可为现有 finite Copy 或一层 `&Copy` / `&mut Copy`。只要签名含任一 Address 参数，结果必须为 Copy，且不得声明环境 lending 来源。既有 184 的 Copy 参数与共享环境 lending 结果保持原义。`&mut` 参数只授予此次调用的重借，不把 Address 标为 Copy，也不消费调用者的原借用。

不开放 function pointer 的地址 ABI、显式资源/Package/slice 参数、嵌套 Address、从显式参数返回借用或异步能力。对这些最近邻输入仍给出明确拒绝。接收共享参数时可以从独占来源重借为共享，不能从共享来源制造独占权限。显式参数的写入不自动把只读环境的 Fn 升级为 FnMut。

## 实现与独立核验

Types 修复同一个已定型签名在前置约束、闭包局部推断、函数体构建、借入 helper 和不透明 factory 返回之间的传播；实参路径沿用普通参数的 `argument_expr` 重借语义。Analyze 独立检查新形状并保持资源、Hold、预算边界。Verify 独立检查定义与 entry 参数精确一致，并对各个实际调用形式统一核验当前实参对、Address 权限、捕获来源与实参来源之间的冲突。

现有 IR 已保留完整参数 Type、entry locals 与实际调用 operands，Wire 和指纹已有完整 Type 编码，因此维持 schema23。公开 `CallableTrace` 继续只表达有限调用模式轨迹；不拿 `hold`、`writes` 或 `lending_captures` 偷运显式参数来源。

187 正在修复私有 Capture/ParameterCapture 的分离义务和 helper 的实际效果替换。本片先修改独立签名与实参路径；待 187 精确冻结后串行吸收其 `captures` / `buffer_effects` / `roots` 逻辑，再完成 Address 调用权限合流。188 的嵌套 helper DAG/traces 算法由其作者维护，本片只修改相邻签名/额外参数准入。不得把未知 helper 外部效果视为空，也不得仅凭不同形参名证明不别名。

Lower 与 LLVM 只准入已核验的现有指针参数表示，沿用直接传参，不加入堆盒、RC、锁、全局表或隐藏类型身份。所有新遍历与集合处理沿用现有有界预算，不提升默认额度。

## 验收计划

- 真实源码覆盖直接调用、借入 F helper、跨模块 factory/helper、Fn/FnMut/FnOnce、共享/独占参数重借、多参数、同一来源共享读取、不同来源对照和最后使用后释放。
- 最近反例覆盖当前最后一次调用中捕获 Desc 写入与实参共享窗口相冲突、捕获 Address 与独占实参相冲突、重复独占实参、共享转独占、借用结果逃逸以及仍未开放的参数形状。已开放的 187 ByteBuffer 路径在吸收其冻结后做同类组合；此前不当作通过。
- 原生 O0/O2 运行检验实际读取/写入、调用顺序、环境清理和错误退出；需要分配的 owning 环境保留故障清理证据。
- raw TypedModule 与 Wire 往返独立攻击参数/entry 不一致、实参权限伪造、同源替换、隐藏 Desc 写入、丢失或伪造 Hold/来源。独立 Verify 不消费 Types/Analyze 的求证结果。
- 限定相邻测试、fmt、生产与新测试 Clippy；新的输入规模做实际有界失败对照。正式全量与合入由集成方执行。

## Rust 四目标与终态

安全性以当前调用的真实来源和独立核验为准；零额外运行时成本以实际 LLVM 指针参数和原生程序为准；组合能力以同一实际 callback 经直接调用和跨模块借入 helper 为准；开发反馈以最小源码诊断和可绑定证据为准。拟突破点是让资源描述捕获与显式借用参数在同一调用统一核验，而不是只增加语法准入。实现前均为计划，不宣称全面优于 Rust；完整语言及产品终态保持未满足。
