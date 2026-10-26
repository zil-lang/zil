# 107：完整构造身份的实现、实测与净回退

## 状态和范围

本片已完成实现、有限功能检验和唯一正式成本确认，**准入决定为 Inconclusive，
生产实现及 crates 测试已净回退**。预先指定的 modules512 Verify 完整进程
ON/BASE 为 0.747634，名义 95% 同时区间 [0.732010, 0.771199]；这是保留的真实
有限收益，不是普遍退化。但 95 个端点仍有 32 个上界大于 1.01，没有满足原全矩阵
准入条件。全部 19 个完整进程和 76 个 API 端点见 `docs/research/107/RESULTS.md`。
本文件为作者交付，非作者审核和集成由 root 完成；权威范围仍为主树 `.task/impl/107.md`。

提案/九闭包证明提交为 `87174539a778b74bf7ed6e51fa0ba42746804212`；生产实现
及测试为 `bbe8dafb570b613f8f5cf9be413882928a8bd511`。后续 `6753959f` 和
`fe37e8fc` 只增加/修正实验工具与合同，不改变这一生产源码。正式 BASE 是工单
基线 `b9cce1d1b7301850a3e65b81323caea8ea88593c`，准备器已逐字核对其 Cargo
及 crates 与 `24ca9583d8789cfdfe3ccdbfe3eeff614bf56429` 相同。

## 保留候选的实现

`session/construction.rs` 用独立域、八种显式 raw 种类、完整 QueryKey 和有序
实际依赖 key/kind/observed 生成私有构造身份。穷尽结构解构防止字段增加时静默
漏入；变长字段有长度、依赖有数量，保持依赖顺序。成功构造并通过取消检查后
才发布。失败当前阶段仍 abort，成功前缀的合法条目可保留。

这个值在固定编译器/单个会话的确定性闭包前提下使用，不是 IR 内容摘要，也不
可跨会话导入对象或授予 Verified 权限。原 `RawSnapshot::digest` 行为保持，
私有编译期常量可在冻结实验副本中关闭优化。公共 TypedModule 指纹、IR/wire、
导出接口、证书、Verify、数据库通用合同、磁盘键/格式、规则串及预算都不改。

九个生产 query 闭包和特殊 loaded_syntax 来源的逐项证明在
`docs/research/107/INPUT-CLOSURE.md`。它明确覆盖源 FileId/ModuleId/路径/文本、
项目 root/有序模块/完整 paths、全部 Limits、build_inputs 和目标全部字段，
不以“以前缓存也如此”代替封闭性证明。

## 有限源码与原生检验

`out/runs/107/limited-002` 记录实际命令、stdout/stderr、源码、完整阶段与 wire
产物和原生 executable。格式检查和 scoped clippy 通过；23 个库测试、6 个既有
068 项目测试、1 个新增原生测试，共 30 个测试函数通过。这里没有运行正式全量。

四个新增库测试通过实际查询检查每个源/项目/预算/目标键、依赖字段/数量/顺序及
八种 raw 种类；热会话和新会话的公共结果逐项相同。保留 37 份完整请求报告，
其中 34 Success、1 Unsupported、2 SafeReject；后三项分别是未知 backend、
错误 rule_version、关闭 checked_arithmetic，不能把拒绝请求写成正例。

新增实际两模块消费者用 Package 拥有 ByteBuffer 并保存 FnMut 闭包。O0/O2 下
各跑冷、热、显式 ProjectSources、真实被调用私有体修改、修改后新会话和恢复，
共 12 次原生执行。原输出字节 `[6,13]`，编辑为 `[7,14]`，恢复原输出，exit0、
stderr 为空。十一项阶段/接口/LLVM、文件加载与显式项目路径均逐字相同；热复用
仍独立 Verify 一次，编辑实际重做 Types，恢复可复用旧 Types。

## 全矩阵 source/plain/count

准备目录 `out/runs/107/prepare-002`，build identity SHA256：
`fb42167556dce910b77d8d2fed3eb3456a7f993f1e78697f25123d2c468655e7`。
三个变体分别为 BASE、仅改常量的 OFF、启用构造身份的 ON；每组单独构建无探针
plain 与注入 count 的 executable，冻结原始源码副本/manifest/工具真实路径与 SHA。

