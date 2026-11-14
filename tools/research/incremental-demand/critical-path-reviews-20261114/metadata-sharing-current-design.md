# 当前 metadata sharing：实现与证据边界

本批从 research 交付树 `4b3a99b873b433501fb9c24184b1348023316b11` 接续；旧测量编译源码为 `1de00f63f80cf4534ec995dc51e106005b270ddb`。生产写集仅 `crates/zil-driver/src/packed_items.rs`、`semantic_items.rs` 的当前 cancellation 传递；测试写集为 `packed_items_tests.rs`。源码已交 root 冻结，本说明不声明 root 后继提交或验收结果。

**真实存储定位。** 输入为 `/workspace/zil-push-main/out/runs/large-project-large1000-edits-02-check/user/block-00/candidate/semantic/`。其中 `semantic-v7-push` 的 36 份 `ZILPKM01` manifest 共 **9,619,436 逻辑字节 / 9,732,096 物理分配字节**，是最大重复项；`packs-v1` 的 36 个 body pack 为 4,439,235 / 4,489,216 字节，discovery 为 39,248 / 40,960 字节。一个实际 envelope 为 267,061 字节，含 2,015 条 item 定位行及 105,789 字节逻辑 manifest。body 已有 packed CAS，push 下旧 FunctionStore 已禁用；本批消除完整 locator/binding 表随项目版本重复落盘的问题。此目录包含完整历史，不能直接把总量等同于 cold→private-032 增长。

**仅旧字节投影，非 formal disk。** 对上述 36 份旧 envelope 用当前确定性分块规则只读核算，得到 453 个唯一块，其文件逻辑字节合计 **2,415,028**（每块包含原 ArtifactCache 的 80 字节外层头），36 个版本索引合计 **85,792**（同样包含外层头）；总计 **2,500,820 字节**，较旧 manifest 少 7,118,616 字节，约 74%。这没有运行新编译器，没有生成新用户缓存，不是物理峰值、32-edit 增长或耗时测量，也不代表整个 semantic cache 减少 74%。正式磁盘仍须逐实际 snapshot 全局 inode 去重后合计，不能相加各类别独立最大值。

**完整持久化链。** 原 envelope 小于 16,384 字节时继续 inline；较大者用 `ZILPKS01` 索引保存原总长度、完整 digest、按序块 key/length。内容边界由字节决定，非末块 2,048–8,192 字节，末块可更短，插入后能重新对齐。块以完整内容 Fingerprint 命名，存在时仍核实际长度、raw wrapper、内容 key 及精确字节；缺失或损坏则写入并回读核对。恢复按索引重组原 envelope，逐块核验后再核完整 digest，最后进入原 `Manifest::decode`、body pack/item 校验、语义重绑定及当前独立 Verify。哈希和 locator 仅为 raw 候选完整性，不是旧 receipt、当前授权或 Verified token。旧 `ZILPKM01` 仍可读取；历史索引各自完整，不依赖链式 delta、全局可变索引或其他历史版本。

**Bound、缺失与取消。** 索引大小、重组大小均受原 `Bounds::manifest_maximum()` 约束；checked 长度/计数计算、行数和、块大小、非末块下限在分配重组缓冲和读取块前检查，不允许递归块引用。重组后原 logical/item/pack 数量、偏移和唯一 body pack 聚合读量上限全部保留；没有新 body 预算或默认限额调整。块缺失、截断、错误 digest/key/顺序、symlink 或超限返回 miss，沿原候选缺失路径重建。当前 cancellation 检查覆盖分块准备、逐块读写、恢复和最终发布，也传入 epoch hint。写入使用原 pinned `CacheNamespace`，每次新块写前检查 namespace，提交前完整 validate；先同步 body/metadata 块，最后原子发布版本索引，成功后才推进 epoch。取消或提交失败保留旧索引和 epoch，已写但未引用的块可能留下，重试可复用；没有删缓存、GC 或自动清理历史。`physical_pack_*` 仍统计 body pack，`manifest_bytes` 包含新写共享记录的完整文件字节及索引 payload，不含索引自身外层 80 字节，不能代替 filesystem disk 计量。

**测试范围与未验收项。** 已新增 5 条控制：①32-edit 后全部历史逐字节读取、逐次重启恢复、hint/warm 和旧 inline 再发布；②缺块、损坏 raw checksum、合法 wrapper 下内容替换、symlink 的拒绝与重发修复；③索引长度/计数/块长、完整 digest、顺序、截断、logical/item 的 N−1 限额和已取消读取拒绝；④取消最终发布时旧索引/epoch/历史不变，retry 不重写已存在块；⑤目录替换不能提交新索引或 epoch，恢复目录后可重试。原小 envelope、body 配额/压实、损坏及发布控制保持。本 owner 未构建或执行 Rust 测试，未操作 Git；这些是已写断言，不是通过收据。root 统一验收当前 Source→Verify→持久化→restart/restore 和默认预算。新增文件数、文件系统分配浪费、分块/恢复 I/O 与哈希耗时，以及长期增长均待新 formal 窗口；旧缓存不批量改写，全部变化时共享收益也不保证。
