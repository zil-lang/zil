# ThinLTO两case研究原型：非作者静态审查

结论：**v4冻结源码/测量器静态审查无新增阻断，可进入prepare并复核生成protocol。** 这不是N64/N256运行质量通过、编译提速或生产采用结论。本审未import/prepare脚本，未构建、运行编译器/native或新增采样；只读源码、AST、hash及作者已保存的工具/进程功能控制raw。

## 固定身份

研究源码远端`eb5b150903b67d8b9011f6adc2d3ac67b1c419a6`，本地作者`e2e3ce1bc17cb16feafe38ebcf40c60229e2e922`；本地核实tree为`1da99d09dbeb53e59f8df4572f4bc5d010ff5929`，相对main990只有`crates/zil-backend-llvm/src/build.rs`差分。baseline为`99035931d358add0eaed6d4535035c38413701dd`，tree `1c16744d838064cb1c687ba56ef3a0d17ed3130c`。

`prefreeze.json` SHA256：`8b03ff1f30e33a56880c735b3a711c38186dadb2df9f25c5c375a97a37bdc2a6`。其中列出的全部文件已逐项重哈希，所有Python只经AST解析；vendor五文件与恢复Git archive93adb1e7中的`research/compiler-multi-edit-20261111/bounded-imports/runtime-frozen-1679/`逐字节相等。

| 当前脚本 | SHA256 |
| --- | --- |
| processes.py | `ac8cbab5aaa2c2b50a891f5dac28c392dbe5228da6d517749af1b13645abbe04` |
| common.py | `0c2365a939040be32188cfcc9d67accc12834599b47e31c34ba610adeb4e6154` |
| prepare.py | `e72cbc51e96466e697c1e00fe71706763221d0889de2542f817c15a76e47d2cc` |
| clang_wrapper.py | `447420ba7c6538040e2518c0096b9bfa0b8ac7cb1c02b6b3aac86695f249db8d` |
| run.py | `75c28fd4c7c95817ad790d171e5979b5acf6a95930ca7900115f19f420a62b6d` |

## 源码门与真实优化链

最初O0export方案已放弃。当前显式`ZIL_RESEARCH_THINLTO_RECEIPT`环境门要求Clang/O2/incremental-o2/after、Linux Main、8 units/2 jobs、无closed/object cache、全新绝对receipt路径；未开启时原策略选择仍走原分支。原O2前置fold、普通canonical分区和local/address并组均保留，只把预复制callee body的policy设为Disabled，跨unit优化交给后续ThinLTO。没有新增native FFI、改变类型/资源安全门或降低优化等级。

研究门对原完整machine、前置变换后的partition输入及每个实际unit分别validate_module，并保留当前独立语言Verify；外部receipt与native_codegen中的同一JSON必须一致。LLVM verifier没有被混作语言安全证明，也没有把“clang退出0”代替machine preflight。source gate覆盖的是研究路径，不是对所有资源/函数地址/global输入已经验收的承诺。

实际命令已按代码检查：每unit原`clang -x ir -O2 -c`追加`-flto=thin -march=x86-64 -fPIC`；wrapper先保存原IR并opt verify，再编译、保存实际bitcode、再次verify、检查实际summary与HASH，llvm-dis确认triple。link用显式`--ld-path=<pinned ld.lld>`、`-flto=thin -pie`、`--lto-O2/--lto-CGO2`、x86-64 CPU、ICF none、merge-functions true且aliases false；保存实际imports/optimized bitcode，成功必须有8份imports和8份`.4.opt.bc`且逐份verify。路径与命令不是照抄API receipt的假字段。

