# Shared manifest metadata 精确独审

结论：**ACCEPT，限定本次 driver metadata 差异，未发现阻断缺陷。** 新层是可还原原 envelope 的有界存储运输；没有把 content key、SHA、缓存命中或 pack hint 变成 Verify 权限。缺失、损坏及取消继续保留原 source 路径与发布边界。存储收益仍需 root 的真实项目测量；本报告不接受整个研究提交或 LLVM 新机制。

**最终状态：FINAL；具体阻断项：0。** 下述未测组合是验收覆盖边界，不是已发现的生产缺陷，也不要求为本独审扩大施工。用户已告知 root 修复 emission asmNOP 回归并恢复统一门；该 LLVM 修复不在本次 metadata 独审范围，本报告不据此重标 c4 身份或宣称统一门通过。

本复核者不是这项实现作者。本轮只读源码、提交和现有测试文本，仅创建本报告；没有 build、compiler、测试或 bench 执行，没有 Git mutation，没有生产代码修改。使用的 Git 操作仅为禁用 optional locks 的 show/diff/rev-parse。已读仓库 AGENTS 及恢复的 goal/workflow；不将恢复看板当作验收收据。

## 精确源码

候选 `c4f869af46efdb3aef5203bf8b69c5d1d084b614`，tree `955b4523e8ec0f985d2349e8f154a7dded7708a5`。以下三个工作文件逐字节等于该提交 blob；本次只审它们相对父提交的 metadata 差异，并只读追踪原消费者和文件系统 guard。

| 文件 | blob | SHA-256 |
| --- | --- | --- |
| `crates/zil-driver/src/packed_items.rs` | `b299ffd4f0cd6f30e0ac615567042eb14d699a56` | `6a03097cb37a3b268eae78bd1f4bdee0abc54b65857fd949d10afa697e8c96d1` |
| `crates/zil-driver/src/packed_items_tests.rs` | `e899a77a8bc98daa1cf337dd37f6390c0c0d120a` | `6397eec8cf718082f44fd2757e7a0a39451bb28de500f2adeaa60a2b9c94840c` |
| `crates/zil-driver/src/semantic_items.rs` | `fa3341b15280c5fde843fac849b5198a82c2e16c` | `66191e2e7b91e7a45206da13fe68682514490061d7dc00bea77beb0a44f66548` |

三文件提交差异为 465 additions / 8 deletions，其中新运输层主要在 packed_items，semantic_items 仅将 cancellation 传入恢复 hint；没有改默认 limits、语义解码器或 Verify 实现。

## 无损恢复与原检查

`packed_items.rs:40` 的 gear splitter 是确定性内容边界，通常每片 2048–8192 B，末片允许更短；大于等于 16384 B 的原 `ZILPKM01` envelope 才选择共享。descriptor `ZILPKS01` 保存原 envelope 总长、完整 digest，以及按顺序的 chunk key/length。它不包含可跳过解码的语义证明。

`read_manifest:319` 只重建一层，最后明确调用 `Manifest::decode`；内层必须是原 `ZILPKM01`，不能把另一个 shared descriptor 当作递归 manifest。重复 chunk key 可合法表示重复的原字节，仍逐位置计长，不会因去重而放大允许的逻辑长度。错顺序只有在还原字节完全相同时才无区别，否则完整 digest 拒绝。

重建后原检查全部保留：logical/selected payload 总长、item/pack count、严格有序且唯一的表项、offset+length、pack 引用完整性、所有 unique pack 的真实 payload 总量，均由原 Manifest decoder 检查。旧 inline envelope 仍走同一个 decoder。

后续 `Reader::load:504` 仍检完整 pack、所选 item 的实际 content key，并按 pack 缓存已读 bytes。`semantic_items.rs:547–579` 仍用 logical manifest 经 `layout`、`assemble`、`decode_bound`：logical/table、总字节和函数数限制先行，逐 segment 核长度/hash，原 raw decoder 使用当前 limits，完整 current binding 重绑定后才返回 TypedProgram/CandidatePlan。shared descriptor 的较小物理大小不能让更大的 logical/body 绕过原 quota。

## Bounds、总 I/O 与分配

令 S=`Bounds.maximum`，I=`Bounds.max_items`，原运输上界 M=`S + 32 + 120×I`，均 checked 算术。semantic consumer 的 I 仍为 `2×max_functions+6`；function-product consumer 的 I 仍用自己的原上界。当前默认 S=16 MiB、max_functions=10000；没有由 chunk 数重新申请 source/body allowance。

在分配重建 buffer 或打开 chunk 前，`read_manifest:325–355` 已检查：

