# 完整组件检查点：4b809

冻结编译器 `4b809bbfedb92b23062d5bf151eb75fef4bfcd35`，tree `0cdc093e968f73a7d7295afdb3192686da7e8622`，release18 SHA256 `4774b015885b9de4ecfba375c2c6cd156541bde39ac814e41e960a6dead0c7f4`。研究默认关闭，未合 main；完整 LLVM 增量化及全面领先 Rust 的目标仍未完成。

两块完整逻辑已进入实际消费者：固定 LLVM MIRPrinter 新 header-only 导出，严格域内以完整 typed body/CFI/编号与当前 MachineVerifier 检查代替重复正文打印，header 完整比较保留；真实机器结果身份改用持久叶，值相同停传播，变化定义通过逆表直达全部 CGU 消费位置。旧 SDK/不兼容输入仍走原路，没有删除验证授权或改变 O2/预算。源码审查见 [MIR覆盖证明](full-push-astra-mir-body-coverage-review.json)、[MIR独审](full-push-astra-mir-header-component-review.json)、[下游独审](full-push-stage-consumer-review.json)。

完整组件后集中验证：SDK16 构建通过，DSO SHA256 `3930f308e6d9dd80dbe32fa05713335f518b883b1b24c8008d027722712c153b`。header组件 e117 的 native76/backend244/driver210、strictClippy/fmt/release17 通过；随后仅后端6文件变化，4b809 的 backend247/driver210、strictClippy/fmt/release18 通过。第一次 test-only re-export 缺失导致的 compile101/零测试执行保留，e117 修复，没有削减断言。全部确切源码/命令/日志 hash 见 [门收据](full-push-checkpoint-4b809.json)，不累加重叠数量，不声称本次完整 workspace 重跑。

原425函数、22文件、20×20×16、CGU16/O2/jobs1/cacheall/cap256，一轮 main3ec7/候选交替诊断；候选开启 profiling，没有 late 开关或新 ELF 质量协议。控制器实际 exit0；[独立核验](full-push-profile15-qualification.json)通过34 rows（18测量+16父准备）、17 pairs（15成功严格质量+2安全错误）、180固定Git oracle结果。完整ELF记录相同，LLVM仅允许既有精确 verified 注释规范化。历史覆盖后的可执行字节未重新物理散列，已保存的全部 LLVM blob 已重核。

| 请求 | main ms | 候选 ms |
|---|---:|---:|
| cold | 6070.252 | 7366.303 |
| unchanged | 475.189 | 82.407 |
| private | 1505.298 | 482.141 |
| multi | 1680.898 | 770.604 |
| broad | 5210.058 | 6210.566 |
| positions | 1500.763 | 816.134 |
| error | 341.272 | 49.019 |
| recovery | 1598.693 | 479.128 |
| restore | 734.177 | 219.771 |

冷编译和广泛修改仍慢于同窗 main。此轮不是新的六轮 Rust 对照，不与旧候选或旧 Rust 数字相除，也不把差值归因于单个切片。最新正式六轮三方结果仍绑定 [328](full-push-matched-328.md)：部分编辑领先、冷/广泛/无修改落后，目标保持开放。

当前单处编辑只 Lower1/复用424、诊断1/复用428、15单元复用/1重建；机器函数39复用/3执行。实际冷请求新存 MIR 文本524704B，完整 typed 等状态仍保留，逻辑 receipt 合计30347680B（不是 RSS 或总生命周期上界）。单处新 MIR 文本1310B；并未靠提高 MF 命中数量报告进展。[全部阶段及父历史](full-push-native-profile-15-scopes.json)保留范围定义，嵌套计时不相加。

从最终产物倒查，单处请求482ms中逐对象 receipt 合计262ms（对象阶段269ms，内含 O2优化161ms、机器码发射56ms），机器准备14ms、诊断21ms、分区不足1ms、链接输入绑定91ms、最终链接27ms；冷请求仍执行424MF，优化1686ms、发射2697ms。当前消费者仍有全局准备、当前独立 Verify、受影响 CGU 整体工作、文本聚合及工具链绑定；中央 ReactiveGraph 未全局接管。下一步优先审计现有对象 patch 和真实 AsmPrinter 消费闭包，不能把旧 whole-object 改名拼接或未闭合 owner 原型当成函数级 LLVM 终态。

原始 profile15 为495文件/415601601逻辑B，连同 SDK16、不可变编译器、首败和修复后批次待固定 v21 远端分片及 fresh 恢复核验；目前仍在本环境，不宣称已跨环境备份。[v20](full-push-recovery-v20.json) 已保全此前 late 零收益/质量失败原始证据；旧失败不改写为通过。
