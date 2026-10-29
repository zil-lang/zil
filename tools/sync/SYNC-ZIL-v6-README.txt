ZIL 一键同步脚本 v6

一、普通使用
把 SYNC-ZIL-v6.ps1 与 .bundle 放在同一个目录，运行：

  .\SYNC-ZIL-v6.ps1

仍然先找当前目录，再找脚本目录；兼容包含一个 .bundle 的 ZIP。
多个候选包会列出，Enter 选择最近修改的包。
逐分支提示保留；Enter 同名同步，输入新名字改目标，输入 - 跳过。
main 现在默认 main -> main，不再要求 -AllowMain。

二、sandbox 已合并 main，只推 main

  .\SYNC-ZIL-v6.ps1 -MainOnly

只有一个候选包且需要完全无交互时：

  .\SYNC-ZIL-v6.ps1 -MainOnly -NonInteractive

多个候选包无交互时须指定：

  .\SYNC-ZIL-v6.ps1 -Bundle .\zil-main.bundle -MainOnly -NonInteractive

MainOnly 必须在推送之前证明：Bundle 内每个其他分支头都已包含在该 main 的提交历史中。
若某个分支尚未合并，脚本失败退出且不会推送；不能仅凭 Bundle 里存在 main 就忽略其他工作。
该证明针对本次 Bundle 中的 heads，不代表未纳入 Bundle 的远端工作也已合并。
该脚本仅创建/快进，远端 main 已更新而发生分叉时拒绝覆盖，需重新在 sandbox 合并冲突后导出。

三、兼容选项

  -DryRun       预览结果，不推送。
  -SkipMain     跳过 main，保留旧默认策略。
  -AllowMain    兼容旧命令，v6 已无须使用。

成功或已包含返回 0；分叉/推送失败/回读失败返回 2；配置/包校验/前置检查错误返回 1。
结果会显示 UPDATED、CREATED、ALREADY、CONTAINED、SKIP 或具体失败原因。

四、验证与来源

基于实际 SYNC-ZIL-v5.ps1（18,789 bytes）增量修改，保留其自动发现、ZIP、分支交互、缓存、网络超时和 SHA 回读逻辑。
v5 原文件 SHA256：585c91a1c3c5de9ae4bdb5ddfec4db9acd25b1b5a1699ecef1da2abaf6f9bfcc
已在 Linux / PowerShell 7.6.9 / Git 2.51.1 执行真实脚本、真实本地 bare remote 的 18 组测试。
覆盖：main 默认快进与重复、远端包含、分叉拒绝、DryRun、MainOnly 已包含/未合并/缺main、
多分支默认与改名、SkipMain/AllowMain、重复目标拒绝、多包选择、ZIP、脚本目录回退、增量包、
服务端拒绝和交互跳过。全部通过；未执行原生 Windows PowerShell 5.1 测试。
上述18组脚本测试仅访问本地 bare remote；本轮 GitHub 主线发布与分支清理由独立的受测发布流程执行，结果另留收据。

可复现测试：安装 PowerShell 7 和 Git 后，将 test_sync.py 放在脚本旁，运行 python3 test_sync.py。
若 pwsh 不在 PATH，可通过环境变量 ZIL_TEST_PWSH 指定其可执行路径。
