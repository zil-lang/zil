# 第五阶段广分布冻结证据（801）

生产实现801933a9e8f060d44b8305786d01399527aa6137，测量工具冻结提交283c661bcb18978fe048770dace4994ade1910d2。implementation-ablation的参考为第四阶段生产9fe5f55e，另外四窗参考为Rust1.99.1。后续机器预检候选另有源码和窗口，不能将本目录时间改绑到它。

五窗共7824请求、652单元，每臂6个AB/BA交错区组；另28个Rust O0参考构建。body-64x16与types-64x16共240次预登记ResourceLimit形成40个无效单元，其速度比为null。其余612单元保留所有有利与不利观察。4392份ZIL报告中4152次成功请求各有当前Verify=1；5928个有效原生构建合计20040次原生观察，加420次oracle观察。15输入不代表15条独立路径，部分程序忽略输入。

独立数据审查逐行复核全部结果、来源与当前对象预检，重新计算10912个统计字段，核对652个表行；五个正式raw档共26524文件与内部FILES/旁收据/原始库存逐项SHA一致。独立审查者为/root/review_incremental_slice1，完整收据和复算脚本在validation/data-review.*、recompute-stage5-data.py、check-stage5-archives.py。wall不利单元按五窗顺序为187/51/21/0/0；RSS不利194/67/48/0/0，不以模型吞吐或对象命中数替代完整请求。

- [完整652单元表](../../../../docs/research/incremental-compilation/DISTRIBUTION-TABLES.md)；analysis.json包含双臂原始六值、峰值RSS、探索配对bootstrap及对象计数/分项。
- [报告与边界](../../../../docs/research/incremental-compilation/DISTRIBUTION-RESULTS.md)；[图切割/zit归因](../../../../docs/research/incremental-compilation/ALGORITHM-ATTRIBUTION.md)。
- 每窗manifest/summary直接保存，raw.tar.gz包含时间行、所有stdout/stderr、源身份、最终恢复源码、公开LLVM诊断IR和native结果。program.ll是最后一次请求的完整模块诊断产物，早于分区/封闭内部化，不冒充每次编辑或实际各CGU都有独立IR文件；实际输入由固定编译器、源码/选项和对象action key绑定，内部化另有符号/Clang实际输入控制。源码编辑可由冻结生成器和逐请求身份重建。
- diagnostics保留初版工具故障、弱oracle预检、全部预检/容量失败及协议故障注入。它们不算正式性能窗口。validation保留112项801限定测试、Clippy/fmt、三份精确生产独审、协议审查、zit原源码/机制探针/结果及原CP25来源。
- ELF、对象、共享库、bitcode与Rust/ZIL对象缓存未打包；完整二进制身份检查需要保留工作区或按源版本复构。原始统计可从档案独立重算。包内FILES与包旁receipt绑定内容，MANIFEST绑定本目录（自身除外）；最终交付绑定收据在父目录，避免自引用。

时间为完整CLI（含ZIL JSON收据输出、链接/发布），原生运行在计时外；check的Rust边界是metadata。OS页缓存未清，宿主噪声/温度未严格控制。LLVM为ZIL23.1.4、Rust内置23.1.1；直接rustc、非Cargo多crate。六区组bootstrap只作探索性描述，正式产品门与main集成状态未改变。
