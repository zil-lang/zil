# 340 基础类型与模式终态候选

权威为主树 `.task/impl/340.md`、当前 `_zil_design.md` §15.2/§16。
本提案不提升 B0 或产品门，不将有限阶段交付记为终态完成。

1. 普通函数指针调用保留已经确定的完整结果类型，使未注解绑定后的
   记录式 Enum match 与同一签名的直接调用一致；局部遮蔽继续按真实绑定。
   原 `tests/language-readiness/batch336/component.zil` 不改写。
2. `T?`/`Option<T>` 使用封闭卫生身份的普通 tagged Enum（None/Some），
   复用既有判别、移动、清理与机器布局；不能与用户同名 Enum 混同。
3. `u64`/`usize` 必须保留独立类型和无符号数值语义；`usize` 以目标宽度
   核验，CT 不借用宿主 usize。公共 Type/Constant/Wire 由 root 发布。
4. 一般模式按真实 place、分支及责任处理。ref 是真实借入，不能复制拥有
   载荷或越过判别失效；失败模式不得先移动父值，通配拥有叶清理一次。

源码正例、最近反例、两核/Wire、机器 O0/O2 和适用额度测试随各阶段提交。
未实现项保留明确未完成状态；不以 Unsupported 当作能力交付。

组合消费者补充：std.iter.find返回Option<含u64的Copy Record>通过Verify后暴露Enum物理布局/store遗漏。按已定原生x64整数布局加入U64/Usize 8/8叶与真实存储，不改变Copy、FFI或线程资格；保留该普通源作为原生回归。
