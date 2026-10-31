# 225 完整Product Store拥有捕获

基线42ce8081；父任务B0。合同见_zil_design.md §9.1.3，依赖204、206、214–216、223。原因：普通Store已接受完整Product，但拥有捕获仍固定叶子Package，阻断容器状态进入保存回调。

复用现有类型/指令/布局，只增递归Product资格；Types、Analyze、Verify独立实现完整禁用类型检查与canonical核验，保持原叶子分支精确费用。不得把supported_element泛化为所有callback元素。正例覆盖child-only、多层、多同型Store、Fn/FnMut/FnOnce、外层回调保存；近邻反例覆盖隐藏callback/Store/借入、部分元素、重复Take、活窗口、伪canonical。原支持边界样本与期望迁移分列，旧失败不改标。

作者实现与限定测试可推进；没有不同身份审核者时保持待验，不自审合入，不声明完整1.4/ZUI/产品通过。
