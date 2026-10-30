# CP27：查询与对象缓存的临时文件所有权

## 结论与状态

CP27 修复两个既有缓存发布入口的文件所有权错误。只有本次 `create_new` 成功创建了临时文件，失败清理才有权删除该路径；发布成功后不再清理旧临时路径。生产改动仅涉及 `zil-driver/src/query_cache.rs` 与 `zil-backend-llvm/src/object_cache.rs`，基线是 CP26 的 `d8872b52f5a215c2efd33aaadba5083f471e0bc2`。

作者限定验证已通过 31 项：查询缓存单元测试 8 项、对象缓存单元测试 10 项、CP27 真实 ELF 碰撞集成测试 1 项、已有 CP12 集成回归 12 项，均无失败或忽略。release 已成功构建并冻结，非作者对同一源码和 binary 给出 scoped correctness Pass。该结果不等同于整体产品门禁通过。CP27 没有性能结果，不据此宣称增量编译或整体编译提速。

## 原始问题与归属

非作者在原 `485301e1f1dcdfb0b4e24f6249355104243c3fe3` 冻结 CLI 分别复现了两处问题，CLI SHA-256 为 `0f8325ebd29ed14afc159f2def11998ac829043c8900ed7b799fb06ce69cd553`。该问题早于 CP22/26，不归因于解析缓存的新增工作。

发布路径具有 `.pending-<pid>-<counter>` 的确定性名称。测试令本次刚构建的 ELF 输出占据第一个临时路径。缓存写入尝试 `create_new` 时正确收到 `AlreadyExists`，但原清理代码仍删除了该路径。结果是本请求报告 `Success`、`verification_runs = 1`，已发布的 ELF 却消失，LLVM 文本副产物仍在。关闭对应缓存的控制保留了 ELF，并能正常执行。

原始证据封存在主研究树 `evidence/research/compile-performance/COMPILE6/CP12-review-finding.json` 与 `OBJECT-review-finding.json` 及各自引用的脚本、stdout、结果和产物存在性记录。CP27 不覆写这些失败窗口，也不将此前的测试通过改写为已覆盖该边界。对象缓存的实际状态位于报告的 `native_codegen.object_cache`；原复现脚本顶层 convenience 字段为空的事实保留，判定依据使用独审已明确的嵌套原始字段。

## 最小修复

### 查询缓存

`Prepared::publish` 原先先构造 `Temporary`，再打开文件。任何创建失败都会触发守卫的 `Drop`，错误地删除已有路径。

候选先生成路径，再执行 `create_new`。只有打开成功后，才建立 `Temporary(Some(path))`。写入、flush 或取消失败仍清理本次创建的临时文件。`rename` 失败继续保留旧最终文件并清理临时文件；`rename` 成功后将守卫设为 `None`，不再对已释放的旧名称执行 unlink。

### LLVM 对象缓存

`store` 原先把 `OpenOptions::open` 放在统一失败清理的闭包内，因此创建失败也进入 `remove_file`。候选仅将 `open` 移到闭包之前，并使用 `?` 提前返回。后续写入、flush、rename 失败保留原有清理行为。成功 rename 原本就不会进入错误清理分支，因此不增加新的守卫或抽象。

两处都保持现有文件格式、key、摘要、长度限制、缓存报告、资源计费、取消与 Verify 路径。查询与对象缓存原本使用的 flush/close/rename 顺序不变；没有新增、移除或扩大 fsync 和断电持久化承诺。

## 四处发布入口的静态核对

| 入口 | `create_new` 失败时的所有权处理 | 本片处理 |
|---|---|---|
| `zil-driver/query_cache.rs` | 原守卫过早 arm，已存在路径会被误删 | open 成功后 arm；rename 成功后 disarm |
| `zil-backend-llvm/object_cache.rs` | 原统一错误分支包含 open 失败 | open 移出后续失败清理作用域 |
| `zil-driver/parse_cache.rs` | CP26 已在 open 成功后 arm，rename 后 disarm | 保持基线 |
| `zil-db/cache.rs` 的 `ArtifactCache` | 守卫已在 open 成功之后构造 | 此次排除了同类 `AlreadyExists` 误删；未修改 |

raw `ArtifactCache` 在成功 rename 后仍会尝试删除旧临时名称；与另三处实现不同，但正常单进程 PID 加单调序列不会重新使用这个名称。本轮没有复现其错误删除，也不把外部主动重建同名路径扩大为新的受信本地威胁模型。这里的审计结论仅针对已经复现的创建失败误删边界。

## 有意义的回归控制

新增测试仅运行于 Linux，使用真实文件系统错误，不加入生产故障注入钩子。

| 控制 | 查询缓存 | 对象缓存 | 验证的可观察结果 |
|---|---|---|---|
| 临时路径已存在 | 独立精确测试子进程 | 独立精确测试子进程 | 固定首次序号 0；已有 sentinel 的字节、inode、mtime 不变，旧最终缓存不变 |
| rename 失败 | 最终目标为非空目录 | 最终目标为非空目录 | 目录内容保留；本次临时文件被清理；去除障碍后可发布 |
| 真正写入失败 | 子进程 `RLIMIT_FSIZE = 0` | 子进程 `RLIMIT_FSIZE = 0` | 得到 EFBIG；旧最终缓存不变；无孤立临时文件；限制解除后重试成功 |
| 原子替换 | 重试后保留原打开的读 fd | 同左，并经对象 loader 校验 | 原 fd 仍看到旧完整文件，最终路径看到完整新文件 |
| 真实 ELF 同名碰撞 | driver 集成测试 | 独立评审使用冻结 CLI 回放 | 缓存 off、碰撞、修复、热命中均保留可运行 ELF；每次重新 Verify |

