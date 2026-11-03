# 339：joined child 的共享 i32 借入

权威合同为主树 `_zil_design.md` §11.2/profile339，root 冻结提交 `c4da43441`。
施工基线 `e5a4d51d844e29c4f4f5184eaff5704b1b27dd24`；独立339树及工单写集。

原始真实输入 `shared339/shared_child.zil` 对 B CLI 的结果是 Types
E-THREAD-WORKER（fn(i32)->i32 门），Verify 未执行。同址父body读取、普通静态
共享helper及join后写入必须贯通；标量thread和顺序shared调用分别为Verify正控。

不增保留名称、公共字段、opcode或Wire版本。现有WithScopedThread的实际argument
类型决定i32或共享Address<I32> lane，所有raw输入仍经当前独立两核。新机器context
为{ptr,i32}，Linux x86_64大小16/对齐8；旧{i32,i32}大小8/对齐4保持。child实际
载入指针并解引用原父对象，不把引用改为快照。helper/context私有且按实际指令生成，
既有CGU依赖、完整Typed指纹和绑定compiler身份的object action涵盖此变化。

共享窗口覆盖payload准备、完整body执行/清理直至成功join；创建失败不执行worker/
body而准确清理，join不能证明静止时abort。两核各自重建实际worker图、共享读取权限/
来源与实际payload/body捕获的同时窗口；不能借最后使用、相同ClosureId或FnMut整位
替代实际逐捕获根。嵌套writer、可变ancestor重借、拥有body清理失活和相交移动必须拒。
父共享读和不相交写应成功。worker仅增加一层shared&i32参数/局部传递和读取，静态
helper结果仍为标量，不允许地址返回/留存、可变地址、FFI/环境/资源/未知间接调用。

新增算法在现有请求余额内累计预付，记录Types/Analyze/Verify的N/N−1/fresh及旧标量
控制实际delta，不提高默认限额。不创建Cargo目标、不跑正式全量。机器观察C仅控制
真实pthread入口延迟和核对身份/清理，不充当语言线程实现。

本文件先记录合同；实际结果与范围/原失败将在完成后补记。无Mutex/Atomic、一般
Transfer/Share、异步/SABI、Rust运行/内存或编译性能结论；B0/完整语言/ZUI保持未完成。
