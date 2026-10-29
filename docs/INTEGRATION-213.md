# 213集成检查点

目标是将语言206及编译专项212合入同一可运行主线。包括语言202已验收内容与后续203–206，保留LLVM API默认、直接Builder、显式Clang对照和对象缓存。实际测试与独立审核结果由evidence/integration/213记录，未运行项不得记Pass。

同步入口为tools/sync/SYNC-ZIL-v6.ps1。默认main同名推送；-MainOnly仅在包内其余heads均为main祖先时推主线；分叉、网络失败和远端竞争均明确失败，不强推。

语言214–216的完整Product拥有捕获与217同步组合消费者已完成限定独审和正式回归，精确收据见evidence/integration/217/RESULT.json。完整1.4及ZUI全部需求仍未完成；可开工范围与真实余项见docs/ZUI-LANGUAGE-READINESS-217.md。
