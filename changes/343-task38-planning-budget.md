# Task38桥接规划使用实际剩余额度

修正独审发现的两个新消费者计费接缝，不增默认限额，不增加权限或公共schema。

Task canonical definition、create结果与poll实际T不能先用default Limits遍历整棵
类型再扣调用者余额。采用340同一聚焦helper形状：规划预算由实际剩余/copies限制，
失败也扣除已经访问的前缀。TaskCreate只复制预付过的实际参数/结果投影；读取
Task head时先借实际local类型，避免复制后才预付。新Solver Task路径也沿同一余额。

标准模块prelude的occupied-name索引按key字节数乘保守有界树查询次数预付insert及
lookup；加法不能覆盖长公共前缀比较。固定生成声明与变长用户名称扫描分别先预付。

控制：同一深拥有结果在足量全局限制下触发depth，在耗尽调用者余额时先触发work；
失败不退还已访问前缀。定义/result各有N/N−1/fresh，原源长公共前缀namespace有
短/长增长邻居及各自N/N−1/fresh。此处是规划单位验证，不冒充拥有Task runtime。
