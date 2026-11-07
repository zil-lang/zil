# goal 分支入口

先读 [goal.md](goal.md)，再读 [workflow.md](workflow.md) 与所涉 [协调规则](coordination/README.md)。本分支维护全任务看板、施工与复核协调、精简记录；不合入 main，不收编语言源码、正式规范、真实测试输入或原始运行日志。

更新前核本地/远端 SHA、未提交内容与工作树占用。按任务 ID 提交最小状态增量，保留原证据绑定和未测边界；只在精确远端 main 已接受、适用独审与验收具备时标已合入。root 集中整合相交状态；普通快进推送，拒绝时先重读，禁止强推、reset、clean。推送后核远端 goal SHA 与 `goal.md` blob。

源码任务一分支一 worktree；任务分支临时 `.task/<ID>.md` 可记录写集和退出条件，交付前把长期语义归入 main 的 `spec/`、`policy/`、`validation/`，过程归 goal 或固定 Git 历史。永久文件、测试和模块以能力/不变量命名。保全只用固定 `archive/source-history`，归档与验收分开。
