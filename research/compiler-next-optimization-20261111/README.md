# 编译优化研究归档与恢复（2026-11-11）

**本轮所有候选均未采用，生产 main 保持 `99035931d358add0eaed6d4535035c38413701dd`。** 本目录保存源码、协议、成功与失败证据；归档、功能正确或运行质量通过，都不等于编译优化获得生产准入。旧 multi-edit、a60/1679及其否定结果不被本轮覆盖。

## 实际结果

| 对照 | 已完成规模 | 结论 |
|---|---:|---|
| S：结构化直接输出 | 84请求、42对、252短I/O | 完整请求wall +7.633%，同ELF/materialIR，编译NoGo |
| B：builder插入位置复用 | 84请求、42对、252短I/O | 完整请求wall +2.078%，同码，编译NoGo |
| 默认ThinLTO | 6编译、54native | 两格runtime B/A约1.736/2.008，质量NoGo |
| 严格完整可见性 | 6编译、54native | 实际Required导入56/224仍runtime约1.618/2.056，质量NoGo |
| 私有Internal导入 | 6编译、54native | 两格运行质量Pass；完整研究请求慢，编译NoGo |
| 请求内身份核验去重 | 36编译、108native | 运行Pass，编译NoGo |
| 引用计数改为一次线性扫描 | 36编译、108native | 运行Pass，编译NoGo |
| 进程内手工prelink复用 | 36编译、108native | 运行Pass；两格编译wall约+22.468%/+9.221%，编译NoGo |
| 完整O2 producer、空Required直接发射 | 36编译、108native | N64编译wall +24.194%；N256 wall −9.483%、CPU −13.246%，但runtime分别+34.495%/+46.228%，两格质量NoGo |

短I/O不是runtime性能门；上述后端质量对照仅覆盖N64/N256纯base。10%相对退化门与Rust/B≥.90均未放宽，单格编译改善不能抵消质量失败。没有新增many-edit、暖缓存、并发或真实用户频率分布的收益证据，也未扩大到原八case、函数地址、可变globals等范围。保留全部样本与长尾，不跨session相减，不把阶段累计wall当关键路径可直接扣减的成本。

## 源码与二进制身份

| 角色 | 精确源码提交 |
|---|---|
| 手工prelink二进制producer | `2321e74897975978925dc7d2a611f7d672f716e1` |
| 对应冻结harness | `850289f5a732e58cf03ee79f8a2c8b1b3ae3a489` |
| 完整O2复用二进制producer | `2458defcaf8be51c87b80c0dadf391bb585735e8` |
| 对应冻结harness | `8072f23187a77f2e63686696111a8e0e71e8be7b` |

harness子提交只保存研究脚本，不能替换binary buildbinding中的producer身份；author与remote同tree也不等于同commit。完整commit/tree/binary/工具SHA以manifest、正式protocol及buildbinding为准。新C API路径是手工模块裁剪、链接与O2复用，未冒称官方ThinLTO index/importer。语言Verify、完整原模型/分区/实际unit预检、原owner/复制预算、LLVM验证、实际Required与逐Use检查均保留；没有开启native/object缓存。

## 布局与恢复

- [snapshot/research/](snapshot/research/)：冻结脚本、源码/协议独审、结果MD、控制与恢复说明；identity、linear、manual及full各自保留版本，旧失败不覆写。
- [snapshot/out/next-optimization/](snapshot/out/next-optimization/)：正式protocol、summary、完成状态、实际构建/二进制绑定及小型过程收据。
- [manifest.json](manifest.json) 与 [SHA256SUMS](SHA256SUMS)：文本payload的路径、字节数和SHA、source关系、范围及原始证据指针；SUMS不自哈希。替换前旧元数据在 `update-history/` 保留。

先在本目录核 `sha256sum -c SHA256SUMS`，再按相对结构恢复snapshot；源码按精确Git提交创建独立worktree。工具按protocol的版本、调用角色及SHA恢复，ZIL/Clang LLVM23.1.4与Rust1.99.1 bundledLLVM23.1.1分别绑定。绝对工作区路径是原始provenance，移址必须另记映射，不能伪造旧attempt或重用已消费运行目录。

完整raw、BC/ELF、decoded LLVM、生成语料、二进制及本轮新增大型 `result-review.json` 独立打包；文本结果MD指向对应raw，不能仅恢复文本便声称已复现。新版实际绑定：

[raw-checkpoint-v1.json](raw-checkpoint-v1.json) 记录实际已保存版本 **1**；包大小 **218069679B**，条目 **33851**，SHA256 `b815cbc9d9741ebaad7a4640bd54a743c460217310b58478422ac1408f1c0f86`。保存文件身份 `libfile_fde3_f-182729328f508191aedc5d2668080d96_bd147c3485448191a9df16b3c3cea7dd`；[完整证据包](https://chatgpt.com/library?file_id=libfile_fde3_f-182729328f508191aedc5d2668080d96_bd147c3485448191a9df16b3c3cea7dd&account_id=personal)。所有源文件打包前后相同，tar逐条读回通过；与v0逐路径比较，11916条旧路径无缺失、内容/模式无改变。

旧v0检查点仍由 [raw-checkpoint-v0.json](raw-checkpoint-v0.json) 固定，只覆盖形成时的证据，不冒称包含后续identity/linear/native实验。

## 退役与历史边界

[退役执行记录](snapshot/research/retirement-next-optimization-executed/)共105路径：104个payload加freeze索引，含四次成功Actions、精确lease、归档先行、实际日志收据和ref404读回。S/B/default/strict旧分支已退役，源码与recipe由归档祖先保留；旧composition-validation历史研究也已归档且ref缺席，不存在待整体合main的生产候选。

旧Thin archive-v1/v2/v3仍有未找到的历史字节，只保留已知hash与实际可用文件，继续明确metadata-only缺口；不因最终收束宣称全部恢复。正式v4及严格v1/v2的完整恢复事实单列。未由本轮解决的旧回归恢复缺口仍见manifest，不以归档或NoGo结论抹除。
