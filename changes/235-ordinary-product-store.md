# 235 普通Product拥有Store字段

用户授权补齐ZUI语言核心前置，按规范§8.1.3实施普通完整Product直接/递归拥有Store字段。旧平坦State已有直接Store，新增的是含Store child的普通Product组合；复用实际Package/Managed<OwnedStore>、字段与前缀责任。

不得直接放宽被capture/StoreElement/shared/array/Sum共用的旧Product gate。普通拥有资格单列，各核分别检查canonical定义、来源、窗口、完整性与声明序清理。新Product不自动获额外位置资格。

最近正例为Session{SubscriptionSet,NodeSet}，两child各拥有实际Store；工厂/参数/返回、take/refill/replace及失败保持。反例覆盖缺child返回、双责任、活窗口相交失效、同型Store来源混同和越界资格。原生O0/O2检查无用户teardown的内建Store清理及逐分配点失败；source/raw/Wire与N/N−1/fresh分列。默认预算/产品门不变，本片无专用新opcode，随共同227接口发布Wire28。
