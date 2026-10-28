# 166：完整 Package 错误联合的有限纵向切片

依据当前 `_zil_design.md` §7、§8、§15.2 与 §15.7；生产基线
`45dbb920d444ab54a307785d068c934926cfb90c`，派工 `4ba11e1`。本片不改规范、
默认预算或产品门。公共 IR/Wire/schema 由 root 单写；本文先于生产变更提交。

## 接受范围与真实责任

沿用 `Type::ErrorUnion { error, result }`。第一片允许两侧分别为现有有限 Copy 值或
完整有限 Package，至少一侧 Package 时整个联合为非 Copy 的普通 Value。只在活动分支
持有实际字段责任；Copy 活动分支也不使静态非 Copy 联合可复制。直接 Managed/Desc、
闭包、地址以及递归拥有联合载荷保持关闭，不能从调用需求猜测 create/give 结果模式。

明确期望类型时沿用唯一一侧匹配的注入；普通 Value 通过已有 Store/TakeLocal、参数与
返回传递。成功/错误两侧均允许完整 Package，`?` 只沿真实 tag 传播并保持错误类型相同。
局部丢弃、未调用消费者、返回准备和后续参数失败都必须清理当前实际持有的活动载荷。

公共最小需求为 `TakeErrorUnion { value, is_error }` 与
`DropErrorUnion { local }`。MakeErrorUnion 对拥有载荷真实消费；Take 消费整个联合，
取出实际活动载荷；ReadErrorUnion 继续只服务 Copy 联合，不能用于复制拥有载荷。
ErrorTag 不消费，但必须读取仍有效的 SSA 联合。独立 Verify 从真实 CFG 重建拥有 Take
的 tag 前置和唯一责任，不相信生产方的候选说明；错误分支或缺少 tag 依据不能制造责任。

## 清理、机器表示和预算

保留现有内联 `{ tag, E, T }`，非活动槽置零，且从不因其字节存在建立或清理另一分支
的责任。DropErrorUnion 先退出 local 的 live 状态，再按活动 Package 的固定字段顺序
清理。原生 main 若返回拥有错误载荷，入口 wrapper 必须在返回错误退出码之前清理该
载荷；不能丢弃它或用 abort 代替正常错误清理。没有隐式装箱、RC、堆表或新的分配 ABI。

Analyze、独立 Verify 分别分类整个联合的非 Copy 责任并处理构造/取走/传参/返回/销毁；
作用域、初始化和活性检查必须覆盖新指令。Types、Lower、后端各自只开放此有限载荷集。
新的类型/CFG遍历沿现有深度、节点和分析步数预算计费；超界返回现有 ResourceLimit。
字节资源定义发现须覆盖联合内的 Package；拥有联合的捕获、借入、CT 与一般嵌套载体
不会因本片准入而顺带开放。

## 验证

真实源码覆盖两侧注入、普通局部及跨函数传递、连续 `?`、未取出销毁、活动分支唯一
清理、返回/参数准备失败和 ByteBuffer 回收，O0/O2 均经过独立 Verify 后实际执行。
最近反例覆盖重复注入/取走、Load 复制、Read 冒充 Take、错误或未知 tag 提取、漏清理、
非活动分支清理、部分 Package 注入以及旧 Desc/窗口在转交后使用。raw 与 wire 从同一
真实源码候选分别重核；错误候选不重新调用 Analyze 取得“证明”。

阶段产物放 run-logs/166；Git 只保留源、提案、紧凑收据及必要失败日志。源码包不混入
执行产物，证据单独交付。作者不合主树、不跑全 workspace；精确候选交非作者审核。

## 实施中确认的有限边界

原生入口继续只允许 void/i32 成功类型；`main()->E!Package` 明确拒绝。成功 Package
清理由 `main()->E!i32` 的 `?` 取得载荷后执行；拥有错误载荷由 wrapper 真正消费。

整体 owning union 重赋值在本片显式 Unsupported。实际近邻在较晚 guard 仍活时显示，
简单 Drop 后 Store 会破坏现有逆成功构造顺序；现有 Managed 私有 Package 槽的字段
替换不等价于整个普通 Value 的重建。不能仅从单变量替换成功宣称已闭合此能力。
本片仍覆盖新绑定/参数准备失败时，先清部分新值，再清既有拥有联合和其他局部责任。
