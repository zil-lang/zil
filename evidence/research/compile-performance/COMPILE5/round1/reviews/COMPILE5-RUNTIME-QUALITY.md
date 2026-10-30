# 编译5：独立运行质量、oracle 与 ELF 核验

## 结论与绑定

**核验通过：39,981 项检查，0 个失败。** 四个运行窗口的全部 240 条正式 native 记录、80 条 startup/warmup 记录和192条编译预检内的短输入运行记录，共 **512 份 native 输出**，均与独立重建的 oracle 一致。实际读取并解析42个 ELF：40个被测程序和2个 ZIL 编译器。没有重新编译或执行任何 native 程序，没有运行基准。

- Actions run：`41086451087`。
- 基线源码：`a715f828261ff4b6ef9c8125623064360745aae4`。
- 候选源码：`951471d5a6e26e15de96e0bfaf91cd3e537a3dd9`。
- 候选 crates tree：`b583bc62e4aa39b206afb27f76f2f8262120fe9d`。
- 审计时间：`2026-10-30T16:36:14.990831+00:00`。
- 原始证据：`/dev/shm/zil-compile5-41086451087/compile4-evidence`。
- 机器可读报告：`COMPILE5-RUNTIME-QUALITY.json`，SHA-256 `bd0d2f7d37fb6228d04a3d73b89878dd551545e9fb32f05746f7238614ef6362`。
- 审计脚本：`audit_compile5_runtime.py`，SHA-256 `f8d85f6b4a2b72c2b42df4e11879e63ed8f9827eda6518b019dbb9b8a358e97d`。
- 被核对的 file index SHA-256：`162f6a39b23c0d7e17515276dda66ced25dcab35ca9ede49bb4ac81403d5985e`。

审查者为 `/root/ct_opt`。这是对协调者运行证据的独立复算和产物核验；审查者参与了 CP19 实现，因此本报告不充当 CP19 的非作者源码审查。

## 核验内容

独立脚本未导入被测 runner。它逐个解释人工树中的标量函数及调用树生成 byte lookup，独立模拟 retained 的历史缓冲、求和、重置与索引逻辑；校验输入生成规则、预期输出和所有原生 stdout/stderr 的实际字节，而非仅信任 `pass` 标志。

四个窗口共29,812个索引内文件的集合、大小与SHA-256通过。脚本还核对提交和 crates tree、冻结 runner/helper 源码、计划与收据、基线/候选编译器身份、预检及正式行顺序、文件前后身份、退出状态和计时有效性，并从每区块同臂两次的均值重算跨区块中位数和固定权重点估计。各窗口均在记录的单 CPU 0 上执行 native 计时。

ELF检查实际读取文件并验证 ELF64、小端、x86-64、可执行/PIE、入口位于可执行装载段、节表边界及非空 `.text`；保留可执行节和数据节的大小与哈希。实际 CGU/jobs、回退和对象缓存行为来自本轮收据。对象缓存命中没有冒充本次 LLVM Builder 或本次 LLVM verification；所有 ZIL 编译报告仍要求当前请求 `verification_runs=1`。

| 窗口 | 索引文件 | 正式运行行 | 全部 native 输出记录 |
| --- | ---: | ---: | ---: |
| rust-o0-cold | 8,836 | 60 | 112 |
| rust-o0-incremental | 10,600 | 60 | 208 |
| rust-o2-closed | 8,836 | 60 | 112 |
| o2-direct-quality | 1,540 | 60 | 80 |

归档传输完整性、全部CI命令/测试、编译耗时统计与 bootstrap 由其他独立审查承担，本报告不重复宣称覆盖这些范围。

## 同配置 O0 的产物身份

O0 bridge 的两臂同为 O0、8 CGU/4 jobs、对象缓存开启、query cache关闭。五语料乘四请求，共 **20/20个单元**的双方记录 ELF SHA-256和大小一致；每臂六个正式样本在同一单元中只有一个 ELF 身份。

该 runner 会删除每序列工作目录，**这里比较的是原始收据中的 ELF 哈希和大小，没有重新读取这240个已删除的 ELF 文件**。候选另行保留的五个 O0 cold 原生程序，其实际读取后的 ELF 身份均与 bridge cold 的候选记录匹配；这提供五个 base 产物的交叉核对，不延伸为所有编辑产物原件都被保留。

