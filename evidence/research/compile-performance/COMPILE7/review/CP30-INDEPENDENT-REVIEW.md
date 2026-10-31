# CP30独立审核

审核者`/root/cp25_evidence`不是实施者。绑定提交`1b86175c8728834c20d1bb18ce13a329764e315f`，工作树干净；裁决为限定正确性**Pass**，不是性能采用或完整workspace／产品验收。

生产改动仅在`Database::read`增加`active.is_empty()`时直接读取已发布快照的分支。新分支之后的原DFS逐字节等于父提交；所有active场景仍按原路径检查直接及传递祖先、缺失输入、结果指纹和循环错误。没有修改publish、invalidate、clear、取消、缓存格式或原测试。

## 不变量理由

内部快照表是私有字段，只有publish能够插入。publish在当前key仍位于active集合时核验所有输入闭包，因此不能发布自身依赖、传递环、缺失输入或指纹不匹配的边；失败会移除active记录且不替换旧值。替换前invalidate删除所有观察了变化结果面的消费者，并传递删除其消费者，剩余图仍闭合且指纹一致。单独invalidate只删除，clear整体清空，都不能制造新悬挂边或环。

发布的Arc虽然带公开字段，调用者只能修改脱离内部表的新副本；在内部仍持有引用时，Arc::get_mut不能返回原对象的可变借用，Arc::make_mut会克隆。因此无active时，所有存留快照都已满足完整读图条件，再做DFS不会得出不同结论。active非空时此推理不足，候选正确保留了原检查。

## 实际验证

独立target使用Rust1.99.1运行`cargo test -p zil-db --offline`，11个原单元测试和2个新增集成测试通过，零失败、零忽略、零过滤；doc测试0个不计作覆盖。作者与审核者运行的是同一组13个具名测试，不能相加成26项。

原测试包含9,600个四节点DAG失效／替换组合，覆盖五类结果指纹、重复依赖、驱逐、空图和稳定Arc身份；另外覆盖active祖先、循环发布、过期输入、取消、InfraError和失败后旧值保留。新测试独立用布尔传递矩阵作为oracle，遍历64个DAG与16个active子集，检查读取、无关active、缺失active、abort／clear及外部Arc修改隔离。

精确文件SHA、命令、测试汇总和范围在`CP30-INDEPENDENT-REVIEW.json`；执行尾部原始输出在`CP30-INDEPENDENT-TEST-TAIL.log`。本审核没有运行编译性能窗口；是否采用仍由根协调者结合完整会话及CLI测量裁决。
