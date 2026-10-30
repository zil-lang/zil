# COMPILE5 辅助探针 v2 独立审查

**原始证据和逐 case 算术复核通过；性能解释为尚未收敛的风险信号。** 没有重跑测量、删除样本、选择较有利区块、跨 case 加权或与正式 CI 合分。主矩阵 JSON/MD 已单独完成，本次没有重新哈希主矩阵数据。

- A：`a715f828261ff4b6ef9c8125623064360745aae4`；B：`951471d5a6e26e15de96e0bfaf91cd3e537a3dd9`。
- 原始目录：`recovery/probes/run-v2`；独审机器结果：[COMPILE5-INDEPENDENT-AUX.json](COMPILE5-INDEPENDENT-AUX.json)。
- 独审程序：[COMPILE5-INDEPENDENT-AUX.py](COMPILE5-INDEPENDENT-AUX.py)，不导入执行器、生成器或原统计函数。
- 冻结执行器 SHA-256：`bd317478a54fee5ee99de2a21a6b5bdfb15a2df85b8a31492f4c6968234bf835`。

## 冻结口径与证据

已核对 v1→v2 只有环境要求修订，三份源码、oracle、规模与36行顺序保持不变。6条预检和36条正式编译记录全部通过；逐条检查了计划/完成收据、命令、源快照、原始CLI/native日志、当前Verify、实际链接器、cold缓存状态及保留产物。123项CPU求和/统计比较和原结果完全一致，最大差为0。

三份源码各有2模块；类型为53节点、32标量叶、最大tuple arity=4。multiple-rewrites两处nominal叶展开后为55节点。前两case各64个返回函数，repeated-hints有128条consume调用。独立按三处值均为1计算，stdout应分别为单字节 **192 / 192 / 133**；42次native日志与该oracle一致。三份源码都没有`#` VM查询。

工具原字节、大小和当前文件元数据与冻结值一致；A/B编译器与CI构建收据绑定。LLVM/Clang/LLD为23.1.4，记录的实际DSO路径和链接器alias一致。每次均为全新cold进程/缓存目录、O0、请求8 CGU/4 jobs、关闭query cache和阶段计时、默认Limits；实际codegen均为 **2 units / 2 jobs**。

本地记录宿主为 **AMD EPYC 9V74 80-Core Processor**，`Linux-6.18.55`；允许affinity为0–8，选定 **[0,1]**，`cpu.max=800000 100000`，`memory.max=8589934592`。它与正式CI的EPYC7763宿主不同。固定两CPU只约束本窗口，不支持跨宿主时间比较或物理独占声明。

## 每 case 点估计

各case每区块先平均同臂两次成本，再分别对A/B的三个区块均值取中位数，最后求B/A。不是对区块B/A求中位数。三个case分别报告，无总分。

| case | A wall ms | B wall ms | wall B/A | B wall变化 | CPU B/A | 最大单进程RSS B/A |
|---|---:|---:|---:|---:|---:|---:|
| unchanged-wide | 161.795 | 176.450 | 1.090573 | +9.057% | 1.080759 | 1.006365 |
| multiple-rewrites | 191.878 | 170.538 | 0.888787 | -11.121% | 0.889821 | 1.001115 |
| repeated-hints | 101.732 | 112.008 | 1.101003 | +10.100% | 1.066873 | 1.001680 |

RSS来自wait4的最大单进程记录，不是同时全进程树峰值；不能据此宣称内存目标已达成。

## 所有区块的方向

区块1/2/3分别对应原始block 0/1/2。比值大于1表示B耗时更高。

| case | 区块1 wall B/A | 区块2 wall B/A | 区块3 wall B/A | 方向 |
|---|---:|---:|---:|---|
| unchanged-wide | 1.302305 | 0.911278 | 1.090573 | B较慢 / B较快 / B较慢 |
| multiple-rewrites | 1.075629 | 0.889889 | 0.808095 | B较慢 / B较快 / B较快 |
| repeated-hints | 0.958019 | 1.055097 | 1.219350 | B较快 / B较慢 / B较慢 |

三个case的区块方向全部混合，只有三个独立区块。unchanged-wide和repeated-hints点估计较高，multiple-rewrites点估计较低；这些变化不能扩展为普遍回退或普遍提速。repeated-hints有静态重复提示机会，但没有测量memo命中计数，也没有拆分CP18/CP19的独立A/B，因此**不能宣称CT memo取得了实测收益**。

**风险结论：** 该辅助窗口暴露了大型复合类型路径需要继续核查的成本信号。主CI的约1.58%编译改进只适用于其冻结语料，不能消除这个覆盖边界；两者不能抵扣或合成更有利总分。本审查保留全部36条正式样本，不通过事后选择轮次、case或估计器改变结论。

## 保留产物的实际字节

42份ELF和42份LLVM IR均从保留原件重新读取，逐份SHA与各自记录一致。每case的14份ELF（2预检+12正式）SHA/size完全一致。原始`.ll`每case有14个不同SHA；逐份比对证明差异只在第二行精确格式的`; verified-fingerprint: <64 hex>`元数据注释，其余字节完全一致。没有改写或替换任何IR文件，也不把不同原始IR说成原始SHA一致。

首次独审程序额外要求原始IR跨样本相同，因上述注释差异停止；该过严断言的输出保留为`COMPILE5-INDEPENDENT-AUX-attempt01-raw-ir-identity.json`。修正的是审查表述和比较维度，不是测量样本或成绩。

## 绑定与复算

- Plan SHA-256：`5aff2f5253cc5e4ef1a36f7eed7e600c83f083942d759448ac402468ec4f6e36`。
- Meta SHA-256：`ac5bd4802a9ecf20218c7012359a612346338a65cb5669c1ea1ecad8b3ada5eb`。
- 原结果SHA-256：`abf6f70e4b7a6550e9fb6199c4d9c3e54d730e8303e9c9d4a85ae6d86fecbf41`。
- 独审程序SHA-256：`f0c3d952cd26263e18ffe6b23fef3849485806a82614db7ff2341b2dc7d87b99`。

```bash
python3 recovery/reviews/COMPILE5-INDEPENDENT-AUX.py \
  --root recovery/probes/run-v2 --repo zil \
  --out recovery/reviews/COMPILE5-INDEPENDENT-AUX.json
```