同时，20个单元的预分区 LLVM 文本记录哈希均不满足两臂集合相等，每臂六次有六种文本哈希。原始 `.ll` 未保留，无法逐行定位差异。冻结源码 `crates/zil-llvm-api/src/model.rs::Module::to_text` 会写入 `verified-fingerprint` 注释，文本哈希可包含非机器指令内容；这只说明文本哈希差异不等同于机器码变化，不能据此认定本轮差异全部来自某一注释或路径。机器产物记录身份相同的事实单独保留。

此外，五个候选 O0 cold 与 O0 incremental 窗口保留的 base ELF 均相同；五个候选 O2 closed 与 direct O2 的 B臂保留 ELF 也均相同。

## 与 Rust 的运行质量分项

下表是**完整原生运行时间**的 ZIL/Rust 比值，不是编译时间；小于1表示该窗口中ZIL较短。权重和测量输入沿用正式矩阵。

| 运行窗口 | 加权 wall 比值 | 加权 CPU 比值 |
| --- | ---: | ---: |
| O0 cold 的 base 程序 | 0.570814 | 0.568581 |
| O0 incremental 的 base 程序 | 0.551300 | 0.547702 |
| O2 closed | 0.985633 | 0.988256 |

| 语料 | O0 cold wall | O0 incremental wall | O2 closed wall |
| --- | ---: | ---: | ---: |
| tree4x2 | 1.345720 | 1.342905 | 1.028762 |
| tree32x8 | 1.288164 | 1.268106 | 0.988710 |
| tree256x8 | 0.330521 | 0.331363 | 0.980876 |
| tree1024x8 | 0.662101 | 0.617160 | 0.983599 |
| retained | 0.850476 | 0.833578 | 0.977656 |

不能用加权值隐藏分项：O0两个窗口的 tree4x2、tree32x8 均在三个区块中同时表现为 wall 与 CPU 比 Rust长；O2 closed 的 tree4x2 也在三个区块中同时较长，其CPU点比值为1.026310，wall点比值为1.028762。其余O2分项在本窗口略短；有限配对样本不支持全部应用或所有宿主的结论。

O0 incremental 运行窗口执行的是预检保留的 base 程序，不能把它与 cold 窗口之间的运行耗时差异解释成“编译缓存提高运行速度”。实际保留的 ZIL ELF 在两窗口中逐语料一致。

## 直接 O2 对照的配置限制与机器码

直接O2窗口的A臂请求 closed **1 CGU/1 job**，B臂请求 closed **8 CGU/4 jobs**，不构成隔离 CP18/CP19 的运行因果对照。B的 tree4x2 和 retained 实际因 `small optimized closed executable` 回退为1/1，其余三树实际8/4；A均实际1/1。

| 语料 | B/A wall | B/A CPU | A/B 完整ELF | 预分区LLVM记录哈希 |
| --- | ---: | ---: | --- | --- |
| tree4x2 | 1.000454 | 1.002493 | 相同 | 相同 |
| tree32x8 | 0.999643 | 0.996526 | 不同；可执行节不同 | 相同 |
| tree256x8 | 0.989826 | 0.989960 | 不同；可执行节不同 | 相同 |
| tree1024x8 | 0.990614 | 0.989952 | 不同；可执行节不同 | 相同 |
| retained | 0.952312 | 0.956822 | 相同 | 相同 |

三棵较大树的数据节相同；其可执行节差异与分区方式及实际internalization数量不同一致，但此窗口不能隔离这些因素，也不能把差异归因于本轮前端改动。tree4x2 与 retained 实际读取的完整ELF哈希和大小相同。尤其 retained 的wall观察约短4.8%，不能作为本轮实现提高其运行速度的证据，因为两臂实际机器产物相同。

直接O2加权 wall 比值为 **0.991395**，CPU为 **0.990952**；没有语料在三个区块中同时全部出现 wall 和CPU比值大于1。本报告将其作为该固定窗口的质量观察，不将其解释为CP18/CP19运行加速，也不替代同配置O0的产物身份检查。

## 执行与保留

普通exec视图看不到协调者解包所在的IPC/tmpfs，首个调用在读取样本前因目录不存在而停止；原始日志保留为 `COMPILE5-RUNTIME-QUALITY-first.stderr.log`，该次没有产生样本或质量结论。自动审批通过后，使用能看到相同证据的 elevated只读执行环境运行原审计脚本，正式返回码为0，stdout/stderr保留为 `COMPILE5-RUNTIME-QUALITY.stdout.log` 和 `COMPILE5-RUNTIME-QUALITY.stderr.log`。

本次只读取证据与精确Git对象，并写共享工作区内的新报告。未改原artifact，未重新解包或复制大树，未执行原生程序、Cargo或性能负载。辅助探针也没有在本核验中执行。
