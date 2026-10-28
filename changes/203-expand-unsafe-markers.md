# 203：保留原清理 marker 的有限显式 unsafe 检查

规范绑定：`1355ae0d2351199391d65ca4faa4a84ce768ece5c8aaaca465b8de5b20aad65a`（§8.2.2、§12.4）；root 在 2026-10-28 明确批准写集内此接受性增量。

原 `04_mode_cleanup_markers.zil` SHA-256 为 `4014fa77d574a6a5f008a49bdb89e49c42736195ca9c4babc4ac8bf04ea74000`。第一次双绑定实现已令原 01/02/03 的 Expand 预期通过，但 04 准确停在模板定义的 `StmtKind::Unsafe`。原字节保留，不把中间 marker 移到别处来避开检查。

仅新双 callable owning State 的定义允许显式 `unsafe` 块进入既有完整参数化 body 检查。块内独立词法局部表通过已计费 clone/join 保留外层实际 move/Store 失效事实，局部名称不泄漏。外部函数签名在原已计费 item 遍历借入独立只读索引，旧路径不增加扫描或预算扣费；外部调用走独立分支，不能冒充纯 ZIL helper。此新增模板通路仅处理 literal C ABI 的直接标量调用（i32/i64/c_handle 参数及这些类型或 unit 结果），不引入环境、State、引用、函数指针或 callable 的 FFI 擦除。

所有实参、结果与真实声明核对，`unsafe` 外调用仍拒绝；未调用模板同样检查。Core 中的原 Extern 声明和显式 Unsafe 块保持，后续 Types/Verify 的 ABI、unsafe、效果和责任检查仍执行。测试将覆盖同一 04 删去 unsafe、未调用定义、实参/结果不匹配及 F/G 通过 FFI 的邻居。原始输入的 Native 清理次序 `MPBQZA` 在实际运行前继续 NotRun。

此文记录接受范围；尚未宣称实现或测试通过。实际结果以随后限定命令收据为准。
