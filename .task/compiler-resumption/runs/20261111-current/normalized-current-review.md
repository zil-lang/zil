# Normalized 当前候选独立裁决

## 结论：NoGo，继续研究，不采用当前生产候选

候选 `130d3a7cab25603ae42a7fae0c9f63116c858c72`（本地 `4cc1226bd7e652201cfb37d992f4bb13988db42a`，tree `4f60d438f30724ed979f62fe3d08904faee07b8b`）相对资源基线 `f38ed14b2c18c587f01a117f3c8a6d9cc67c4e7b`，出现 **primary 3 格新 >10% 运行回退，其中 1 格新 Rust90 穿越；另 holdout 1 格 >10%**。这些失败的完整 ELF 不同，primary 三格 .text 也不同，不能以机器等价解释或用编译加权收益、multi/State 成功抵消。PR84 当前交付应保持未接受。

原始证据完整，独立原始审计 complete=true、errors=0；这里 complete 只表示数据完整，不表示数值门通过。所有原 false、新回退及原始样本保留，未追加计时、删样本或修改门槛。编译收益表明仍有研究价值，下一变体必须使用新身份并保留本次反例；不能用“只算有利多模块场景”重新解释当前候选的原范围。

## 真实身份、管线及代码变化

- 比较的是资源基线的显式 after 与候选的显式 before-and-after，不是两项历史百分比相加。实际 CLI：base SHA256 `aafc969f277684cfc1e678dd6064bcdc13ed46fa80db04a2a213cda1dbfadeff`；candidate `9f5637ea40ac9b019ff89c0b9a7702f78166f8383684287bbd6bcb1f1f3270d5`。当前文件、外层 receipt、primary/multi/holdout manifest 相符；旧硬编码 baseline 标签未改，实际源身份由外收据纠正。
- 已逐条核对 primary 1728、multi 864、quality 20、neighbors 20 个 ZIL 当前收据：Success、Verify=1，实际 CLI 正确；所有 O2 base 对象是完整 late-after 管线，candidate 是完整规范化前缀及末尾 after。O0 仍为原 default<O0>；不能只看 CLI 选项名称。
- 候选实际前缀：SROA/early-CSE → IPSCCP/globalopt → mem2reg/有限 InstCombine/SimplifyCFG → CGSCC inline → mergefunc → 完整 default<O2>，再末尾 merge，aliases=false。缓存命中按显式 producer-verified 与本次 current preflight 核对，没有把缓存对象记为本次重新跑 LLVM 验证。
- 原八项 288 对 O0 代码段全部相同，288 对 O2 代码段全部不同；保存的 primary 16 对完整 ELF、multi 8 对、quality 10 对、neighbors 10 对均不同。具体身份和全部 primary 格的原样本见 `normalized-current-code-identity.json`。

## 已确认的运行阻断

| lane/格 | base 中位 ms | candidate 中位 ms | 新回退 | Rust90 |
|---|---:|---:|---:|---|
| primary/owned8/base | 16.133628 | 20.014741 | +24.056046% | 新穿越；Rust 17.082201 ms |
| primary/shared4096/base | 259.321841 | 294.132174 | +13.423602% | 原该格无 Rust 臂 |
| primary/shared4096/private | 265.862747 | 299.709758 | +12.731009% | 原该格无 Rust 臂 |
| holdout/dispatch16/base/0 | 1.500405 | 1.754607 | +16.942226% | 此 screen 无 Rust 臂 |

owned8/base 的 Rust/base≈1.0588、Rust/candidate≈0.8535，原 90% 门确为新增穿越。三个 primary 失败格每臂都是 5 次正式 + 1 次预热，输入/输出/退出/无超时均逐次核对；没有“漏格补 Pass”。其正式样本按原顺序如下：

| primary 格 | base 全部正式 ms | candidate 全部正式 ms |
|---|---|---|
| owned8/base | 14.858586, 19.139441, 14.729005, 16.133628, 16.228587 | 24.005138, 20.014741, 13.985238, 17.794632, 23.244897 |
| shared4096/base | 256.429497, 272.975613, 259.321841, 253.817288, 262.203518 | 297.088595, 294.132174, 368.703795, 288.434841, 279.940236 |
| shared4096/private | 265.862747, 276.147912, 273.909008, 251.591248, 257.167746 | 322.752834, 286.797395, 299.709758, 280.946618, 329.832618 |

