# 197 只读预检证据

实际编译器来源与 SHA256 见 `IDENTITY.json`，原 clean build 收据见 `compiler-build/`。
`probes01/` 保留首轮 12 位置的全部原始输入和观察；`probes02/` 是 root 批准的同位置更正轮，先运行两个控制后再运行剩余输入。每条 `command/command.json` 记录实际 argv、cwd、输入 SHA、开始/结束时间及返回码；`stdout.json` 是 CLI 原字节，`emitted/` 是实际阶段产物，未改写其中的历史绝对路径。

累计 24 次输入检查：4 Success、20 Unsupported，verification_runs=4，native=0。两轮分别 1/11 与 3/9；重复的 State<F> record 控制不是新增能力。全部 mixed State<T,F> 输入仍在 Expand 关闭；源面负例尚未触达 Borrow/Verify 的不能当成安全验证通过。首轮 package 上 Copy bound 是当前 aggregate solver 的真实边界，缺 manages 则是夹具遗漏。

`run.py` 是本次更正轮实际使用的脚本，也可以在新目录中复制 CASES.json 和原始 .zil 输入后复现选定位置。它校验被冻结 CLI 的 SHA，不构建编译器，不覆盖已有 command，不允许当前证据树超过 24 次输入检查。复现应使用独立新输出目录，不能覆盖本证据。首轮由同参数的内联 Python 顺序执行，全部实际命令收据均已保留，不声称使用了后写脚本。

全部源输入/命令/JSON/阶段文件都收录；运行文件、Cargo target、缓存和可执行 CLI 不收录。`FILES.json` 索引除自身外的全部文件；构建器原日志仅作为来源复核，不计作197构建或测试次数。

详见仓库 `changes/197-proposal.md`。本证据不替代实施、独立审查、原生消费者或正式全仓门。