- descriptor 自身 ≤ M；重建 total 位于共享阈值与 M 之间；
- count 非零，≤ ceil(total/2048)，且 `52 + 40×count` 精确等于 descriptor 长度；
- 每片 1–8192 B，非末片至少 2048 B；所有片长 checked 累加且精确等于 total。

因此没有一个很短的 descriptor 可以任意声明无限 I/O、递归树或无界片表；重建 buffer 只按经检查的 total 分配。单 chunk 的 ArtifactCache 读取按它声明的 length 限制，实际读取还有原 raw header 和至多一个越界探测字节；坏文件不会按磁盘实际巨长分配。

**总 I/O 合同需准确区分**：新 metadata chunk payload 的逐次读取总量 ≤ M，另外有每片 80 B raw header 和 descriptor；原 body packs 的 unique payload 总量仍 ≤ S，并保留各 pack header 开销。不能把“body quota 仍为 S”表述为“整个请求所有磁盘字节仍只有 S”。这是有界的新 metadata 运输开销，不是 body 额度放大。`remember_packed_epoch` 会再次恢复 metadata，因而一次 load 后的 hint 可能再读一轮 chunk；每轮独立有界，不能宣传成 metadata 零读取。

read 中保留一个重建 envelope，decode 时创建原 logical/table；完成或 early return 后局部 buffer 和已读 chunk 自动 drop。Reader 只在本次实例保留所选完整 packs；Epoch 只保留 locator/length/stamp，不保留全部 metadata chunk bytes 或新历史索引。writer 的 pieces 是原 envelope 的借用切片，descriptor 另分配，原 envelope 活到发布作用域结束；新增峰值是有界 envelope/descriptor/片表和单片检查 buffer，不是每个引用再保留一份大 manifest。原 logical/body/decoder 数据本身的内存开销没有被这项优化消除。

## 缺失、损坏、soft miss 与 hint

`read_chunk:306` 拒绝静态 symlink/非普通文件，使用原 ArtifactCache header、key、length、payload checksum 检查，再检查 payload 自身 hash 等于 content key。即使攻击者重算外层 checksum、保留旧文件名，错误内容也不能被当成该 chunk。重建后还有整个 envelope digest 与原语义结构检查。

缺片、错片、截断、超长、错 digest、错顺序和任一原 decoder 失败均返回 None。`Store::load` 是 Option candidate 入口，不把这些情况提升为语言接受或必然的用户编译错误；driver 继续当前 source 路径。取消也返回 miss/停止候选恢复，随后仍受原 driver cancellation 检查，不以缓存 miss 绕过取消。

`Epoch::hint:102` 现在同样检查真实目录并通过完整 shared→original 解码；恢复的 pack stamps 仍全部为 None。不能从一个有效 metadata SHA 获得可信 body-pack stamp。下一次 publication 的原 `inspect_pack`/selected-item 检查仍会实际读 pack；之后进程内精确 FileStamp 快路径及 namespace 匹配合同不变。旧 hint、失败 hint 或另一 namespace 的 hint 都不能直接授予当前 semantic/Verify 能力。

内容块与 body packs 共用 packs-v1 的 content-key 文件名，但都以完整原字节取 key：相同 key 若对应相同字节可安全共用，含义不同则仍须满足各自 MAGIC/长度/结构检查。没有 key0、零 fingerprint 或“第一个 manifest”跳过检查的分支；fixture 的 id=0 是正常请求 key，32 编辑后仍逐个恢复 0..32，不依赖活跃 epoch。root request key 的 compiler/request 绑定规则没有修改。

## 发布、取消、drop 与历史

writer 先构造并由原 decoder 自检完整 Manifest，再共享 metadata；缺片经 namespace guard 写入、sync 并重新逐字节检查。`packed_items.rs:793–813` 在所有 chunk 后才执行原 before_manifest/取消检查/完整 namespace validate，最后原 ArtifactCache 原子 rename root。root 发布成功后才替换 Epoch；`semantic_items.rs:633` 之后才安装对应 raw binding epoch。

取消或文件系统错误发生在 root rename 前，旧 root/旧 Epoch 不先替换。已经完成的内容块/body pack 可以留在磁盘作为未引用 raw 文件；这有利于 retry，也避免误删历史 root 所引用的共享块。它不等于“取消绝无磁盘写入”。未完成的 `.tmp` 仍由 ArtifactCache `Temporary::drop` 清理。rename 成功是原合作式发布线性化点；不承诺抢占已在运行的 write/fsync 或在 rename 之后自动回滚所有缓存文件。

没有新 GC 或 drop 删除历史共享块的路径。32-edit 测试明确丢弃 hints 后仍恢复每个历史 root；共享不靠延长某个 current Epoch 的生命周期，也未新增无限进程内历史。磁盘 cache 跨不同请求持续积累的清理策略仍是原合同，这项改动不声称全目录存在固定容量上限。