shared4096 两个快照中，5 个候选正式值全部高于各自 5 个基线值。owned8 有波动，但当前不同代码的完整原门失败不能擅自归为噪声。未对这些样本复测或截取子集。holdout 的低时长格也保留原门失败，不因它只有约 1.5–1.8 ms 就豁免。

## 其他 lane 的结果与边界

| lane | 完整计数 | 运行门结果 |
|---|---|---|
| primary | 1800 构建 / 1500 正式 / 3672 短输出 / 216 runtime | 新 >10% 3 格，新 Rust90 1 格，false |
| multi | 1008 构建 / 840 正式 / 12960 短输出 / 384 runtime | 固定 16 格通过，最坏约 +2.9943% |
| quality | 30 构建 / 900 runtime | 无新增回退/穿越；5 个既存 Rust90 失败，false |
| neighbors | 30 构建 / 480 runtime | 无新增回退/穿越；10 个既存 Rust90 失败，false |
| controls | 48 构建 / 864 观察 / 432 对 | 完整通过；Clang 参考仍用 after，不冒充 before |
| holdout | 64 构建 / 48 正式 / 1088 短输出 / 256 runtime（224 正式） | 16 格中 1 格 >10%，无 Rust 臂，false |
| State/Store | 16 构建 / 128 观察 / 56 次实际分配失败 | 真正 before 消费者通过，不能替代性能门 |

holdout 的 104 个源文件、64 次构建原始 JSON、实际策略、全部短输入及独立业务模型输出已核；100 个新对象确有 LLVM 前后验证，36 个缓存对象有 producer 验证及本次当前 preflight，64 次当前 Verify=1。长循环未重跑昂贵 Python oracle，独立核对原 producer 校验标志、256 次原 stdin/stdout/stderr、退出状态和跨臂输出一致；此范围在审计 JSON 明示。unequal16 明确不等的 handler 保留且通过短输出控制，不能删掉失败的 dispatch 格后称 screen 全通过。

State 的 8 个 O2 构建实际使用上述完整 before 前缀，ordinary/probe 原始 IR 相同，probe 只链接真实 API 对象；56 次注入的实际 failures=56，live=0，allocated=frees，完整输出及退出码吻合既有 State/Store 业务 oracle。该证据是实物消费者的失败清理，不是 CLI 选项单测。

## 研究价值与取舍

| 固定分布 | base wall ms/请求 | candidate wall ms/请求 | 等待减少 |
|---|---:|---:|---:|
| primary/interactive/edit | 310.923298 | 290.884304 | +6.4450% |
| primary/interactive/clean | 441.805975 | 432.988450 | +1.9958% |
| primary/release/edit | 557.898536 | 491.514625 | +11.8989% |
| primary/release/clean | 832.873856 | 793.181779 | +4.7657% |
| multi/O2-only/edit | 167.656644 | 153.536079 | +8.4223% |
| multi/O2-only/clean | 247.088504 | 183.977568 | +25.5418% |

这些是当前资源 after 基线与 normalized before 候选的真实配对结果，不能与资源相对 main 的收益相加。原八项最高 11.90% 的加权等待减少、独立 multi/clean 25.54% 的减少说明继续研究有价值，但当前实现用可观的生成程序退化换来了部分编译收益，未满足原质量约束。

冷/编辑代价也不均匀：ring768 的 O2 cold 1467.45→1955.33 ms（+33.2472%），private 821.83→1092.45 ms（+32.9299%）；这是编译延迟，ring 运行反而分别改善约 4.23%/8.68%。multi 的 pipeline16/private 从 146.65→250.88 ms（+71.07%），holdout 独立出现145.78→241.65 ms（+65.76%）；恢复完整优化可见性丢掉部分 after helper 分区的私有编辑收益，不能宣称 before 支配 after。

下一步应保留当前 frozen 输入、源码身份、首败和全部数值，在独立新候选中查清 early normalization/inline/merge 对 owned8 与 shared4096 机器代码的影响，再验证通用的最小修正。不要按样例名、既见规模或运行输入开例外，也不要把这三格从原八项中移走。当前候选保持 NoGo；既定资源 after 的独立有限采用意见不因该 before 候选失败而被改写。

产物：`normalized-current-audit.json`、`normalized-current-code-identity.json`、`normalized-holdout-current-audit.json`、`normalized-state-current-audit.json`。本审计仅静态读取、哈希、解析及重算，没有编译或计时。
