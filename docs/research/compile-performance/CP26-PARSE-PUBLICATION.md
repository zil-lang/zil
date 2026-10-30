# CP26：解析缓存的缓冲发布与临时文件所有权

基线：CP22 完整交付 `7ca57fb61aa6fe4f7cac963a531a6cf2851792e4`。作者 `/root/zit_analysis`；独审 `/root/incremental_review`；由根协调者派定为编译专项的独立后继切片。CP22 的源码、二进制、完整窗口和负结果保留，不能以本片替换或重标旧窗口。

## 问题与合同

CP22 每个未从磁盘命中的模块均编码完整 AST，写入 80 字节 header 与 payload，再调用一次 `sync_all`，最后原子 rename。根的冻结窗口报告 cold 回退；本片仅消除发布 I/O 的不必要同步和小写入，不更改解析键、codec、预算、源码发现、查询图或 Verify。

这没有放宽既有缓存耐久性合同。基线的 `docs/research/compile-performance/CP12-QUERY-CACHE.md`“预算与持久化”明确说：**没有 fsync 持久性承诺；丢失缓存可以重建。** `query_cache::publish` 原本使用完整 write、flush、close、取消检查和原子 rename；后端 `object_cache::store` 同样使用该模式，并明确崩溃/磁盘故障可以丢缓存而不能产生核验权限。CP22 文档只承诺请求成功后的完整原子发布，没有新增断电耐久性承诺。

私有语法缓存是可以重建的 memo。内容的完整性和原子可见性必须保留；掉电后仍能命中不是保证。丢失、截断、校验失败或不可解码的条目都走原冷路径，不为缓存恢复跳过独立 Verify。

## 改动

只修改 `crates/zil-driver/src/parse_cache.rs` 的发布部分：

- 用 `BufWriter<File>` 合并小的 header/payload 写入，显式传播 `flush` 错误，再关闭文件。
- 去掉逐模块 `sync_all`；同目录、完整写入后的原子 rename 保留。
- 仅在 `create_new` 成功后建立临时文件清理 guard。此前构造 guard 会在路径碰撞时删除未由本次请求创建的文件；独审已用真实 `-o` 产物路径复现。这是明确的失败清理所有权修复。
- 只有 rename 成功后才 disarm guard，避免对已移动的临时路径再做无效删除；写入、flush、取消或 rename 失败时仍清理本次拥有的临时文件。

保留原 Magic/key/length/checksum、codec 与读取检查，保留完整请求 Success、ArtifactPlan、输入/输出重叠保护和取消检查。新增 parse 目录仍只在成功发布阶段创建；既有 CP12 可在 ArtifactPlan 通过后提前创建其目录，因此不能将此扩写为“任何 Build 失败都没有 query-cache 目录”。

## 定向验证

新增 `crates/zil-driver/tests/parse_publication_cp26.rs` 使用真实 ZIL 源码和真实文件系统行为：

1. 大 payload 跨请求命中；损坏后修复；修复前打开的 fd 仍读到旧 inode，确认正式条目没有被原地截断改写。
2. 隔离进程中预占下次所有临时名称，验证 `AlreadyExists` 仅使缓存写入失败，所有已有 sentinel 和旧有效条目保持不变。
3. 将目标条目替换为非空目录，触发真实 rename 失败；已有目录内容保留，本次拥有的临时文件清理，解除障碍后可恢复缓存。
4. 子进程以 `ulimit -f 0` 和忽略 `SIGXFSZ` 注入真实 write/flush 的 `EFBIG`；请求仍正常 Verify，缓存写入失败不留临时文件，正常配额下重试可建立并命中缓存。限制和信号仅影响子进程。

同时复跑 CP22 的解析复用/预算/发现/损坏/取消与输出保护控制，以及 CP12 的受影响缓存控制。嵌套进程只算所属控制的一次执行，不与外层测试数相加。精确源码、命令、退出状态、原始日志和最终 release 收据记录在 `evidence/research/compile-performance/CP26/`；实际未运行的项不写成 Pass。

源码 `d8872b52f5a215c2efd33aaadba5083f471e0bc2` 的上述 22 项定向控制已通过。对应 release 又通过 5 次新进程 CLI、4 次原生执行：实际 `-o` 选为下一临时名称的反例不再误删 ELF；后续修复和热命中保持新 Verify，所有原生 ELF/LLVM 内容 hash 相等。详见该目录 `AUTHOR-RESULT.md`、`TESTS.json`、`RELEASE.json` 与 `cli/MANIFEST.json`。这些是发布正确性证据，未用于计算提速。

## 范围与后续测量

本片不解决同 Session 命中后重复持久化，不去重 source fingerprint 或 producer，也不增加语义增量。只有独立冻结的完整 CLI A/B 窗口才能说明编译性能；用测试数量、少了几个 syscall 或缓存 hit 数推导提速不成立。正式测量由根协调者安排，采用既有场景、请求权重、编译选项和原生控制。

四目标：源语言安全规则与独立核验保持，临时文件失败清理所有权得到修复；机器表示和运行路径不改，运行/峰值内存优势未测；语言表达不变，发布逻辑少一次不必要同步并明确资源所有权；冷/增量编译收益待精确窗口。没有据此宣称产品级突破、普遍超过 Rust 或完成语义增量。
