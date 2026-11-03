# 325/328 公共结构化线程合同

集成方冻结WithScopedThread { worker:FunctionId, argument:ValueId, body:ValueId }，opcode95，结果(created,worker_result,body_result)。schema/Wire34和指纹域34包含前一life33合同；旧33/32及更早版本拒绝，新未知opcode拒绝。无新Type或可信权限bool；执行资格必须由两个核从实际worker可达静态图独立重建，不能只看头或名称。

依据§11.1，Linuxx64适配真实pthread_create/join与稳定context、失败body清理和join失败abort；body留父线程，禁止隐式同步fallback。325实现语言/语义/Lower/CT消费者，328独立实现backend/OS/机器依赖，双方源码与近邻/raw/Wire/两后端须共同验收。登记节点不表示支持完整async/SABI/Mutex。

保留此前schema32独立指纹参考原字节；新增schema34参考并复用旧31锚点。没有新增Cargo测试目标。

合同文字校准：body留父线程且不需Transfer，允许既有合法父线程捕获/借用；删除最初规范草稿误写的“自含”要求。子线程仍只接收i32，worker白名单不放宽。
