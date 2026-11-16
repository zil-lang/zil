# 编译效率恢复入口

唯一施工分支 `research/full-push-native`。最后正式窗口13属于 **8ad2bfb5c9604dfdd761dd9fd48ebab7a94fe547**：原生30请求/10功能格/4运行格完整、运行质量通过；直接首改 **2.817s / Rust1.483s，仍落后**，冷编译4.537/R7.715s。完整参数范围→实际CFG压缩→跨CGU纯调用事实已贯通，函数374完整解码、4727复用。用户边界缓存/产物峰约29/R59.5MiB，仅本生成项目范围。[精简结果](native-direct-edit-chain-20261116/closed-range-cfg-decode.json)。

本批新 Analyze 投影共享、独立 Verify 当前请求叶函数成功复用与后端单元剪枝合并 **未测**。先核HEAD与gates36实际收据；缺失才集中补测受影响模块/strict/fmt/release，再跑native直接首改window15；不要重复整套未变模块。旧窗口12恒假标志未缩实际图的负结果及09运行退化都保留。

SDK18/native-link复用已安装版本，不重建LLVM，不全量打包。仅比较实际main/当前/Rust；native同O2/CGU16/compilerjobs1，原5105输入、257功能域及原运行质量门保持。研究默认关、当前完整输入/原预算/独立Verify保持。计时为resident请求、启动单列，不称普通CLI完整启动退出或OS冷。工业多crate/proc-macro/build.rs与分钟/GB级未测，全面/main门开放；goal/archive不合main。check54a、原生7e、98fa/021997各归原版本。
