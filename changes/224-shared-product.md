# 224 完整Product共享借入

基线 a8e8831a9e28ac2ec83be6eac740a8f04a00af7d。承接206与214–216；本记录是施工合同，不是完成证据。

仅对§8.1.1完整普通Product开放共享&Product参数、局部和同步转借；可递归读取Copy字段及既有有限用户资源头的只读投影，不从共享地址制造可变Desc。BorrowLocal核所有直接/递归责任完整，建立真实whole-root loan；AddressField保留实际父来源和只读权限，不授予额外独占或拥有资格。活动借用期间whole move/drop及相交子字段take/replace拒绝，最后使用结束后按现有NLL释放。非相交位置精度沿现有足迹，不用类型相同推导实例相同。

普通Product共享借入资格与Store元素资格独立；Types、Analyze、Verify分别重建实际类型、来源、生命周期和布局。raw/Wire父位置伪造、未初始化child、过期窗口、共享写入/重复消费必须拒绝。借入返回/逃逸、长期捕获、mut Product和异步不开放；ByteBuffer拥有叶不使其可变Desc可从共享Product取得。布局/清理/schema/Wire27/默认额度/产品门保持。

保留206已有raw攻击原样，新增真实合法源晋升及跨函数、局部别名、嵌套投影、分支/循环、活跃借用冲突、最后使用后移动、未用定义和预算反例。O0/O2验证机器地址、资源读取和失败清理；不得仅移除Unsupported守卫。
## 正式CI原边界迁移（2026-10-30）

第二轮 `41084362447` 完整默认workspace保留2858/3/0，其中两项属于224旧断言迁移：192的原 `&state`（完整P）与 `&state.r`（有限Copy用户资源头）迁移到Verify成功，第三份 `&state.bytes` 原源码仍要求Types Unsupported；三个原输入均保持。Verify的原Package capture view插入BorrowLocal攻击继续走raw/Wire双拒，只把旧 `non-Copy type` needle迁移为真实的 `Package capture views cannot expose a local address`；后半不可变重绑定反例不动。没有开放ByteBuffer头、捕获视图取地址或新生产权限。
