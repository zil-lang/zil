# 最终机器预检候选证据（59d）

最终生产59d8753dc23f0ddfcddbaf5780e3057f3c60b9e9；隔离参考801933a9e8f060d44b8305786d01399527aa6137。生成器保持283c661b原字节，9族、6个AB/BA交错区组。三个正式窗口1080+540+1080＝2700请求/225单元，无编译或原生失败；1890份成功ZIL请求各有当前Verify，9840份对象报告保留当前预检与命中来源，2700次完整4字节native均一致。

完整结论见 [DISTRIBUTION-RESULTS](../../../../docs/research/incremental-compilation/DISTRIBUTION-RESULTS.md)，全部225单元见 [PREFLIGHT-FOLLOWUP-TABLES](../../../../docs/research/incremental-compilation/PREFLIGHT-FOLLOWUP-TABLES.md)。按default/closed/isolation顺序，时间不利45/20/24、RSS不利50/28/34。隔离large-512/O2/unchanged比1.041，探索区间[1.003,1.093]整体高于1，原样保留。大单文件O2无改动的预检232.243→115.835 ms，完整请求1025.708→910.377 ms；分项改善不等于完整编译同倍改善。

独立统计复算4615数值字段及225表行，三个正式档与controls共16997文件逐项对照FILES/旁收据/原始库存。收据与复算脚本在validation/data-review.*及recompute/check脚本中。六区组bootstrap是探索性区间，不是产品门或多重比较保证。

最终23生成族O0/O2共92请求、14真实族O0/O2共112请求，另14 Rust oracle；仅4次原默认ResourceLimit，最终有效原生200构建/1768观察，另210 oracle观察。它们是广分布源码组合控制，计时不并入正式窗口。05e同规模控制另存：它的功能曾通过，但O(V)准备费门槛经自查收紧后已由59d替代，不是最终性能证据。

最终113限定测试、Clippy/fmt及release日志保存。独立43 API/text-reference测试、5087 CFG的完整idom/reachable、5975139块对、1645模块的26320次fresh HashMap完整预检均保持旧801的schedule或首错。精确源码审核和可复算oracle包见validation/review.*及review-fanin-oracle.tar.gz。05e与59d的记录不混绑、不叠加测试数。

raw档保留每次stdout/stderr、时间、命令、源身份及最终恢复源码、公开LLVM诊断侧文件和native结果。program.ll早于分区/内部化，只保存最后一次请求的该产物；不冒充所有编辑或实际各CGU均有单独IR转储。ELF与可再生对象/增量缓存排除，二进制SHA保留；源码/统计可从档案复算，完整二进制身份校验需保留工作区或复构。MANIFEST排除自身，最终交付收据在父目录。

ZIL按现有合同发布program.ll并输出JSON收据，Rust不额外输出LLVM文本；完整CLI时间包含这些成本。默认与封闭分窗，后者显式放弃生成符号的外部导出合同。LLVM版本、直接rustc/单crate、两CPU、页缓存保留、宿主噪声、原生运行在计时外等边界沿用冻结协议；不以当前结果宣称产品全面优于Rust或修改既有产品门。
