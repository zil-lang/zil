# 编译效率恢复入口

唯一施工分支 `research/full-push-native`。本提交的新完整链为 **未测**：闭合调用实参范围→实际算术/CFG压缩→纯调用属性跨CGU声明，以及原始函数ID/诊断名之外逐字节相等的解码共享。接着先查gates33收据，缺失才集中补测，再跑native直接首改window13。研究默认关闭，独立Verify/原预算/完整O2保持。

最后正式结果属于 f8287f2a9 / window12，30请求/10功能格/4运行格完整且运行质量过，但首改仍输；5586恒假标志没有缩小实际大图，不能算优化成功。[精简负结果](native-direct-edit-chain-20261116/range-flag-only.json)。更早 e402/797/543及128运行退化见[结果](native-direct-edit-chain-20261116/summary.md)，数字不转记当前源码。check54a、原生7e、98fa/021997亦各归原版本。

SDK18/native-link复用已安装版本，不重建LLVM，不全量打包。只比较实际main/当前/Rust，native O2/CGU16/jobs1；完整原5105输入、257域及原运行质量门不改。工业多crate/proc-macro/build.rs与分钟/GB级门未测，全面/main门开放，goal/archive不合main。