查询缓存已有的取消发布测试继续验证取消时旧文件保留与临时文件清理。已有 key、元数据和 CT 收据测试，以及对象缓存的损坏校验、并发发布和成功后才提交的测试保持不动。新的所有权断言不把临时文件数量替代为产物成功：真实构建控制还要读取 ELF 魔数并执行产物。

## 验证与构建口径

作者使用独立 `target-cp27`，`CARGO_INCREMENTAL=0`、`CARGO_PROFILE_DEV_DEBUG=0`、`CARGO_PROFILE_TEST_DEBUG=0`、`CARGO_BUILD_JOBS=2`，离线且锁定 Cargo。范围包括两处库的相关单元测试、CP27 的 driver 集成测试，以及现有 CP12 的相关行为回归；不启动全 workspace 产品门禁或性能窗口。

release 使用独立 `build-cp27`，保留精确提交、crates tree、完整命令、日志、binary SHA-256 和 metadata 收据。工具链沿用本轮实测的 Rust 1.99.1 与 LLVM/Clang/LLD 23.1.4；Rust 自身的 LLVM 为 23.1.1，ZIL 使用的 libLLVM 由校验过的官方 23.1.4 静态组件在本机生成。上述配置不构成跨宿主或通用工具链性能结论。

原两项失败已有冻结 CLI 证据，因此不额外重建整个旧候选以重复红灯。作者完成限定回归后交给非作者 `/root/incremental_review` 对精确提交与冻结 CLI 进行复核；独审仍是必需的接受门槛。

## 作者验证收据

`evidence/research/compile-performance/CP27/author-tests.json` 记录完整命令、工具版本、相关源码 SHA-256、开始/完成时间和逐项日志 SHA-256；同目录保存原始日志与可重放脚本。库筛选未执行其余单元测试：query 为 28 项 filtered、object 为 38 项 filtered，均如实保留在原日志中。本片不把 31 项定向通过称为全库或全产品验收。

## 冻结候选与构建

- 源码提交：`d9a71f603d73c1df4a367189cc5b41ca15bf7d70`。
- crates tree：`efd735f8821637c1876708cd19d5b649fa70530c`。
- release binary：`build-cp27/release/zil`，SHA-256 `b82d690e8e26c6337f02d67cf515d816ed833db6b6d69ff6f66db49a889db7d3`，7,614,536 字节。
- 命令：`cargo build --offline --locked --release -p zil-cli`，独立 `CARGO_TARGET_DIR=build-cp27`、`CARGO_INCREMENTAL=0`、`CARGO_BUILD_JOBS=2`；没有静默替换 LLVM 后端或 flags。

同目录 `release-build.json` 保存完整绝对路径、环境、工具版本与 SHA-256、binary 原 inode/mtime/ctime、构建日志摘要。基线、CP21、CP23、CP26 四个旧冻结 binary 及 Rust/Cargo/Clang/LLD/libLLVM 的 metadata 构建前后完全一致；新 CP27 binary 也在下一次独立工具调用核回了相同的 SHA-256 与 metadata。后续文档/证据提交不改上述 crates tree，也不将重新构建的文件冒充原冻结二进制。

### 工作树模式异常的边界

本环境把 `tools/research/212/bench.py` 从 Git 指定的 100755 执行位恢复到 100644 的现象曾跨工具调用重复发生。内容一直匹配基线 blob `048f8e0fc454aef32afbef33b14040547b780a3f`；提交前恢复执行位并核对 staged 写集，该文件没有进入 CP27 提交。没有更改 `core.filemode` 或用 skip 标志隐藏差异，也不声称工作树每时刻都完全干净。该脚本不属于本片 crates/Cargo/LLVM 构建输入，受测源码与 flags 由精确提交和单独收据绑定。

## 独立复核结果

`/root/incremental_review` 对精确源码、crates tree 和冻结 binary 给出本片正确性 **Pass**，完整判定与限制保存为 `CP27-review.json`，SHA-256 `e5119e546c21ebfa2a130be520126861c3a247af84b7fad233fed56b778f0824`。

评审者独立复跑了同一批 31 项测试；其执行复用了 `target-cp27` 已有开发构建产物，没有宣称独立清空后重建。另对冻结 release 执行查询缓存 4 次 CLI/4 次 native、对象缓存 5 次 CLI/5 次 native，共 9 次完整请求与 9 次真实 ELF 执行。两类原始同 PID 临时路径碰撞都保留了已发布 ELF；碰撞依然报告可恢复的缓存写入失败，之后修复和热命中正常。每种场景的 ELF 与 LLVM 文本 SHA-256 均与该场景缓存关闭控制相同，9 次请求各自重新执行 Verify 一次。

评审没有另行 clean rebuild release；源码与 binary 的绑定采用作者冻结构建收据、精确 crates 身份和独立重核的 binary hash。这里的 Pass 关闭了该候选的两个临时文件所有权 finding；原基线 ChangesRequested 和原始失败证据继续保留，也没有据此宣布性能采用或全产品验收。

`independent-evidence-manifest.json` 列出归档原始 JSON、stdout/stderr、日志、脚本、源码与 LLVM 文本的逐文件 SHA-256；实际运行生成的 ELF 和缓存 payload 保留原路径及 hash，不向 Git 重复添加编译产物。原始文件内容未为适应归档路径而改写。
