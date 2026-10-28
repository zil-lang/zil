# 205 旧完整 State 转交来源修复

root 的正式 `workspace01@0317a093ade0118741d334c45438ade07e9d7228` 中，原
`impl177_state_composition::complete_local_assignment_restores_liveness_after_rhs_transfer`
合法源在 `(next)=relay(next)` 被 Types `E-STATE-ORIGIN` 误拒。原 867 字节输入
SHA256 为 `dc68299194fcc8dcea73fad37540a270bf16fe497235a8c6858d1bff750e199e`；
原失败、旧测试和 Success 预期均保留。正式派工见主树 205 工单与
`changes/205-workspace01-followup.md`。

旧单 State 的公开 Core 头把 F bound 放在拥有 Package 参数的位置，而没有
`state_parameters`。205 新启用的私有声明来源检查曾把该参数的完整
`Shape::Package` 直接替换进结果的每个 F 字段，导致原来的字段来源被多嵌一层
Package。`d71758d2be95995aeb4a9b690fa62ee0095a3797` 改为逐个核对完整参数
Package 的直接 `Named(F)` 字段，提取并统一实际来源，再以该来源替换结果 F。
它保留声明 callable kind、完整字段顺序/名称、独立实际位置和原 MoveWalk，
没有制造新的公共 State 表、借用权限或 Copy 资格。新增字段/name/shape 工作先计费。

同一 clean 提交的有限作者复核：

- 原 177 exact 测试 1/0；两份原合法源各 O0/O2，共 4 次原生。867 字节原源
  仍输出 `ZA`、退出 0；原重复转交反例仍拒绝。原 177 文件未修改。
- 205 source/raw/budget 三个 binary 为 25/0，分别 16、7、2；source 共
  58 次原生，其中循环 0/1/3 次各 O0/O2 独立记录。
- 新 raw 族对一/两字段旧单 State，在两个公开 Types 入口检查 F→G 重命名转交、
  FnOnce 保持、拒绝加强到 FnMut、接受更强来源、旧 alias 转交后失效和 last-use
  在转交前合法。共 10 个 Core 对照，各检查两入口，不计原生执行。
- 既有独审 single-F producer 测试仅由作者重用作回归控制：1/0、4 次原生；
  不替代非作者审核。
- 205 原固定 Types 边界 5293、88、4498、2935 全保持；原 N、N−1 ResourceLimit、
  fresh N 完整 Typed 相等断言均通过，没有修改这些预算。
- 限定 `zil-types` / `zil-driver` lib 与 205 source/raw/budget 的 Clippy 通过。

初次 dirty 实跑的补丁和完整生产/测试源码快照，与 clean 重跑的命令身份分列。
精确摘要为 `evidence/impl/205/types/LEGACY_FORWARDING.json`；完整阶段、源、
raw Core、结果、stdout/stderr 在独立紧凑证据包中，执行产物仅保哈希。

这些结果仅是此修复的作者限域复核。root 新正式整仓、修复后 CLI 的真实消费者和
非作者最终审核各自记录；本文件不晋升完整 1.4、B0、产品或原生 Windows 状态。
