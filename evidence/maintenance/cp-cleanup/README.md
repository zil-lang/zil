# CP245 清理收据

已删除十三个远端研究分支及一个已进入主线的本地旧研究分支。远端删除使用同一次原子 push 和逐枝精确旧 OID lease，实际读回见 `retirement.json`。没有删除生产代码、测试、历史证据或工具链，没有运行正式全量，也没有编译提速结论。

十四个原远端头均保存在远端标签 `archive/cp-245-20261102`。标签 OID 为 `3f072a39549a80bf0d5df27de1197f3402bb154b`，归档提交为 `27ccb6d23652dba6cc3461434f5d5c48aced37de`，其十四个父提交就是被审计的原头。归档提交使用冻结基线的树，仅用于保存历史，不是主线合入或研究采用。十三个研究头仍有非等价、未合入提交，因此全部先保留历史，再删除在线分支引用。

已从 GitHub 将标签完整取回无 alternates、非浅克隆的独立 bare 库，逐头核对 OID/tree，并通过 `git fsck --full --no-reflogs`；见 `remote-restore.json`。另外，57,799,332 字节的差异 bundle 已独立恢复并核查全部可达对象，见 `restore.json`。此额外副本位于 `/workspace/attachments/zil-cp-cleanup-245/research-heads.bundle`，SHA256 为 `999cee84f00d9ab81da9a6b709fe813627eab9c89051b8c65f003b01a74ebc5e`，没有重复写入主线 Git。

保留 `task/cp25-reactive-session@e739f583b63ba8fa44fa26a937d36afb9cbaea2a`：其末次正文仍为施工中、严格哈希发布未完、独审 NotRun，且保存的源码补丁只有三个分片。它是唯一在途工作，不能因名称或提交标题删除。

无 research/CP worktree 或可清理的本地 target/.cache/out；当前语言任务工作树全部保留。两个独立恢复验证产生的临时对象库均已移除。`.zil-env` 未改。主树 `.agent/git-branches.md` 的旧长期分支入口仍是历史指针；本次明确清理授权覆盖其旧保留要求，施工写集不包含该文件。

恢复任选原头 OID（完整映射见 `audit.json`）：

```sh
git fetch origin refs/tags/archive/cp-245-20261102:refs/tags/archive/cp-245-20261102
git branch research/compile7-20261031 a894f8476cd043314113ba40b4a492deaf281895
```

完整路径、提交和 `git cherry` 行在 `audit-detail.json.gz`；精简清单在 `audit.json`，结论在 `receipt.json`。差异 bundle 以 `81b7ae0c9ce5e7245613ed536b4702c419fe99bd` 的祖先对象为前置；远端标签可独立恢复，不依赖该本地附件。

交付分支已吸收 root 的 `77b1c8209` 合同修复。主树 handoff 检查为 1144 项既有历史错误、0 警告，本单 245 为 0 错误；见 `handoff-final.json`，不报告整体通过。Rust 四目标和产品突破均未因维护晋升；安全、运行/内存、编译性能没有本轮新实证，维护改进仅为减少在线冗余引用并保持可恢复性。