这与LLVM官方[ThinLTO用法](https://clang.llvm.org/docs/ThinLTO.html)的compile/link双侧thin、显式backend线程控制和独立native cache相符。文档也明确是基于summary的import和并行后端，不能仅凭声明存在推断函数body已导入。工具版本/精确参数兼容性仍由正式运行实际收据判定；本审没有调用工具验证。

工具身份已修复末尾symlink的argv0问题：identity保留绝对调用路径、SHA读取实际字节，不把ld.lld改写成通用lld。已有argv0控制raw明确ld.lld退出0、通用lld退出1、真实rustc退出0；正式运行仍要核全部LLVM工具23.1.4和Rust1.99.1。build binding同时检查source/tree/binary路径与SHA、release成功、build log hash，不能仅靠source_claims字符串。

## CPU、完整计时与运行门

总预算为同一组两CPU；runner设置affinity，wrapper强核继承相同集合。ZIL两worker，ThinLTO jobs=2、lld threads=2，Rust也继承同两CPU。线程数不是额外CPU许可，没有使用额外核心换加速。A是main LLVM API full O2，B是Clang/ThinLTO研究链，R为真实Rust O2；producer工具与管线不同，比较结论必须依赖功能和真实运行门，而非仅同“O2”名称。

完整B请求计时包含ZIL前端/Verify/preflight、导出/序列化、每wrapper重哈希、LLVM prelink/thinlink/backend/native link及检查。各子命令另留wall/CPU/RSS；它们有嵌套，不能相加后从完整wall扣除。当前仅质量探针，cold编译wall明确不形成编译提速主张。RSS是wait4高水位，不是并发总live峰值。

scope固定为原pure动态整数fixture的`n64/base`与`n256/base`两case，100000轮、seed周期251、acc模1000003；原generator/独立周期oracle没有变，新增producer没有权威地生成期望结果。6次clean编译；每case每arm一次warmup、六formal，另00/ff两功能输入：共54次native，36formal。formal六排列ABR/BRA/RAB/ARB/RBA/BAR各一次，每arm各位置两次；使用全部formal算术均值，未过滤invalid或长尾。

两case都必须逐格满足B/A≤1.10且R/B≥.90，缺格/无输出/异常不能通过，不能用geomean或Rust通过豁免相对退化。运行原型只缩小**输入接口覆盖范围**为两个pure base控制，不降低安全、当前验证或运行门；原八snapshot、函数地址、可变global、资源/闭包控制和生产集成均仍未通过，不能把这次两case Pass宣传为那些范围通过或自动采用。脚本没有执行后续矩阵的代码。

## 已发现问题的v4修复与失败边界

v2的nested measured各自setsid会令外层timeout杀不到实际工具，已指出并在v4改为独立processes.py：外层ZIL/native请求拥有私有session；wrapper内actual工具不新建session，启动前要求getpgrp==getsid==getppid。外层在top-child已reap后也清理残留group；有残留即失败。内层timeout/异常在杀整个本ZIL group前先关闭输出、写process-attempt与raw-metrics，未reap的CPU/RSS保持null，不冒充完整计量。

只读`process-scope-pilot/result.json`及mock source核验：normal、outer-timeout、inner-timeout三个原控制均记录无live工具/孙进程；inner-timeout有真实partial收据（CPU/RSS null、reaped false），outer-timeout时内部来不及落盘的事实也没有补造。它们是sleep进程拥有关系控制，不是性能样本。组内SIGKILL不能保证Python finally复制全部LTO partial；因此不能外推为每种硬终止都保全了所有内部文件，外层失败收据及Incomplete才是确定边界。

正常返回的compile/native先保存raw metrics再parse/validate；失败必须保留已有结果并停止，fatal落phase/case/arm/已完成计数，manifest不能以缺失格Pass。NoGo走已完成NoGo状态；异常/取消走Incomplete。未使用成功子集重算Pass。真正正式protocol尚需prepare后冻结SHA并与工具/source/input/counts再核，本审不提前替那份尚未生成的protocol盖章。

## 正式protocol身份追加复核

随后只读核对`out/next-optimization/thin-protocol-v1/protocol.json`，SHA256为`b5eeba53b6b79b07f825591a6026c3bd568e5a7925fab9744d19a7e603ed514b`，source、全部scripts/tools、build receipt与build log实际SHA均匹配；两case全部640份ZIL/Rust源hash重算一致。实际Rust路径为toolchain的`1.99.1-…/bin/rustc`，lld调用路径保留`bin/ld.lld`，没有抹成通用driver。CPU0/1、jobs2、100000轮/周期251、六排列、6次编译/54次native/36formal及双运行门均与上述v4审查一致；无新增阻断。root已另行启动唯一quality窗口；本审没有启动或复制该运行，也不在运行完成前报告任何质量结果。
