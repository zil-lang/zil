# CP12：可信本地前端检查点

本候选在 `542d30676c42e1b691287bcab55874799ca910c1` 上实现，授权合同见主树 `CP11-13-PLAN.md` 的 CP12。新增显式、默认关闭的 `--query-cache DIR`，用于 Linux 上完整文件项目 build 的**相同全输入**复用。私有体、公开接口、未用声明、路径或导入顺序发生变化，都保守失效；没有实现细粒度编辑增量，也没有本候选超过 Rust 的性能结论。

## 使用与可见状态

```sh
zil build project/main.zil -o out/program --backend llvm-api --query-cache /private/cache/frontend --json
zil build project/main.zil -o out/program --backend llvm-api --query-cache /private/cache/frontend --object-cache /private/cache/objects --json
```

缓存目录由调用者明确选择。新建目录为 0700、新文件为 0600；最后一级目录或条目是 symlink、目录/条目可被 group/other 写入时拒绝该缓存路径。缓存目录必须来自可信、私有的本地环境；这些模式检查不提供敌对同用户写入、恶意父目录或远端供应链的认证。

`CompileOptions.query_cache` 默认 `None`。JSON 新增独立 `query_cache` 对象：

| 字段 | 含义 |
| --- | --- |
| `requested` | 调用者是否显式启用 |
| `state` | `disabled`、`bypass`、`miss`、`hit` 或 `corrupt` |
| `key` | 本次完整 action key；尚未准备检查点时为 null |
| `reason` | bypass 或损坏原因 |
| `stored` / `write_reason` | 本次完整成功请求是否原子发布；可选写入放弃原因 |
| `ct_steps` / `ct_nodes` / `ct_functions` | 保存或重放的项目 CT 总逻辑用量 |

`corrupt` 表示损坏条目被当作 miss 后重建；若本次成功覆盖该条目，仍保留 `corrupt` 原因且 `stored=true`。命中后 Verify 失败保留 `hit`，报告本次失败，不重跑前端并二次 Verify。旧 `disk_cache` 仍专用于 `--cache-dir` 的不可信原始源码候选；LLVM 对象缓存保留原有统计与身份。

以下入口保留原管线并报告 bypass：`check` / stop-after、`--emit-dir`、内存 `compile_source` / `compile_project`、非 Linux host、显式 Win64 配置、动态加载器 interposition。CLI `run` 包含后续可能失败的执行，首版也明确 bypass；只缓存完整 build 请求。Managed 与其它原本不支持的目标仍按原入口拒绝。

## 当前请求仍执行的工作

1. 重新读取、发现完整源码图，检查文件与 `mod.zil` 歧义及缺失文件。
2. 验证源码身份、项目总源字节/路径限制，建立保护全部输入的 ArtifactPlan。
3. 使用当前源码执行解析和累计前端额度，重新解析导入与检查模块图。
4. 查询检查点。命中提供裸 `TypedModule`、`CandidatePlan` 和原始 `TypedProgram` 元数据；报告 Expand、CT、Types、Analyze 为 reused。
5. 进入与未命中完全相同的 `finish_candidate`，独立调用 Verify **恰好一次**，随后重新发布接口合同、lower、生成/复用机器对象、链接、验证并发布实际产物。

没有把磁盘数据注入原进程内 `RawSnapshot` / Database 的私有 construction digest。`result_origins`、`callable_contracts`、`callable_returns`、`contract_fingerprint` 的编码端要求全为 `None`，解码端只能构造 `None`；当前 Verify 成功后才由原 `contracts::publish` 产生新值。缓存中从来没有 `VerifiedModule` 或其它已核包装。

## 完整身份与信任前提

键包括原项目 query identity：schema/rules、完整 source fingerprint、模块及 FileId 顺序、每个路径、源码字节指纹、逻辑导入路径映射、全部 11 个 Limits、显式 `build_inputs` 和完整 checked target。另绑定解析后按原顺序的 imports（alias、module、span），以及 backend、optimization、profile、Windows 配置、CGU units/jobs、entry、output、显式 clang、旧 source/object cache 路径和 `ZIL_CLANG` / `ZIL_LINUX_LINKER` 环境值。

编译器生产身份复用后端已存在的 `trusted_installed_tool_identity(current_exe)`：GNU ELF build ID 加文件安装元数据，缺 build ID 时读取完整文件 SHA-256。此 helper 的实现、对象键及原 fallback 均未改变。它以**受信任且运行中不可被替换的安装工具**为前提；不是对任意可替换生产者的内容证明，也不认证有人重新计算过 checksum 的错误源→IR 映射。`LD_PRELOAD` / `LD_AUDIT` 非空会显式 bypass。远端/共享/敌对写入不在此缓存信任范围内。