## 权限、symlink、文件别名

沿用原显式 trusted-local semantic cache 合同：新建目录请求 0700，拒绝 leaf directory symlink 和 group/other writable 目录；reader 和 hint 都检查 packs 目录。chunk 文件先拒绝静态 symlink，writer 通过临时普通文件与 rename 替换目录项，修复 symlink 不截断其目标或同 inode 的其他 hard link。

生产 publication 仍经完整 ArtifactPlan 和 CacheNamespace：source、输出与 manifest 的 resolved path/inode 别名在初始/最终 guard 检查；packs directory 与受保护路径的包含关系也受检查。Linux 的 chunk 写入使用 held directory 的 `/proc/self/fd` 路径，每次实际 write 前检查原 path/ancestor stamp，root 前再完整 validate。替换 packs 目录不会把新 chunk 写到替换后的受害目录；原 root/epoch 不会被这一检查失败提前提交。其他平台保留原完整 plan.validate fallback。

精确限制：没有新增每个 candidate 必须 0600、必须同 uid 或读取 O_NOFOLLOW 的合同。candidate 文件 mode 仍由原 OpenOptions/umask 决定；安全假设仍包括显式可信本地 cache、私有目录以及原路径 guard。`stamp`→`File::open` 并不是抵抗同 uid 恶意并发路径替换的原子无跟随 API，不能将静态 symlink 测试推广成这种更强保证。root/候选 SHA 只校验完整性，不认证 writer；current Verify 检查原 raw program/candidate 的语义合同，也不把 hostile cache writer 变成可信源码翻译者。上述边界均未因 shared metadata 放宽。

## 测试内容与验收边界

本轮未运行以下测试，仅核对实际断言：

| 现有测试 | 实际覆盖 |
| --- | --- |
| `shared_manifest_32_edits_keep_all_bytes_and_restart_restore_without_history_index` (`packed_items_tests.rs:467`) | 1024 raw items、变化 logical metadata、32 编辑；每次重启式 Reader、换掉 hints，最后恢复全部历史 root；实际目录增长小于完整 manifest 累计的 1/3；旧 inline 同 key 可读并再次共享。 |
| `shared_manifest_missing_corrupt_and_wrong_content_chunks_are_misses_then_repaired` (`:542`) | 缺片、坏 raw checksum、有效外层 wrapper/错误内容 key、静态 symlink 均 miss；修复后所有原 item 字节恢复，symlink 目标不变。 |
| `shared_manifest_valid_chunks_do_not_authorize_changed_digest_order_or_quotas` (`:597`) | total/count 溢大、零长度、错 digest、乱序、截断、logical/item 额度收紧、预取消拒绝。 |
| `cancelled_shared_publication_keeps_old_root_epoch_and_all_historical_bytes` (`:646`) | chunk/body 已写后、root 前取消；旧 root/epoch 和原内容保持，retry 使用已完成文件。 |
| `shared_namespace_replacement_cannot_publish_a_new_root_or_epoch` (`:671`) | Linux packs 目录在 root 前替换，拒绝新 root/epoch，替换目录无新文件，恢复目录后可重试。 |
| 原 packed quota/compaction/item-integrity 测试及 `semantic_items.rs:916` 等 | 原 pack aggregate read、N/N−1 payload、selected slice、logical/layout/raw decoder 限制仍在原层检查。 |

原 driver `semantic_cache` 集成测试还保留 source/output hard-link 与 symlink 别名、可写 cache 目录 fallback 和当前 Verify；它们是原合同证据，不是新增大 shared descriptor 的组合运行证明。

**未测边界（非阻断）**：新增 fixture 是 raw transport 测试，不是原 1000 声明 check/32-edit Source→current Verify 的运行收据；无精确 16383/16384 共享阈值、chunk 读取中途取消、每个 chunk 写入点故障、shared root 的权限变化/key0 受保护文件别名组合的新直接测试。原路径 guard 与统一消费者在代码上保留，root 的统一 driver/Source 原生门仍须据精确候选结果验收，不能以本独审或 SHA 代替。

统计继续只描述工作：manifest_bytes 增加新写 metadata chunk 的 raw 文件长度，并加 root descriptor payload；root 的外层 header 沿原口径不计入，因此它不是整个目录物理增长的精确替代。manifest_writes 仍是 root 次数，body-pack counters 没有冒充 chunk 写入数；取消留下的可重用文件也不应伪称从未写入。32-edit 测试使用真实目录字节统计，未拿这些 counters 当权限或存储总量证明。

本次唯一写集：`/workspace/zil-recovery/shared-manifest-exact-review.md`。LLVM 机制、全 API/backend/driver 门和 Source 原生运行交由既定 owner，本报告不重复施工或宣称它们已通过。