19 模式单元全过，共 456 个完整请求和 72 次原生执行；另有三组各 142 个既有
guard 请求，共 426，请求数与测试函数数分别报告。三个 guard 的分析边界均为
3122、CT 边界均为 12，所有 guard（含失败和分析预算）完整一致。

三组 plain 的公共字段、诊断、接口、所有阶段产物和原生预填输出相同；同组 plain
与 count 的阶段产物（含 LLVM）逐字相同。BASE 与 OFF 的真实工作量和 raw 身份
完全一致。ON 不再调用旧 digest，其每个实际发布身份由独立 Python 编码重算；
依赖 observed 逐条追溯到真实前序发布，DB/Types/Analyze/Verify 工作量仍一致。

| modules512 请求 | BASE/OFF Debug 字节 | ON 构造编码字节 | 成功发布数（两边相同） |
|---|---:|---:|---:|
| cold | 3,414,920 | 153,918 | 517 |
| unchanged | 0 | 0 | 0 |
| private_edit | 3,163,118 | 153,766 | 516 |
| public_edit | 3,423,600 | 153,918 | 517 |

这是实际工作计数，**不是速度表**。例如 private_edit 仍有 516 次成功构造，
本片没有把整体项目改写成细粒度增量，也未消除其余 Parse/Expand/CT/Types 成本。

首次 `prepare-001` 因移植夹具漏定义场景常量，在构建前以 NameError 失败；完整
输入及失败命令/stdout/stderr 保留。`fe37e8fc` 补本地常量后用新的 `prepare-002`
执行，不覆盖失败，不称它为自然编译器缺陷。本轮未发现自然编译器失配。

## 唯一正式确认和最终处置

`docs/research/107/CONFIRMATION.md` 固定只比较 ON/BASE 一次：19 单元、每单元
2 预热对及 64 个 ABBA/BAAB 块、总 4940 进程 / 19760 请求。95 个完整终点用
既定 1% 线和第 18/47 次序统计同时区间；主要收益预定 modules512 Verify 完整
进程。OFF 只做功能/工作消融，**OFF 耗时未测**。本合同的具体判定将噪声导致的
精度不足记为 Inconclusive，并不授权生产准入或扩大样本。

controller 从 101 修订 `fa26b97dd2778b750fe34578c345391097e36897` 静态移植，
并同步 `00e279a40d1186efe6eb488e142627874c5aa0f4` 的完整 JSON 严格类型封口；
合同只使用此前独立 A/A，不读取 101 的候选 R 结果来调节。107 自己绑定代码、
build identity、功能控制、顺序、每条原报告和 runner；29 份合成坏原件被拒，
4 个正常完整进程/16 请求的实际 capture 通过，10 份被改参照被拒。

root 授予 formal107-ON-001 独占窗口后，实际采集于 12:27:50Z 开始、12:37:07Z
结束，4,940 进程/19,760 请求完整退出 0。随后才分析，95 项全部保留；没有明确
超过 1% 的回归区间，但 32 个上界未满足 1.01、7 个点估计超过 1.01，最终
Inconclusive。没有删热请求、扩样或重复正式采集。全部逐项结果和顺序/前后半诊断
见 RESULTS.md 及 evidence/impl/107/confirmation-summary.json。

净回退提交 `3b84952ecff4c0467b779d651a41e699449b8e80` 将 4 个生产/测试文件恢复，
整个 crates 和 Cargo 对 BASE 的净差异为空。候选和所有实验工具仍保留；完整原件
47,922 文件/697,926,176 字节归档为 raw-final-001.tar.gz，逐成员读回校验通过。
这不代替 root 的独立审查，也不把作者记录写为生产批准。

四目标边界：安全行为已有上述有限证明/源码/原生控制；运行期程序输出不变，
本片没有测运行速度；编译已实测固定输入上的完整净成本收益，但严格全矩阵未准入；
表达和维护成本没有新增产品比较。它解决已有真实热点的一段工作，不能替代语言
或产品目标，也没有测与 101 或 108 的组合。
