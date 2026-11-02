# 266–270 同线程外部回调作用域

在已验收的具体拥有闭包/公共调用基础上，按设计§12.2–12.4实现可长时间运行的 Direct 事件循环。Type/Syntax可与本片隔离施工；正式合入等待前片验收。首片使用调用者真实存储和同步结构化调用，不引入heap、RC、锁、全局登记表、公开Registration类型或新Source语法。

## 冻结源面与责任

`unsafe { c_with_callback(register_name, close_name, &mut f, body) }` 返回普通`(bool,i32)`。前两项是已解析的静态外部声明符号，不是新的外国函数一等值。f是实际具体、自含的Fn或FnMut(i32)->i32，借入真实位置；不得接受FnOnce、借入环境、Desc/Address捕获或错误名义实例。body是实际具体、可一次消费的零参数i32闭包；不得直接或经捕获/别名访问同一f，普通不相交借用仍按已有核验规则处理。四项沿源顺序求值，名字解析不产生机器值；不得重新求值f/body。

register精确C签名为`(extern "C" fn(c_handle,i32)->i64,c_handle)->c_handle`，声明`provides(static_callbacks(),scoped_callback_register());effects(ffi);`。close精确C签名为`(c_handle)->Unit`，声明`provides(static_callbacks(),scoped_callback_close());effects(ffi);`。角色是外部实现的显式unsafe承诺，不制造借用/活性/独占事实。角色声明只能由该组合器调用，不能绕开作用域作普通ForeignCall；既有无角色声明行为/费用不变。两者同线程Direct，不得跨界unwind/longjmp/挂起；不允许外部擅自取回或伪造userdata。

组合器先初始化固定栈槽context，再调用register。context保存实际f指针及Closing/active状态，地址在整个调用期间不变。register可在返回前同步反调；回调原子地（同线程、无让出窗口）检查Open且非active、登记active，才以已有FnMut路径调用f；返回后归还访问再清active。Closing或重入返回i64状态1/低32位0；成功状态0/低32位为f的i32结果原位，负数不经有符号移位溢出。

register返回null表示失败，必须已经无在途/未来回调；body不调用且按既有规则清理一次，返回(false,0)。非null才一次消费body，保存其完整i32结果，完成剩余环境清理，再置Closing并调用close。close期间反调只拒入；close返回保证同一登记的直接/排队入口均不再开始，已进入者已结束，且不再访问context；随后才释放f借入并返回(true,result)。active=0不能替代close/no-future。body只有i32正常返回；可恢复错误须在body内处理。Trap/panic按本profile固定abort策略，不展开离开公布的栈槽、不承诺abort后的cleanup；外部无法兑现no-future时须abort或不返回，绝不可返回后释放槽。

## 公共表示

runtime schema/Wire/fingerprint统一32，CT artifact schema1保持。`ForeignDeclaration.scoped_callback: Option<ScopedCallbackRole>`，enum为Register/Close；规范编码None=0/Register=1/Close=2，紧随static_callbacks，所有声明与指纹/缓存同批绑定。

TypedIR新增`WithForeignCallback { register: FunctionId, close: FunctionId, callback: ValueId, body: ValueId, region: u32 }`。callback必须是对实际具体closure的活独占Address，body为一次消费的具体closure值，result精确Tuple[Bool,I32]。region必须是已进入的实际unsafe区。LIR保持同字段/类型关系，禁止CT执行。无外部返回的值可冒充f位置或body身份。

Types/Analyze与独立Verify分别核真实F身份/自含资格、地址来源与整段排他窗口、body捕获冲突与无条件消费、两个声明真实签名/角色和unsafe区。Verify不调用Types/Analyze求证函数，不信任候选claimed owner/capture/contract。新变体的访问/调用图、normalize、wire、私有缓存、lower和两种后端同步处理；旧无角色/无opcode路径保留费用，新增工作预付并消费当前请求余额，默认额度不增。

## 并行写集与验收

公共wrapper保留两个真实角色：With的调用轨迹为`F* (epsilon | B F*)`，登记成功一次调用B，失败只清理B；F允许反复。组合流分别核每个Once角色不可重复，而非见到Once即禁止其它角色的循环，也不因存在With放开任意多角色或环。Types/Analyze与Verify分别重建该责任，不交换证明。

266 root：合同、公共IR/Wire/版本、共享注册和集成；267：Expand/Types/Analyze；268：独立Verify及raw/Wire反例；269：Lower/LLVM API与text后端；270：真实普通ZIL模块、可控C驱动、独立消费者。265解析标记为前片收尾，不新增编号测试目标；本片回归按功能放入既有模块或一个必要的foreign-scoped功能测试入口。

正例含公共wrapper/真实事件循环、两个相同ClosureId的独立f、注册同步反调、body期间多次反调、f返回负数/边界i32、body自含资源清理、close反调拒入、嵌套但不同f。最近反例含body访问/移动/捕获f、FnOnce/借入f、错误role/签名/region、raw伪造f/body及重复消费、调用者过早移动/离域。真实C驱动检查顺序与完整结果，O0/O2默认/参考分别运行；注册失败、body未执行的剩余清理、关闭后不再调用，以及准备资源的实际分配失败须live0。另用子进程核abort边界，不将非法C违反合同当成语言保证。完整异步/跨线程、可返回登记值、外部恶意任意写与原生Windows不在本profile完成声明内，仍保留后续义务。
