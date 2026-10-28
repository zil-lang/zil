# 187 局部同步 ByteBuffer 描述捕获

## 原因与规范

设计1.4的弱描述与同步捕获规则允许局部闭包保存真实资源描述，通过实际Call效果判断与地址窗口的相交。182修复候选ebaddd7已把私有Desc来源与公开强Hold分开；动态ByteBuffer的来源、读写/失效与真实机器操作已有链路，但Types与Verify仍各有一处旧的借入ByteBuffer捕获硬门。

本片开放有限局部、同步、不逃逸的Desc<ByteBuffer>捕获，不允许描述制造拥有者、不为目标延寿、不隐式延长view。仍独立核验当前调用和逐捕获实际效果，不从Fn推导只读。

## 最小生产写集

- Types closures.rs：在现有原始操作、资源来源和结果形状检查下开放有限描述捕获。
- Verify closures.rs：独立去除同一旧准入限制，既有sealed类型、来源、责任、贷款、逐捕获Call及retained边界必须全部实际通过。

预计不需要新增公共schema、IR、runtime、Lower或LLVM变更；旧views与拥有字节环境已经有机器表示。如果实际源码或raw/Wire暴露现有182/资源检查不足，先提交最小失败，请root扩大对应写集，再修真实关系；不提前改通用规则。

## 验证约束

同源共享view活跃时，reserve、push、truncate及view_mut必须拒绝，reserve即使容量已够仍遵守真实写合同。清空沿现有bytes_truncate(buffer,0)，包括两层普通clear helper的隐藏写。共享只读描述和共享view可组合；不同buffer窗口不得误拒。独占view还须排除相交读。最后一次调用后释放环境及其实际借用，之后合法变更/给出可执行。

捕获和ReadCapture保留真实资源实例：先give/drop后未来调用拒绝，旧别名不能复活，未调用环境的清理不制造目标所有权或延寿。保留共享/独占Slice捕获、显式Address callback参数、逃逸工厂/retained、拥有环境lending等原边界。最近反例保持真实诊断，不以笼统拒绝代替关系核验。

四模块源码通过普通helper、运行时输入和实际ByteBuffer读写形成消费者；O0/O2运行输出、退出及真实分配/增长失败清理均有预先固定oracle。raw与Wire从可验源码候选变异，独立挑战实际来源/效果/Hold/所有权/逃逸。默认预算不改，限定验证覆盖新片与182/动态存储/窗口邻接，正式整仓仍由root执行。

## 身份

基线ebaddd7faf7b83b257b873e8544af0fef13e670a；1.4、schema23；design 8df5f5bb3cb25fdea8670124b09941713529c859ab18864d0fc87090864786d5，norm1068cda3fe79b906f49eeee4bd3f62863f9db4ae85e8a0dbaa3c439d310777b8。工单只读main/.task/impl/187.md。与184的closures.rs交集仅此ByteBuffer局部准入，串行合流并各自独审。
