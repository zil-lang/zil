# 多文件编辑测量器独立审核

结论：**最终冻结测量器可执行有限 screen，未发现阻断其当前用途的逻辑问题。** 当前仅完成只读源码、协议配置和 SHA256 复核，没有编译、运行测量器或增加采样。此结论不等于生产候选通过性能、运行质量或总体采用门。

## 冻结身份

| 文件 | SHA256 |
|---|---|
| fixture.py | `73b7b2475d178c905fa12546a327e98505301d76a51a96d418a9b10cac60afa3` |
| prepare.py | `3f16987ba78ae4e08d1dadb2f81e171122514dc5a0bdc99bd93f85624eb349c7` |
| screen.py | `0191bc35e4d8571fb714405e93fb4bb26dd9252070f89842b5289bfaf670e9ed` |
| final-inc-off-v1/protocol.json | `d8b51a4b7f404255fb4d3da9390af530d3248eb0fd567da6e01af2961853f018` |
| final-inc-after-v1/protocol.json | `4f4510c59cc397892781dbf6d66665c2e8c6c4d4a818a1ece1c6471c940d4713` |

上述五个 hash 已在最终文件实际计算，并与作者声明一致。执行器还会在运行前验证脚本与既有 CLI measured 实现哈希、冻结 fixture 的逐源文件哈希。两个协议分别固定 incremental-o2 与 incremental-o2-after；旧施工草稿不作为运行依据。

## 核心正确性

fixture 的 dynamic-body 直接消费 getchar 的实际输入，对每个 leaf helper 执行四轮整数乘加模 251，Python model 独立计算 seed 0/65/255 的精确四字节输出。数值范围不会接近 i32 溢出。每个 selected leaf 改首、末两个 helper body；若 selected 包含 main，则改 own_a/own_b 两函数。selected_files 在 k<=N 时产生精确 k 个唯一文件，prepare 还比较实际源文件 hash 差集与 changed_names。N 包含 main，故 k=N 的含义明确。

source_snapshot 先完成全部 pending 写入和 rename，再发一次 compile；循环不在每改一个文件后编译。cold 只编最终快照；warm/resident 先完整 base seed，再批量发布最终变化后编一次。所有 source 写入、原生输出检查、recipe 目录扫描都在请求计时外。

A/B 使用交替 ABBA/BAAB，每个 arm-position、k、state 独立 source/object-cache 目录。warm 的 base 与 edit 在同一路径和缓存上；resident 的同一进程先 ready、再 seed、再 edit，启动与 ready 单列。cold 指新进程与空工具缓存，不应解释为操作系统页缓存已清空。常驻进程使用固定 256 capacity、64 reset-after，本次两请求序列不会主动触发 reset；意外清空或非零 active_queries 会拒绝。

每个 ZIL 请求固定 LLVM API、O2、8 CGU、2 jobs，并根据协议选择 incremental-o2/after。report 必须 Success、verification_runs=1、实际 N 文件、实际 unit receipts；每 unit pipeline、LLVM23.1.4、当前正数 preflight_ns 都检查。object hit 要有 producer LLVM 前后核验，fresh/patch 要有当前 LLVM 前后核验，patch 另检查 current_full_machine_preflight。协议没有把缓存命中当作省略当前 Verify/preflight 的许可。

## 最终新增证据边界

每个冻结 snapshot/policy 先由基线做一次全 O2、无 object-cache 的 clean compile。之后全部 A/B seed/edit 必须满足：完整 ELF SHA256 等于对应 clean oracle，sidecar 仅去掉**恰好一条** `; verified-fingerprint: ` 诊断行后全部字节 SHA256 相等。符号、路径、属性和其他内容没有被广泛规范化。异码即记录失败，原生输出成功不能覆盖它；clean artifact 留在独立目录。该规则较保守，可能因非语义字节变化而停止，但不能把异码质量变化误判成同码。

每请求 layout v1/v2 目录的文件名、大小、哈希集合在计时区外记录 before/after；它提供实际 recipe 证据，不强行假设源 body 改动后 machine cost/结构仍相同。存在同路径 recipe 文件并不独立证明 hit，结果分析还要结合前后集合和实际结构条件，不能仅凭文件存在宣称全部命中。

