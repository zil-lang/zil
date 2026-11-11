# 多文件多点修改：已准备、未运行

这套草案沿用仓库`distribution_bench.py`的完整CLI `wait4`计量方法、Rust O2对照形式及Source/原生输出验证，驻留场景直接使用PR78真实CLI JSON stdio，不新增probe或生产代码。v1首个clean oracle暴露ZIL不接受Rust式unsafe表达式，0测量样本首败完整保留；v1脚本和协议另存archive-v1。v2仅将ZIL main改为unsafe语句block，Rust、动态算术及oracle不变。baseline实际64/256 base功能pilot均完整O2、Verify1、8unit当前预检及LLVM before/after verification通过，00/41/ff三输入各符合原oracle，证据在functional-pilot-v2；这不代表批量编辑矩阵或Rust已执行，也不是性能结论。

## 输入和场景

- N=64/256/1024 表示每种语言的实际总文件数，含 main。默认只准备 64/256，1024 必须显式选择。
- k=1/4/16/64，固定分散选点；每个选中文件修改两个分离的函数体。N64/k64 会修改 main，始终是 64 个文件。
- 默认 dynamic-body：程序从 getchar 取得实际运行输入，所有叶子接收该输入，每个函数做四步不同系数的有界算术；避免把全程序输出在编译期折为一个常量。独立 Python 模型固定检查输入 00/41/ff 的完整四字节输出。scalar 仅作旧屏幕对照。
- 全部文件写完并做 rename 后才发一次完整 O2 请求；没有每改一个文件就编一次，也没有编译期间写输入。文件系统写入不是跨文件原子事务，这里保证单个编译消费者只在最终状态开始读。
- cold-cli：直接编最终编辑状态，空对象缓存；warm-cli：新的 CLI 进程先编 base，再以相同对象缓存编最终状态；resident-cli：同一真实 CLI 先收到一次 base compile，再收到一次最终状态 compile，统计完整外部响应时间。
- call-edge/signature/cfg 是独立结构变化控制，不混入常量编辑的固定频率加权均值。CLI 无 Limits 覆盖接口；预算 N/N−1/fresh 必须通过候选现有的独立 API 测试另验，协议保留其具名位置，未冒充本 screen 已执行。

## 固定协议和第一轮范围

`prepare.py` 只生成输入和不可复用运行身份的 protocol.json；`screen.py` 默认不执行，必须显式 `--execute`。协议固定文件数、修改数、ABBA/BAAB、轮数、CPU0/1、场景权重、工具脚本摘要和所有源文件摘要。完成后再修改脚本会使旧协议拒绝执行，应生成新的协议目录，不能改写已有协议。

最终第一轮冻结为64/256文件、k1/4/16/64、cold/warm/resident三态、ABBA与BAAB两块，分别固定incremental O2 merging off和after。每cell每arm四个测量点，observed p95样本很小，只用于判断是否继续正式矩阵，不声称通用收益。每策略192次测量+128次seed+10次无缓存clean oracle=330次ZIL编译；两策略共660次。可选Rust每cold/warm cell两点，每策略32次测量+16次seed，两策略共96次。default O2 merging off/after先作功能与缓存控制，不执行四策略完整矩阵；结构控制由具名独立测试承接。最终协议见`final-inc-off-v2/protocol.json`与`final-inc-after-v2/protocol.json`；v1及其首败只供恢复和对照，不能混用。

```bash
python3 out/multi-edit/prepare.py --out out/multi-edit/final-inc-off-v2 \
  --files 64 256 --edits 1 4 16 64 --blocks 2 --controls \
  --cpus 0 1 --frequency-policy observed-uncontrolled --policy incremental-o2
python3 out/multi-edit/prepare.py --out out/multi-edit/final-inc-after-v2 \
  --files 64 256 --edits 1 4 16 64 --blocks 2 --controls \
  --cpus 0 1 --frequency-policy observed-uncontrolled --policy incremental-o2-after
```

