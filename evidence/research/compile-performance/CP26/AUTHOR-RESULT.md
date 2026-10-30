# CP26 作者结果

基线 `7ca57fb61aa6fe4f7cac963a531a6cf2851792e4`，独立分支 `research/cp26-parse-publication`；未修改旧 CP22 工作树、二进制、原窗口或历史判断。最终源码与 release 身份由本目录机器收据绑定。

| 冻结项 | 身份 |
|---|---|
| 源码提交 | `d8872b52f5a215c2efd33aaadba5083f471e0bc2` |
| crates tree | `7e5def64ddddb7def8637bb96d8ab69f6f3813e6` |
| release | `/workspace/scratch/4ab361722127/build-cp26/release/zil` |
| SHA256 | `c9f10a9840fd8bdbb884e4981c7dbb5895ebfe653ecb7b486b1979c16441278f` |

构建为 `cargo build --offline --locked --release -p zil-cli`，无显式 features，`CARGO_INCREMENTAL=0`、`CARGO_BUILD_JOBS=2`，与本轮基线相同；完整命令和工具链在 `RELEASE.json` / `release-build.log`。源码冻结后只有证据和说明文件更新，未再改动或重建二进制。

## 已完成

生产仅改 `zil-driver/src/parse_cache.rs` 发布 I/O：BufWriter、显式 flush/close、同目录原子 rename；仅在 create_new 成功后拥有临时文件清理责任，rename 成功后 disarm。缓存键、codec、预算、语义图、取消检查和独立 Verify 不变。

新增 4 项真实文件系统控制与原 CP22 6 项、CP12 12 项，共 **22/22** 通过；原始日志 `driver-publication-tests.log`。新增控制覆盖大 payload/旧 fd 原子替换、未拥有临时路径碰撞、rename 失败与真实 write/flush EFBIG。子进程没有修改父进程文件限额/信号，嵌套执行不重复累计测试数。CP12 使用原 Clang 后端并实际核对原生返回/输出。

现有 CP12 明确不承诺 fsync 耐久性；本片继续允许掉电后缓存丢失并重建。没有进行断电试验，不能宣称文件系统掉电持久性。独审发现的 CP22 未拥有文件误删属于旧实现缺陷；本片修复该缺陷，不把旧反例隐去。

## 真实 CLI 与原生产物控制

`cli_publication_controls.py` 在五个新 CLI 进程中执行种子 check、无缓存输出碰撞路径 Build、开启缓存的同类 Build、正常路径修复 Build 和热命中 Build。四个 Build 的可执行文件均实际运行，退出 0、stdout/stderr 为空，ELF 与 LLVM sidecar 的四份 SHA256 全部相等；每个请求都执行独立 Verify 一次。

开启缓存的碰撞请求先冷解析损坏项，随后 `create_new` 返回 AlreadyExists，报告 `stored=0` 与 `write_reason`，但已发布的 ELF 和 sidecar 保留，旧损坏缓存条目也未被覆盖。解除输出碰撞后下一请求修复缓存；最终请求 parse hit 1、parsed 0、stored 0，并命中 CP12，仍重新 Verify。原始 CLI/native stdout、stderr、完整 argv、产物 hash 和报告在 `cli/MANIFEST.json` 及同目录文件。

这是独审 CP22 实际误删产物反例在 CP26 上的恢复控制，不是性能采样。输入文件、producer、当前配额与缓存检查没有因控制而放宽。最后再次核对 release hash 与 dev/inode/size/mtime/ctime 不变。

## 验证边界

- 此为限定测试与研究交付，不是全仓或产品门验收。
- 冷启动 `node tools/handoff.mjs --check --main-tree /workspace/scratch/4ab361722127/zil` 仍为 Fail：恢复环境只有研究分支、缺 normative registry，未把该状态记 Pass。原始输出保存在 `handoff.log`；主树研究工单、精确 baseline、已派定写集及用户授权由根协调者明确提供。
- 恢复环境中一次全路径 `git diff --stat` 返回 139 且无输出；后续指定路径 diff、status、write-tree 和源码提交均正常。没有删除历史或缺失文件来绕过 Git 检查。
- 运行/内存性能、表达能力不因本片改变；编译收益等待根协调者独立冻结的完整 CLI 窗口。测试数量和 syscall 减少不是性能成绩。
- 本片仅修 parse-cache 发布。独审另外发现的旧 CP12/object cache 同类所有权缺陷交由根派定 CP27 处理，不把本片有限修复扩大成全部缓存已安全。
