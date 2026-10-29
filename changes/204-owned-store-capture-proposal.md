# 204 有限叶 Store 拥有捕获提案与实施合同

基线为已验收203 `ed3f797561ae5f2ea4b53b76d96df47778c5a43d`。当前语义由设计§9.1.1及interfaces.owned_store_capture204给出；本提案不替代规范，也不把登记计作执行。

203后续只读预检中，完整Bag返回可到Verify，直接Store、完整Store-Package及FnMut写入环境均在Types关闭。四份输入与CLI03原身份保留于evidence/impl/203/next-preflight。历史审计开头的显式move/FnOnce待定只是当时建议；本片已选择既有§9.1实际拥有推断及Fn/FnMut/FnOnce，Copy/Unit结果，取出后由内部局部清理。

## 写集与分工

Types作者修改closures.rs、closures/package_effects.rs、lib.rs与dynamic_store.rs，保持完整实际类型、自动拥有和真实模式，不用目的地反推捕获。Analyze作者独立修改dynamic_store/callable_fields及必要budget/lib入口；不得调用Types或Verify求证函数。root持Verify closures/dynamic_store及LLVM emit/resource_closures的精确Store清理。既有schema27/Wire27复用，无新公开字段。第三方审核不得改生产。

新callback可以进入现有State/外层Store，但内捕Item只限旧叶子Copy/直接Managed；不能复用已开放callback的通用StoreElement谓词作为内层资格。允许已有直接Closure子环境，Copy结果约束沿实际拥有子图传递；不可全局关闭旧lending或给旧无Store路径追加扫描费。完整Store头是24B并不证明Item有效，未用环境/仅资源定义也需独立核。

## 必需验证

原四源逐字控制；显式move与正文give；完整Package自动拥有；同Item两个实际Store的不同值/marker/责任；FnMut重复调用与FnOnce未调用/取出前/取出后/参数准备失败/增长失败；Package只取Store而旁置字段仍活；活动元素窗口与真实Store父根；State/outer Store组成、pop后消费、外层增长/失败；原真实TypedIR派生raw/Wire的错实例、重复/伪造捕获、坏未用元数据、bad second root、隐藏callback/Store和循环；新N/N−1/fresh及旧精确预算。Linux与Win64/Wine实际执行单列，原生Windows仍NotRun。

旧impl172_owned_store.rs、impl174_store_fields.rs与impl201_analyze_store.rs精确登记用于旧断言迁移：先保留实际失败及原源身份，只晋升本合同明确开放的具名输入，其余攻击保持。只改捕获类型未改MakeClosure真实输入的raw攻击不成为合法程序。闭包数组旧例按实际后续完整组合判断，不批量修改为Pass。

## 预算、交付与终态

新增扫描、分配、完整类型复制、队列和私有结果profile传播先计费；默认额度、原product-gates字节与独立编译速度支线均不改。root独占正式全仓与集成，作者仅窄测。源码Git不新增可执行文件/缓存/大执行证据，标准bundle与独立小证据交付。完整1.4仍NotSatisfied、B0 Doing、产品门NotRun；本片不是全部ZUI前置完成。
