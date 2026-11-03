# 322/325 公共版本与旧精确边界迁移

首轮集成候选88fde29a6默认全仓4063通过/26失败，422组；未登记通过。包含旧Wire32夹具、已分配opcode95、旧life诊断和旧预算阈值。原始输入与历史结果保留，不提高默认额度。

322为避免unused/generic及非函数类型位置的life标注被擦除，新增完整、预付的类型位置遍历；同一剩余预算进入aliases/engine。无life函数保持原来源推导语义，但实际新增遍历并非免费，不能重置预算维护旧N。

本次在完全相同源码上实测229控制(work,nodes)：plain (589,220)→(725,277)，alias (952,233)→(1641,491)，where+alias+specialization (1651,488)→(1813,549)；名义遮蔽inline (837,188)→(922,229)、where (936,230)→(1021,270)。原N−1/fresh/冷暖与近邻均保留。原未声明life输入如今精确Expand/E-LIFE-DECLARATION拒绝；混合type/life仍Unsupported，const binder旧对照不变。

Wire34已分配95给WithScopedThread，旧三个手写opcode负控与review177将“未知95”改为实际未分配96/97/254；原指令字节、所有截断和坏布尔检查保留，新增95正控由当前wire/fingerprint与真实thread源覆盖。未允许旧Wire32进入当前生产解码器。