协议里的scenario_weights是预声明的假设请求发生频率，未声称是用户实际分布；完整三场景固定cold=.10、warm=.45、resident=.45，文件规模和k等权。选子集时只对选中场景按比例归一化。主指标先对每个cell求绝对wall算术均值，再按固定频率加权，最后求B/A；同时保留block median和observed p95。控制、seed都不进入该加权分数。

CPU 时钟频率是另一件事：当前环境没有可读 cpufreq 接口，协议显式记录 observed-uncontrolled，不声称锁频。`fixed` 模式只有在每个声明 CPU 的 min=max=指定 kHz 可核时才允许执行，不修改宿主设置。

## 执行示例（此交付没有执行）

先加载现有`toolchain/env.sh`。A/B二进制、确切源码身份及构建收据须由协调者绑定；runner不把手填源码SHA当成已证明的二进制来源。baseline已知路径为`targets/baseline/release/zil`；候选使用协调者本轮新冻结生产SHA实际构建的产物，先前结果独立保留。

```bash
python3 out/multi-edit/screen.py \
  --protocol out/multi-edit/final-inc-off-v2/protocol.json \
  --baseline targets/baseline/release/zil --baseline-source BASELINE_FULL_SHA \
  --candidate CANDIDATE_BINARY --candidate-source CANDIDATE_FULL_SHA \
  --out out/multi-edit/runs/inc-off-UNIQUE_ID --execute
```

可加 `--rustc /absolute/rustc`，接入同语义真实 Rust source，而非另造计算 microbenchmark；固定 Rust1.99.1、O2、overflow checks、panic abort、8 CGU、每序列独立 incremental 目录。Rust 只参与 cold/warm CLI，无驻留实现；不捏造一个 Rust resident 值。CLI/驻留内存口径不同：CLI 用 wait4 最大单进程 RSS，驻留记录请求后 RSS 及包含 seed 的生命周期 HWM，二者都不是聚合进程树瞬时峰值；CPU 口径也写入协议。

每个ZIL成功请求必须有Success、verification_runs=1、N个实际source_files、非空且数量匹配的LLVM API unit receipts；每unit必须完整default<O2>管线、LLVM23.1.4、当前preflight_ns>0，cache hit要有producer verification，builder/patch要有LLVM before/after verification，patch额外核current_full_machine_preflight=true。seed也验证，不能缓存Verify。每个snapshot/策略先用baseline做一次fresh/no-object-cache完整O2 oracle；之后每个A/B seed/edit都必须与oracle ELF字节SHA一致，sidecar仅删除唯一`; verified-fingerprint:`诊断行后SHA一致。不同码即失败，原生输出相同不能覆盖该失败。每次request计时外记录objects/partition-layout-v1/v2里的name/size/SHA前后集合，用于核真实recipe复用，不强制MachineCost已变化的输入仍同key。

所有失败保留当次输出并终止，不自动重试或筛掉失败样本。JSON损坏、原生超时、resident reset等提前异常也写`fatal-failure.json`，含phase/cell/arm、已落盘rows、已完成oracles、已得metrics、异常和UTC时间；manifest变为Failed/Incomplete，未执行请求明确不计Pass，也不会输出成功汇总。完整resident原始stdout单独保存。

这只补编译 screen。已有完整当前机器预检、独立 Verify、full defaultO2、≥Rust90%运行目标及10%相对运行回退门保持独立必过，三个小输出 oracle 不替代运行质量验收。baseline/candidate 默认相同选项，after 策略明确标注且不等于低优化等级。

## 第二阶段：并发候选，尚未执行

一次多点请求和多候选并发分开。第二阶段另冻 protocol：同一个 base 上 c=2/4 个候选，每候选 k=16，各自 source/output/object-cache/stdio resident 进程，全部只编最终状态；同机器资源总上限仍为 CPU0/1，两候选和四候选共用该上限。记录每候选 wall/CPU/完整输出/Verify/preflight，以及 all-complete wall、总吞吐、每候选 p95；不要把不同候选的补丁合成一个请求，不共享可写输出，不用串行 c 次的计时冒充并发。当前 screen.py 只执行第一阶段，协议明确标为未执行并发轴。
