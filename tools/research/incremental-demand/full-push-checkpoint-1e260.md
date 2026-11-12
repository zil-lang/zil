# 全流程施工检查点：1e260

本次冻结源码 `1e260a4ed8898e1ce92ae6f0b7931b47d4dceb7d`，main `3ec7e8a960ca69368cd9a90f1a15ac063fec5a02`，二进制 SHA256 `e2411c9d010cc7c8a8d4a21d40563384118568faba094b795d2cdad997ddb393`。完整六轮对照见 [正式结果](full-push-matched-1e260.md)；这里说明实现、残余成本和后续施工，不能据局部通过关闭父任务。

## 当前逻辑与 push 边界

```text
安装本轮输入变化
→ 为当前实际消费者建立输入/依赖绑定
→ 重算受影响函数；值相等则保留产品身份
→ 独立 Verify 核对当前 raw、Candidate、目标、全部 Limits 和自己的成功证明
→ 仅替换改变的机器函数及所属单元
→ 对象相等则停止；完整当前链接输入相等则恢复最终可执行文件
```

核心方向与 Zit 的变化逐层 push、值不变停止一致。实际各阶段消费者已经贯通到 exe；中央 ReactiveGraph 仍不是整条流水线的唯一调度器，外层整项目进入、若干全局扫描及保守回退仍在，不能叫全局最简 push。ProducerMemo 路径不运行 signal 的逐读 pull；删掉另一条 graph 的读取机制不会消除本次主测成本。必要的当前依赖发现、独立 Verify 授权也不能以“只关心终态”为由省略。

Rust query 是从需求向下验证/取得依赖，再用 red-green 和结果指纹停止无效传播的 pull；这里的 push 从变化点传播。两者都应避免无关重算，差别不等于性能结论。Rust 仍有全 crate 收集、分区、序列化及受影响 CGU 整体重编；本项目也仍有这些成本。固定 Rust/Cargo 源码引用沿用 [源码审计](pipeline-audit.md#official-rustcargo-fixed-source-comparison)，不是用概念代替真实实现。

## 本轮落地

- Types 错误恢复：producer 在错误请求后前进、Types 成功 epoch 落后，旧逻辑退回完整函数构建。现用完整私有输入上下文、当前函数 owner/token、位置绑定和全部 Limits 桥接成功历史；真实长度变化恢复为 1 body + 20 position + 404 retained。不同环境和未知依赖仍完整回退。
- 原生快照：机器指令改成连续的 instruction/operand/MMO/tie 池，完整字段及当前 frame/CFI/PSV 绑定保留。指令头 88B→40B 是表示变化，不等于整请求速度收益。
- LLVM pass 依赖：取当前真实 scheduler 缓存的 per-instance AnalysisUsage，完整区分 Required、Transitive、Used、Preserved 四列；不靠重新调用虚函数猜测当前调度契约。补的是完整绑定，未声称此前存在已观察 wrong-code。
- FPM 缓存按完整组的实际 fresh/key/replay/capture 成本决定是否保留，执行原 O2 pipeline，失败不发布学习；不靠减少优化制造速度。

## 从最终产物倒查

以下为本次正式 O2 六轮的范围中位数，嵌套子计时不能相加。受影响单元仍按完整 LLVM 质量合同优化和发射。

| 位置 | 冷候选 | 私有编辑候选 | 已确认仍在的成本 |
|---|---:|---:|---|
| object scope total | 5471 ms | 295 ms | 包含下列嵌套步骤，非额外总和 |
| 机器码发射 | 3230 ms | 82 ms | 快照捕获、重建、机器验证及原 AsmPrinter/MC |
| LLVM 优化 | 1643 ms | 159 ms | 仍有完整 affected-CGU/module passes 与未复用 FPM |
| MIR 快照 key / capture | 136 / 452 ms | 13 / 1 ms | typed MIR 和完整 pass 元数据捕获有真实自身成本 |
| IR key / capture | 73 / 110 ms | 6 / 3 ms | 完整身份/依赖比较及有限域快照 |

无修改 block0 的 native total49.2ms，其中链接输入发现33.7ms、诊断输出5.2ms、machine4.9ms；16对象均命中并复用当前预检，不能把这49ms误叫LLVM重编。当前链接工具身份只做14次元数据复核、没有再次全文件哈希；把旧282MB工具哈希当当前主因是错误的。以上 block0 为单行范围诊断，不代替无修改六轮63.766ms整请求中位数。

下一项后端完整切片：消除 pipeline/observed sequence 的完整 AU 向量重复保存和临时深复制；保留当前四列逐值核对，执行序列仅持有经核对的 ordinal/标志。另外，链接发现将两工具的 ldd 探测合成一个进程，保留完整分节解析、依赖库/目录/环境及前后绑定，未知格式走原路径；实际 noop 早已复用完整诊断分区，其5.2ms主要是完整sidecar写出，不能冒领新聚合省工。另有 plain CT 环境的实际消费投影已完成独立源码：停止让无关 raw NodeId/exports 重编号清空所有 plain body hints；完整 checkpoint 和独立 Verify 授权仍严格绑定。以上后续项尚未进入本次测量源码，不回填本表。

## 验证和证据边界

精确1e260通过 native72、backend239、mixed driver3、严格全目标Clippy、fmt和release11；Types101/driver208在前置Types修复上通过，生产Rust代码随后未变，不将重叠测试相加，不声称当前完整workspace新跑。独审见 [flat MIR/AU](full-push-flat-mir-canonical-au-review.json)。Types首次100/1 fixture失败、fmt09失败及修复记录保留。

check/O2 各270请求（144测量、126具名父历史）；O2 2808原生执行，78成功质量对（42测量、36父准备），全部质量对照由原始 validator 核对，见 [非作者结果独审](full-push-formal-1e260-results-review.json)。严格汇总重新核对完整源/二进制/工具/构建身份及全部LLVM比较；check使用 [逐文件验证的20文件投影](full-push-check02-summary-projection.json)，投影不冒充全量raw恢复。检查与原生分开；CGU16/O2/jobs1相同，不代表实际分区内容相同。Rust1.99.1/LLVM23.1.1，新Cargo进程；ZIL resident/LLVM23.1.4/独立Verify，预算及优化等级未降。

check 全量raw已在 [v14固定远端](https://github.com/zil-lang/zil/tree/8d10022e4c0daf0717a07bdfeb2905372372ca4a/research/incremental-demand-full-push-20261112-v14) 全新HTTPS恢复、逐文件和模式核验，并用捕获的原始validator重算通过，见 [恢复收据](full-push-recovery-v14.json)。本地check目录仅在完整manifest匹配后退役，精简元数据保留；不能假定原路径仍存在。O2全量raw当前正在固定archive/source-history分片归档，尚不宣称远端恢复完成。旧环境最初丢失raw仍无法恢复。

本次检查修改延迟已有明显进展；O2 private候选560ms/Rust593ms，尚无显著领先，cold、broad、noop仍负，recovery基本持平。继续施工，开关默认关闭，不合main。
