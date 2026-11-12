# 完整组件检查点：03b

冻结源码 `03b078b5799fe44d98855f26279f0f09e1d25bf5`，tree `5294dbb3a4c84c7f8ac8647df5b83ff386724c45`，二进制 SHA256 `d0d6b76c543148f44b3856cd41f4317b1da5e589a7735ec814c5bbdebb75630c`。研究继续、默认关闭、不合 main；完整 LLVM 增量化和显著拉开 Rust 差距目标未完成。

本轮删除两块实际重复工作：机器函数捕获的三张 lookup-only 临时树表改为 DenseMap，producer 序号和 MRI 使用链原序不变；Expand 的成功声明 inventory 由真实 Normalize/Restore 消费者共享，兼容私有编辑不再重新分配全项目名字、构建符号/函数/文件表。完整环境、当前 preflight、预算双 fit、不足原序回退和成功后发布保留。全局输入比较和其他遍历仍存在。

按完整组件冻结后统一验证。1453 的 native72/backend242/mixed3/strictClippy/release14通过；随后仅 Expand3文件变化，03b 的 Expand105/full driver210/strictClippy/fmt/release15通过。每个门的确切 SHA、命令和日志 hash 见 [收据](full-push-checkpoint-03b.json)。没有声称当前完整 workspace 重跑，重叠数量不相加。两块实际源码独审见 [DenseMap](full-push-dense-capture-review.json) 和 [inventory](full-push-shared-inventory-review.json)。

原425函数、22文件、20×20×16、CGU16/O2/jobs1的一轮 main3ec7/候选诊断，候选开启 profiling。不是新的六轮 Rust 比较或单切片归因。外层控制器实际 exit0，[完整核验](full-push-profile13-qualification.json)通过34 rows（18测量+16父准备）、17pairs（15成功完整ELF记录与严格LLVM对+2safeerrors）、180固定Git oracle执行结果；独立Verify和原预算保持。

| 请求 | main ms | 候选 ms |
|---|---:|---:|
| cold | 5870.974 | 6801.416 |
| unchanged | 506.299 | 58.918 |
| private | 1411.421 | 476.112 |
| multi | 1715.559 | 680.727 |
| broad | 5605.278 | 6579.049 |
| positions | 1456.119 | 819.219 |
| error | 376.724 | 42.804 |
| recovery | 1910.921 | 454.374 |
| restore | 518.960 | 153.507 |

cold和broad仍慢于main。当前private39机器函数复用/3执行、2FPM复用/376执行；object scope优化151ms/发射64ms，完整请求476ms。cold仍执行424MF/3816FPM，MIR捕获223ms/key88ms；嵌套计时不相加。[全部阶段及父历史](full-push-native-profile-13-scopes.json) 保留原始字段，不用旧候选或旧Rust时间相除。

完整六轮同窗三方结果仍绑定 [328](full-push-matched-328.md)，raw已在fixed v17/v18远端恢复核验。本次profile13及两批门、两immutable编译器和源码已在 [固定v19](full-push-recovery-v19.json) `60b8b494911e85e1eb387f6c387d402c5f6e02d0` 保全：4存储片+索引freshHTTPS逐文件hash/模式与496原文件master全集核验；原payload核过后退役，未声称整份拼回后再运行qualifier或native。

后端全景复核：保留原生owner生命周期有实证，但完整allocator/stdlib/ELF计量与当前provider/symbol overlay未闭合，[owner](full-push-astra-native-owner-review.json)/[emission](full-push-astra-native-emission-closure-review.json)明确原型未启用、无性能承诺。文件变化watch也因mmap不可见和工具/loader未知read-set不能替代当前完整链接绑定，[否定收据](full-push-link-watch-negative-review.json)保留，未添加不闭合快路。下一块在显式、默认关闭的late-closed-executable无ZIL符号导出合同下，于原O2结束后再删无用途副本；当前IR缺少完整源码可见性凭证，不把nonforeign猜成private。保持原内联决策与CGU，仍施工未验收。见 [late裁决](full-push-astra-late-dead-definitions-review.json) 和 [JITLink路线负结果](full-push-astra-jitlink-product-review.json)。