每个条目有单独 payload SHA-256，检查头、完整 key、长度、实际字节及 codec；其中哈希是完整性检查。墙钟 deadline、诊断 timing 和 query-cache 所在目录不属于语言输入；当前请求仍独立检查取消。保守绑定 output 与其它 cache 路径会减少复用，不会扩大复用。仅影响 Verify 之后机器代码的选项须按独立授权适配，并由机器对象键承担实际生成输入的身份。

## 预算与持久化

`EvaluatedProject` 在明确启用、适用且路径准备成功时额外保存每个模块的 CT `steps`、生成 `nodes` 与最终函数数，顺序与原项目执行一致。读取在原 Limits 的余额上逐模块 checked-sub；任一溢出、顺序/覆盖不符都在进入 Verify 前作为 miss。全部 Limits 已在 key 中，不同配额不会借用旧成功。Expand/Types/Analyze 的确定性成功只可在同输入、同规则、同完整限额下复用；没有把任何限额调高或把部分成功缓存。

传输仍调用原 Wire27 `encode/decode_module_candidate`，字节格式未修改。原始项目接口另用私有 `ZILQMD01` metadata codec，外层 `ZILQCP01` 封装包含两个长度、逐模块 CT receipt、原 Wire27 和该原始 metadata。读写使用原 Limits，整体 payload 再受 `min(max_source_bytes, 128 MiB)` 限制。合法源码可能比 typed wire 小；codec/传输限额失败只导致读 miss 或跳过写入，不能把原来成功的源码改成 ResourceLimit。

只有原完整编译、Verify、链接与产物发布全部成功之后才写检查点。临时文件 `create_new`、写完整内容并 flush、检查当前取消，然后原子 rename；错误路径清理临时文件，保留旧条目。已取消请求、Verify/链接失败均不生成新有效条目。没有 fsync 持久性承诺；丢失缓存可以重建。

如果同一个内存 session 在关闭该选项时已产生 CT 快照，随后开启缓存，旧快照没有 usage receipt，允许原内存复用但不将其持久化，并给出 `write_reason`。默认关闭路径不为检查点分配 CT receipt。

## 限定验证与证据

作者只运行限定测试，没有正式性能采样。最终 19 项 CP12 控制及 45 项既有回归通过，重复运行不叠加计数。命令、失败和结果在 `evidence/research/compile-performance/CP12/`：

- Driver 新增真实 3 模块源码测试：冷/未变/私有编辑/公开编辑/恢复输入、接口重新发布、旧 source cache 分列、入口 bypass、当前歧义/缺失/解析错误/产物别名先拒绝、CT N/N−1/fresh N、七种损坏与有效校验和 codec/usage 反例、有效校验和但无效 candidate 的一次 Verify 拒绝、链接/取消旧产物保护、可选传输额度及目录信任边界。
- 单元控制：实际 key 的全部 11 Limits、安装生产者/源身份/规则/target/imports/build option 漂移，CT 三维累计用量与模块顺序，四种接口已核字段禁止编码，metadata 边界，以及取消时原子条目保护。
- CLI 独立进程：冷/命中、同 ELF 字节重新安装导致 miss、默认关闭与缺参诊断、interposition/run bypass，以及 LLVM API 对象缓存和前端缓存共同命中时 Verify 仍为一次。
- 原项目与文件发现 session 回归，以及原 Wire27 单元控制单独核对。

第一次 check 的 TimingTrace 接口返回类型错误保留在 `check-01.log`；修复后 `check-02.log` 成功。第一次原项目回归在稀疏 worktree 缺 `examples/numberlab` 的 include 文件，保留 `driver-targeted-02.log`；补齐同基线示例后重跑，未修改示例内容。

## Rust 四目标与剩余义务

| 目标 | 本轮判断 |
| --- | --- |
| 同类安全 | 当前请求独立 Verify、原拒绝/预算/失败清理保留；可信生产者前提已显式化。不能推为全语言安全证明。 |
| 运行与内存 | 测试执行真实原生程序并核对输出/状态；没有同语义 Rust 运行时间/RSS 比较。 |
| 表达与维护 | 一个显式研究选项，不改变语言源码或公开 Wire27。整项目 memoization 的维护收益尚未测量。 |
| 编译成本 | 删除 unchanged 完整 build 的 Expand/CT/Types/Analyze 重算；序列化、解码、哈希与额外 I/O 成本尚待协调者冻结后的正式比较。 |
| 实质突破 | 是可独立审查的缓存候选。私有/公开编辑必 miss；不能把 unchanged 的局部收益代替完整四请求增量或超越 Rust 的结论。 |

正式计量、runner 选项支持与新的计划冻结另行提交，旧历史结果与产品门保持原样。
