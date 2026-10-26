# 024 集成：快照一致性与统一工具构建

第二轮统一门 `20261023T055828Z-4f09b7ce` 为30/31。native-roam-tests的八个队列测试正确拒绝摘要与实际字节不同的快照；两种摘要对应Cargo的debug和临时strip产物，替换路径的外部机制未被本次证据确定。失败原件/摘要/产物映射保留在`evidence/impl/024/failure.json`与指定raw目录。

根集成者把native-roam-build由单独追加rustc strip参数改为普通cargo build，采用024工具Cargo中一致的dev/test debug=0。原有28测试全部保留，最低数量提高到29，且要求真实执行`host::tests::snapshot_uses_open_handle_when_source_path_is_replaced`。这项控制打开A后替换路径为B，验证快照的字节/摘要/权限仍来自同一已打开对象，之后篡改仍被拒绝。

生产freeze由024作者实施并独立审查；根改动不调整语言、队列格式、断言或验收标准。单次读取消除hash与copy重开路径的窗口，但不声称并发原地改写的文件提供原子版本，也不追认未密封环境为完全可重放。
