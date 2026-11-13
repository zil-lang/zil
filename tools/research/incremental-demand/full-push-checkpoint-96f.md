# 诊断实际消费者检查点：96f

编译器冻结源码 `96f5053f59ddb50871eaabbf5005a46ff782dcda`，release19 SHA256 `c5334425287f32770f4841ed4a9294c5369d24f4e52fe92d77a441d4e7c0c3c1`，SDK16不变。默认研究开关关闭，未合 main；本组件未新测性能，最新测量仍绑定 [4b809](full-push-checkpoint-4b809.md)，正式 Rust 六轮仍绑定 [328](full-push-matched-328.md)。

实际机器 Item 仍完整比较，同位置相同结果保留 item/文本片段的持久根，不再每轮为429项分配名称、重建整张名字表与聚合片段数组。仅增删重排才懒建借用名字索引，仍由完整 Item Eq 授权，重复名只会保守重绘。完整 `.ll` 仍无条件写入 staging，当前验证指纹仍替换；64个 IoSlice 的固定窗口直接写 retained payload，删除1MiB中间payload缓冲与全部字节的额外用户态复制，短写、中断、零写及flush错误都保留。逐 Item 遍历和真正内核写出仍存在，不能把此前21ms诊断scope全算作收益。

整块冻结后统一门已完成：backend249、driver210（0失败/忽略）、strictClippy20、fmt18、release19通过；[确切源码/命令/日志](full-push-checkpoint-96f.json)、[源码独审](full-push-diagnostic-consumer-review.json)、[独立门核验](full-push-diagnostic-consumer-gates-review.json)。新断言检查实际旧片段地址被writer消费、完整fresh文本一致、历史根不变/单叶变化、重排/增删/重复名，以及跨64片窗口与所有写出错误。没有逐行反复构建，没有累加重叠测试。

初控制器在driver运行中丢失工具连接，实际进程继续；完成的249/210子进程原日志/exit0被独立核实，原控制器exit保留unknown。之后仅接跑未执行的strict/fmt/release，新的持久控制器及工具实际exit0；不重跑已完成测试，也不把unknown改成0。首次事件和所有原日志已由下述v22固定远端metadata保全。

此前profile15全部495files、SDK16完整156regularfiles与alias、源码及immutable17/18、所有首败/成功门和独审已在 [固定v21](full-push-recovery-v21.json) `902ed10db8cc7fe634de585fd3dabf4b7fd75096` 保全：8存储片+索引fresh HTTPS逐文件hash/模式，profile/SDK原manifest精确并集核验。SDK和immutable原位置保留，没有声称整份重新拼回后再运行qualifier/native。当前96门、不可变binary、完整源码及精确8f文档也已在 [固定v22](full-push-recovery-v22.json) `98dfcab311a22dae96f6e5ffac9cc0b83ce35118` 保全；单metadata36files/43134497B与index均fresh HTTPS全hash/模式核过，原binary路径/dev/inode保留，原controller未知状态不改。

下一施工是实际机器输出组合树，避免 emit 将单函数已共享结果重新聚合为整Module后丢失身份，并让相同实际结果贯穿历史/诊断/CGU；复杂变换仍显式原路物化。LLVM另线由Astra按固定7a复核每块MI展开产品，当前函数头尾/CFI/布局不能略过，尚无实现验收或速度承诺；完整LLVM增量和全面领先Rust目标仍开放。
