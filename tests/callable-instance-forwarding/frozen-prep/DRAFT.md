# F→F consumer preparation — draft only

所有文件仅在 `/workspace/.zil-env/forward-prep/`；没有修改 Git 工作树、正式工单、规范或生产代码。本目录不是 256 交付，不含 native 执行或验收 Pass。

`frozen-original/source.zil` 来自既有 `evidence/planning/owned-callback-finalR/f-identity/source.zil`，逐字保留。`project/` 恰好三个模块：model 提供真实拥有数据/marker，relay 提供单 F 按值返回同一 F 的 identity、局部移动、分支、nested keep 和 FnMut 先调用再返回，main 消费这些 API。普通不可 Copy Value 的局部移动使用既有 `let current=f`，没有发明 `move f` 或 `give F` 新拼写。`probes/` 单独隔离 Fn/FnMut/FnOnce × Copy/Event→Reply 六种签名，避免一个早期入口掩盖其他签名。

模式与身份分别核验。具体 Fn 或 FnMut 穿过声明 FnOnce 的 keep，返回调用者后恢复该调用者原先的声明模式；nested keep 内部返回只恢复到上一层的声明模式。因此外层声明 FnOnce、实际实现为 Copy Fn 的位置不能在 keep 返回后升级为 Fn 或 Copy；外层声明 FnMut 的位置也不能仅凭实际 Fn 在不可写局部共享调用。Fn 不授予通用 F Copy。返回关系绑定实际输入位置/实例，不以 ClosureId 相同代替；两个 model.marker(65/66) 是同一工厂的不同实例。

独立预期见 ORACLE.json，由 oracle.py 仅按源操作生成，完全不读取 baseline 输出。13 个普通输入及 41 个计划 allocator 行均是未来实现后的预期，不是实测。主要输出：0/1 `SaaABQ`（A/B 同 ClosureId 不同实例）；2 `SCCCQ`（具体 Copy Fn 原位置仍可用）；3 `S123MQ`（FnMut 在 helper 内先变为 1，返回后继续 2/3）；4 `SDHhQ`（拥有 FnOnce 未调用销毁）；5 `SDCHhRAQ`（调用后清剩余捕获）；6 `SDHhQ`（后项准备取消，无 C）；7/8 `SDaAQ`（拥有 ErrorUnion 两个真实槽分别作数据）；9 `SCRACRBNQ`；: `S1RA2RBNQ`；; `SaaAQ`；< `SDaAQ`（直接 Product 伴参）。

拥有 FnOnce 的三个 reserve 点是环境缓冲、普通伴参 D、实际调用 Event A。环境构造前失败预期 `ShHQ`，首个 F 参数已经准备后后项失败预期 `SDHhQ`，forward 已返回后调用实参失败预期 `SDAHhQ`。逆序局部清理与按 capture 声明序清理因此能分开观察。模型枚举 disabled/逐分配点/one-past，并要求预测 live=0；尚未运行 allocator 注入。

BASELINE.json 记录当前 CLI 的真实结果：15 个独立 source graphs 各 parse/check 一次，30 次矩阵 CLI 调用；8 个拟晋升合法源都在 Expand Unsupported，7 个非法近邻也被同一个缺失正能力挡住，后者不能算正确语义拒绝。另有最初两次 source preflight 单列在 PREPARATION.json；Verify=0，native=0。binary SHA 为 a07605bd547068bc0220ea459a66b7f1e2a8d3b085fe78ae5190db15becb13b6，其构建身份 fa8638df7e9fc30e717e23191252bb1600d1e42f；root 已核生产输入等价于受测 R288e296。当前 main 仅作来源定位，不能冒充 binary 构建提交。首次未 source env.sh 的 LLVM loader 错误单列为环境调用错误。

需要 root 在正式合同决定的两点：普通伴参是否包括已支持含 Store 的 Product（此草案只用有限 Mark/ByteBuffer/Product/Enum/ErrorUnion）；F 显式签名首片是否仅 Copy/已验拥有值，或也承接旧 Copy-address profile（此草案没有据此增权，外部借入环境明确负例）。公开返回来源的表示由 root 冻结：必须能表达“返回同一输入角色实际值”，不能只写结果 ClosureId/类型相同。

后续 raw/Wire 最近攻击计划：把合法 F 返回替换为同 ClosureId 的新 marker 实例；分支一侧偷偷换实例；保留输入清理却又返回同一责任；双返回/重复消费；删掉 FnMut 变更；把外层声明 FnOnce 返值伪装为 Fn/Copy；借外部 Address/Desc 或 child 环境夹带借入；把 F 藏在普通伴参。所有非法例不 native。新旧精确预算与机器证据待正式合同/实现后另采，不预造 N。
