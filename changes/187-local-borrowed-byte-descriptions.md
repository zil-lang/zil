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

## 实际纵切补正

同一原始最小输入依次保留Types、Analyze budget.rs、Verify及Lower的真实Unsupported。root依据这些失败及后端同构排除项扩写Analyze budget.rs、Lower lib.rs和LLVM emit.rs：只去除Desc<ByteBuffer>的旧额外捕获限制，保留真正已实现的类型形状、Store排除、Slice/owned-error门和有限Copy签名。资源/窗口/私有来源独立求证、计费与布局均不改。原先“预计无需Lower/LLVM变更”的判断由实际失败修正，不能把到Verify成功当机器执行完成。

实际机器构建又暴露同一emit.rs中capture_type与Closure字段alignment的旧Managed<ByteBuffer>单项限制。这两处同步接受已核Desc<ByteBuffer>，调用既有llvm_type(ptr)及alignment(8)，不新增表示、堆盒、引用计数或运行时映射。

impl099原两项局部弱描述捕获输入属于新能力，需同源迁移为Success；root与184的相邻lending旧例统一迁移，本片不编辑该共享测试文件。

## 内部窗口与实参义务补全

实际check探针发现，闭包内部先从read描述形成view、再经edit描述reserve时，原私有Apart只记录普通Parameter/Parameter，漏掉Capture/Capture与ParameterCapture。两个捕获来自同一Buffer的输入错误Success；异源也Success。该不安全机器未执行，不能以先前外层view反例通过判定能力已闭合。

root据此授权Verify borrows.rs及borrows/{buffer_effects,captures,loans}.rs：分离义务只记录真实外部描述/地址来源，保留普通参数、当前闭包捕获和来参环境捕获的身份；在每条真实普通/闭包调用边逐槽替换并独立拒绝实际相交。拥有捕获保持环境内部责任，环境位置不与其外部目标混为一体。公共Hold/Call/writes及schema23不变，所有遍历与闭包都计入原预算。

普通helper额外Desc<ByteBuffer>参数先前也被Types/helpers.rs有限Copy门拒绝。其开放以同一私有义务可表达capture/argument关系为条件；普通R描述、Store、显式Address callback参数仍保持原边界。callee实际Effects同时保留其普通显式参数与真实被调用捕获，不再因看到callable_contract就只导入callback效果。原Types拒绝、内部窗口漏拒、编译环境/测试格式失败均保留，不更改原正例与同源反例目标。

内部Capture/Capture修复后，同源反例、异源机器、raw/Wire与预算已实际通过；额外ByteBuffer参数随后在Analyze/helpers.rs暴露同构旧门，Verify/helpers.rs也独立保有该门。root授权两处仅追加相同Desc<ByteBuffer>形状，保留普通R和Address额外参数旧边界，不改变已有trace/公有writes含义。
