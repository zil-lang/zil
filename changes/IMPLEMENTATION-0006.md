# 006 原生Roam路径核

## 问题与决策

实现 zil-compilation-session/1 Rust 原生开发工具 profile。保持 roam/1 case、roam-worker/1 有界JSONL协议、六态、before持久化ACK、逐步计数、清理、同property缩减和原case不变；确定随机/BFS路径与TS对拍。独立tools工作区允许锁定serde_json，不增加生产crates依赖。默认原始产物写out/runs，精简可审核收据进入evidence。完整FileStore多worker租赁和Bun冻结API不在本片，必须明确范围。

## 可检验边界

84正常路径/1311动作同集；16 seed逐步对拍；注入8步缩减及clean控制；跨语言worker/supervisor互操作；坏帧/错ACK/缺结果/超时/预算/冻结篡改必须非Pass。性能按同路径测量，不先声称更快。

正式语言/产品完成门保持。详细派工在.task/impl/006.md。
