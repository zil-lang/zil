# CP26 作者结果

基线 `7ca57fb61aa6fe4f7cac963a531a6cf2851792e4`，独立分支 `research/cp26-parse-publication`；未修改旧 CP22 工作树、二进制、原窗口或历史判断。最终源码与 release 身份由本目录机器收据绑定。

## 已完成

生产仅改 `zil-driver/src/parse_cache.rs` 发布 I/O：BufWriter、显式 flush/close、同目录原子 rename；仅在 create_new 成功后拥有临时文件清理责任，rename 成功后 disarm。缓存键、codec、预算、语义图、取消检查和独立 Verify 不变。

新增 4 项真实文件系统控制与原 CP22 6 项、CP12 12 项，共 **22/22** 通过；原始日志 `driver-publication-tests.log`。新增控制覆盖大 payload/旧 fd 原子替换、未拥有临时路径碰撞、rename 失败与真实 write/flush EFBIG。子进程没有修改父进程文件限额/信号，嵌套执行不重复累计测试数。CP12 使用原 Clang 后端并实际核对原生返回/输出。

现有 CP12 明确不承诺 fsync 耐久性；本片继续允许掉电后缓存丢失并重建。没有进行断电试验，不能宣称文件系统掉电持久性。独审发现的 CP22 未拥有文件误删属于旧实现缺陷；本片修复该缺陷，不把旧反例隐去。

## 验证边界

- 此为限定测试与研究交付，不是全仓或产品门验收。
- 冷启动 `node tools/handoff.mjs --check --main-tree /workspace/scratch/4ab361722127/zil` 仍为 Fail：恢复环境只有研究分支、缺 normative registry，未把该状态记 Pass。原始输出保存在 `handoff.log`；主树研究工单、精确 baseline、已派定写集及用户授权由根协调者明确提供。
- 运行/内存性能、表达能力不因本片改变；编译收益等待根协调者独立冻结的完整 CLI 窗口。测试数量和 syscall 减少不是性能成绩。
- 当前已完成源码与限定测试冻结准备；release 和真实 CLI 产物碰撞恢复将在后续收据记录，不在未执行前标记通过。
