# 原生重启首改：按需机器恢复整链

源码 `4e3b48bfd3d5cffe9b54c94f512e27806813e113`，二进制 `30ef1deda6e2b2296fe3037e89c7b2cd61e6369c6b19b19bb12e6b46cdc824bf`。唯一施工分支 `research/full-push-native`，源码已推送。

**NoGo：普通CLI首改 ZIL 1.799秒 / Rust 1.449秒，六组五负一胜。目标未达，不合main。**

| 场景 | ZIL秒 | Rust秒 |
| --- | ---: | ---: |
| 普通CLI冷编译 | 2.882 | 7.924 |
| 普通CLI重启直接首改 | 1.799 | 1.449 |
| 驻留请求 cold | 3.247 | 6.170 |
| 驻留请求 unchanged | 0.062 | 0.028 |
| 驻留请求 private-001 | 1.578 | 1.169 |
| 驻留请求 recovery | 1.542 | 6.267 |

5105函数/102文件，实际main/候选/Rust，同O2/CGU16/compiler jobs1；原30请求/10功能格、另12运行构建/4运行格、六CLI区块36构建完成。候选/Rust功能和运行质量通过；实际main仍ResourceLimit，不算速度胜利。

本批完整实现：先比较当前raw输入/facts再解码输出；闭合标量机器保持共享模板与外层别名；当前callee确定之后才请求class规范化结果并检查完整关系；原始及classed完整机器沿同一个preflight核消费alias视图；完整witness和闭图可达性直接消费视图，裁剪后才物化存活函数，再走原range/scalar/purity/CGU/完整O2。没有新全量持久缓存或Verify权限，研究开关仍默认关。

六组均为5266逻辑机器项→166存活物化项（含声明，逻辑数包含新增class）；全量当前检查仍保留。不能据这个计数声称首改达标。六CLI各阶段独立中位数：Expand91ms、Types89ms、Verify67ms、后端632ms，其中机器产生197ms、类变换159ms、对象127ms；嵌套字段不能相加。

CLI同一时点全局inode去重峰值约27.62–27.65MB/Rust61.15MB；完整编辑历史约29.13–29.17MB/Rust61.55–76.73MB，含实际缓存和最终产物，不含SDK/宿主target/研究文件。相较32没有明显盘量增长，也不能宣称本批大幅减盘。

strict/API117/backend415/Driver267/native publication4/fmt/release及精确SHA Astra独审通过。首次控制器漏传ZIL_TEST_NATIVE_LINK_PROFILE，导致后端1、Driver1、原生发布4在配置查找处失败；同源码补齐已安装profile后仅补这6项，首败与未受影响收据保留。独立Verify、原预算和SDK18保持，没有LLVM重建或全量证据打包。

前轮32 CLI1.768/R1.217秒，本轮1.799/R1.449秒；不同窗口漂移，不能归因单一补丁或宣称整链突破。单次同口径阶段诊断C1.411/R1.629只属诊断，不覆盖六组负结果；Rust明确incremental=1且实际rustc参数已核。该诊断输入/解析75.7ms、前端发布81.2ms、semantic发布84.2ms，后端480ms。高额耗时在不同样本的Types、CT、后端迁移。wait4显示候选进程树system CPU为208–1015ms/Rust207–465ms，尚无阶段内user/system/缺页增量或尖峰栈；不能归因外部调度、异步join或某个Types循环。

工业多crate/proc-macro/build.rs、分钟/GB规模和全编译场景超越仍未验成。当前源端/机器恢复并非仅改动函数才检查，也未实现CGU依赖有效性判定在全部后端工作之前完成。

精简结果：[current](native-direct-edit-chain-20261116/current.json)，本轮[window33](native-direct-edit-chain-20261116/window33.json)，前轮[window32](native-direct-edit-chain-20261116/window32.json)。
