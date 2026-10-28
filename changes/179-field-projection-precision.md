# 179：回调字段的访问足迹与父壳存活

状态：实施提案，基于 `8c4ed992138fc103c0fbaf6318e2e34d261bda81`。
规范：1.4 / Ante-R1，`_zil_design.md` SHA256
`fe11dddb8fd64aebd773e90dea5ceaa545da3e41e280fd939806c4dc0ea212e2`。
依据：§3.4、§4.1–4.4、§5.3、§8.1–8.2、§9.7；V02/V05/V06/V12/V13。

## 为什么

177 的 `BorrowPackageField` 已直接借用真实具体闭包字段，且 Types/Verify
要求环境自包含；借用来源却提升为整个 State 局部。因而借 callback 时，
同一父壳的独立 Store 操作和 Copy 字段更新也被拒绝。父壳必须活到窗口
结束与窗口独占该字段是不同义务，不能因它们共用一个局部编号而混同。

## 改什么

Analyze 与 Verify 各自从原始 `local + index` 和已核 Package 声明重建字段
来源。具体 callback 地址使用字段根；既有 ByteBuffer/Store 字段根保持。
Copy 字段更新和具体 callback 的取走、替换、清理使用对应字段足迹。
根中的 LocalId 继续参与父壳存活检查；整体 move/drop/StorageDead 仍用
整个 Local 根并与所有子字段相交。公开 HoldRoot 仍是保守的 Local 上界，
候选不能自填字段不交事实，也不增加新 Wire 权限。

保持已核具体自包含闭包的准入；此片不开放借入环境进入 State、不实现
任意混合捕获逃逸，不把不同参数名、payload 类型或字段拼写当 apart。
未知用户资源的 cleanup 足迹仍保持整壳保守规则。字段精度只来自实际
Package 字段位置及现有已核布局/操作合同，不改资源实例身份规则。

## 怎么验

从真实源码到独立 Verify 与 O0/O2 native：同父 State callback 借入期间
操作 Store、ByteBuffer 或 Copy 兄弟字段；调用状态保留；字段替换、提前
退出、整个父壳最终清理准确。Store 窗口与 callback 在当前调用和后续使用
中并存，不通过提前结束窗口或拆成两个 State 绕过问题。

最近反例保持拒绝：同 callback 字段 take/replace、父壳 move/drop、重复
独占或独占/共享并存、同源 Store helper 隐藏写、旧字段窗口复活、借入
回调逃逸、伪造字段索引/类型及删改候选 Hold。raw 与 Wire 各自进入 Verify，
攻击者允许重算指纹，不能只靠旧 hash 拒绝。限定回归覆盖 120/166/174–177
邻近能力，逻辑预算不放宽，未执行及明确未支持与 Pass 分开记录。

## 不变的目标状态

不引入运行时借用表、GC、RC、世代、锁或隐式盒；Verify 不依赖 Types/Analyze
证明实现。此片只关闭当前字段精度义务，不登记完整语言、产品性能或全体
高阶/平台生命周期完成。正式工单、公共 schema、完整验收由集成者管理。