原草稿没有 ELF/material sidecar 对照、fatal 路径缺少明确停止记录；最终版已补。fatal handler 保存 phase、cell、arm、attempt、已完成 rows/oracles、已取得 metrics、异常类型/消息/traceback，写 fatal-failure.json 并把 execution-manifest 标 Failed/Incomplete 后重抛；剩余请求标 not-executed，不生成成功的部分聚合比率。常规报告/原生/身份错误也先保存 row 再停止。首次失败不被丢掉或改成通过，不追加轮次求正结果。

## 汇总、Rust 与使用限制

加权指标先计算各 cell 的全部观测算术平均绝对 wall ms，再乘 N、k、state 的固定权重，最后计算 B/A；没有平均百分比或删除长尾。结构控制排除在加权主样本之外。协议明确权重是假设请求发生分布，**不是用户实际使用分布，也不是 CPU 频率声明**。单格另有 median-block-mean 比率和 p95，这些描述量不应替代主算术均值加权指标。

Rust 可选 arm 使用固定 rustc1.99.1、O2、overflow checks、panic abort、8 CGU，以及每序列独立正常 incremental 目录；warm 用相同基线 seed/最终源码历史，不能把其比较称为 Cargo 全工程构建。RUSTFLAGS 记录在 manifest，但直接 rustc 不自动消费该环境变量，真正生效参数以完整 command 为准。Rust 无 resident arm，不能据此虚构常驻比较。A/B 与 Rust 都继承相同 CPU affinity；源码到二进制的身份仍需 root 提供构建收据，执行器明确只记录 source claims，未伪装已自行验证 provenance。

原生三向量输出只证明这些功能 oracle，未测程序运行吞吐，不能宣称通过 10% runtime regression 或 Rust90 门。协议明确这些运行质量仍由原有 runtime suite 独立承担，预算 N/N-1/fresh API 门也单列，不降低预算。严格同 ELF 支持本 screen 的同码比较，仍不代替未执行的原协议门、全模式和总体频率矩阵。

最终结论限定为：允许在 root 分配的唯一资源窗口执行上述冻结 screen，并完整保留结果；没有额外源码阻断问题需要作者继续修改。

## v1 首败与 v2 语法修复复核

后续实际执行 v1 时，首个基线 clean oracle 在 ZIL `let seed=unsafe{getchar()}` 处发生 E-PARSE-IDENTIFIER。**v1 没有完成性能测量，measured samples=0；上文的静态源码结论不能写成 v1 已可运行或已执行通过。** 该首败与旧文件由执行者保留，未用修复结果覆盖。

v2 只将 ZIL main 中该表达式改为 `var seed:i32=0;unsafe{seed=getchar();}let result=value(seed);`，Rust 分支与 scalar 分支原样。独立比较两个 policy 的全部 10 个 snapshot：每份只变 main.zil，且逐字等于该单次语法替换；leaf、Rust、动态 Python model、native_vectors、expected oracle、选中文件、规模、权重、顺序和测量时序均未变。protocol 顶层差异只有 created_utc、fixtures 的源码哈希、tools 的 fixture 哈希。prepare.py 与 screen.py 精确保持上表 SHA256，未扩大测量器实现。

v2 已实际计算并核对：

| 文件 | SHA256 |
|---|---|
| fixture.py | `b4d588232df9b0d41d8877adcf99c712a90cb8f931adcf991dce85510ca7f314` |
| final-inc-off-v2/protocol.json | `2012bf6b317d7f67eb282add88b45e05e06ca3e8d7b20409332d339bb1ceb4f8` |
| final-inc-after-v2/protocol.json | `dbab00ff9c04bb8a58eaaf0eeba2109f3aefa0d8c2a79bab3638baa49379dd35` |

本审核未重跑 pilot 或编译，只核查上述精确 delta/hash；执行者报告的 baseline64/256 功能 pilot 与后续正式 screen 应分别引用其原始收据，不与 v1 首败混合。v2 可以继续沿用先前对未改变 prepare/screen 的静态逻辑审核。
