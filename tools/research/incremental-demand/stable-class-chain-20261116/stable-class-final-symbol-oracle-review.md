最终物理符号 oracle 增量独审

结论：本次两文件增量未发现新增 blocker；新增 class 生存与原 source 删除分账符合当前完整核验链，不是前缀放行。

只读核对 `404aafdc7844ff3d85a08edb3fd0da0f2046816a` → `ea5349ed069f91f8baaeeb811a421ca8c805bb6c`（tree `8cb2af98299b522c511dd4e3cd9ff9b1d22a2f84`）：仅 closed_project_quality.py 及其测试文件变化；这两个工作树文件与该提交一致。compiler 源码相对 404 未变。未运行任何编译、测试、重放或计时，未修改仓库。

`quality` 会先重新调用 `executable_abi`，覆盖 row 的 scalar_abi；后者重新 hash 校验归档 IR 并执行 scalar_model，完成原 body 全值见证及纯 DAG 验证，同时检查实际 object/ELF 符号一致、owner/binding、cross/foreign/relocation。返回的 class_sources 因而来自本次验证，不采信预填 row 或 inventory 的同名前缀。

令 B/C 为扣除精确 private-import 名称后的 baseline/candidate canonical 物理符号集合，K 为已验证 class_sources 的键，D 为 inventory 中实际布局定义减去 canonical 物理定义得到的 removed。新增条件分别保证：

- `C−B ⊆ K`：新增物理定义只能是已验证 class；未知 class 名也不能进入。
- `B−C = D−K`：原 source 删除仍逐个精确核账，不因 dead class 混入 removed 而失配，也不退化为子集检查。
- `D ∩ K ∩ C = ∅`：活的 canonical class 不能同时谎报删除。私有 import clone 有独立完整名称与 LOCAL/owner 核验，不能冒充 canonical class；canonical class 删除而其 import clone 生存是允许的不同角色。

dead class 是否完整列入 D 仍依赖此前 inventory 的实际 current layout/物理集合差及 retained inventory 的来源绑定；这三条 aggregate 条件本身不是独立重建全部 D 的算法。本次没有删除该前置链，也没有扩大 protected export、ABI、O2 或 runtime 豁免。main ResourceLimit 时原有无 baseline aggregate 比较分支保持，不能据此声称三臂 aggregate 已比较。

新增回归源码覆盖合法 live/dead class、未知 extra、漏记 source removal、活 class 谎报 removed；其中 executable_abi 被 mock，验证的是集合核账逻辑，不替代已有真实 body/ELF 门。22 tool tests 与真实 cold 三臂 raw 整体 quality 离线重放通过、retain PASS/native 运行均为 root 通报，本审阅不将其扩大成完整 native 或性能通过结论；两次首败保留。
