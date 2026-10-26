# 087：ZUI 首窗所需的有限静态 Win64 外部边界

2026-10-25。此提案先于实现。085负责前端与分析，086负责独立安全核，087单写公共schema、CT、Lower/LLVM、目标构建与纯ZIL平台消费者。父任务B0和完整产品门保持原状态与原阈值。

## 目的和拒绝边界

沿用用户确定的Win32路线；窗口、消息、绘制的调用点必须在ZIL源码中。不得添加手写C窗口宿主、命令回放器或隐式全局对象表。目标是正常注册WndProc的窗口，不用系统STATIC控件冒充输入/状态/绘制闭环。若本机只能运行Wine，证据只称Win64 PE在Wine兼容层执行，不称原生Windows验证。

## 值、构造与权限

新增普通整数u16，取值0..65535、大小/对齐2字节。checked/wrapping/除余/位移/显式保值转换与u8规则相同，位宽改为16；无隐式宽化。新增u16转换保留名。它使RegisterClassExW的ATOM真正使用16位返回ABI，而非伪报i32。

新增Copy值c_handle，对应Type::ForeignHandle；c_null()产生Constant::NullHandle。机器表示为目标宽度的不透明ptr，可作外部值参数/结果、Copy字段、相等/不等比较；不能解引用、算术、整数转换或转成ZIL Address/Desc。可以借用存放handle的普通槽。该值不证明OS对象存活或唯一释放，拥有责任仍由显式Managed包装/unsafe工厂承担。

新增Copy值c_wstr，对应Type::CWideString；c_wstr("字面量")产生Constant::WideString(Vec<u16>)，向量含且只含末尾NUL，内容必须合法UTF16；c_wstr_null()产生Constant::NullWideString。字符串编译为程序生命周期的只读UTF16全局，允许C留存该静态值，不允许写入；空字符串与NULL区分。禁止非字面量、内嵌NUL、孤立代理项、从整数/handle/Address转换、外部返回和外部可变写入制造该类型。编码单元按实际数量计入raw/wire/Verify预检预算，不修改默认限额。

新增Type::ForeignCallback { parameters, result }，源面extern "C" fn(...) -> T；c_callback(静态普通函数符号)生成InstructionKind::ForeignFunctionRef { function }。它与普通FunctionPointer分离，不能普通CallIndirect。独立核验目标必须是实际定义、无环境、非foreign、非资源teardown、非closure入口的普通函数；签名完全相等，参数只含i32/i64/c_handle，结果只含这些或Unit。不得捕获、闭包转码、传ZIL地址/拥有值或由外部返回/可变写入制造回调。后端必须发出该目标真实C ABI地址，无隐藏参数。静态代码地址可被C留存；没有附带数据环境。c_*构造自身无外部效果，真正外部调用仍需词法unsafe。

CoreExprKind::Literal(Constant)承接三个静态外部常量，仍是不可信Core。c_callback保留Core Call并由Types内建解析。外部值/回调不进入本片CT执行；u16正常进入两个独立核验后的CT入口。raw CT明确拒绝新外部常量及嵌套类型。

## C声明、布局与重入

ForeignDeclaration新增static_callbacks: bool，源码provides(static_callbacks()); effects(ffi);显式启用。true覆盖传入/留存静态回调以及本次调用反调此前注册的回调；不能只按参数判断GetMessageW/DispatchMessageW等是否会反调。false保留原无回调/重入合同。直接或共享记录含回调的外部参数必须true。

外部直接值参数支持u8/u16/i32/i64/c_handle/c_wstr/受限ForeignCallback；结果只支持Unit/u8/u16/i32/i64/c_handle。窄量按真实C目标ABI发出，不能用宽量声明猜测返回寄存器。Address参数仍须noescape完整唯一列出，最多一层。共享载荷允许u8/u16/i32/i64/handle、静态字符串、静态回调及递归非空数组/非空Record；可变载荷只允许全部位模式有效的u8/u16/i32/i64/handle和数组/Record。拒绝Bool/Unit/枚举/资源/Package/普通函数指针/Address嵌套及零长聚合。静态值的可保留性不放宽record借入地址本身的noescape。

本片仅在受支持的64位小端Linux SysV和Win64 MSVC目标保证上述严格Record子集按声明顺序、自然对齐和尾部补齐的C布局。只按地址借入，不开按值聚合ABI、packed、union、bitfield或变参。padding不是ZIL字段数据；外部只可读取已初始化声明字段，可写padding但不得越过完整对象。原借用核仍独立检查活性、初始化、来源和同时访问；不加入noalias/nocapture/dereferenceable。static_callbacks允许的回调不得访问本次借出的ZIL存储；外部函数本身仍可按声明读写它。外部实现不得跨边界unwind/longjmp/挂起，也不得保留任何借出地址或将其交给回调。声明真实性、OS资源和实际长度关系仍属于显式unsafe边界。

## 目标和迁移

新增x86_64-pc-windows-msvc、64bit/little/llvm/checked的完整TargetProfile；Lower/Emit/Build必须检查全元组。PE入口直接调用ZIL入口后ExitProcess；导入库用显式DLL符号声明及llvm-dlltool生成，lld-link负责链接，不新增手写C宿主。memcpy/memset与__chkstk等实际产生的依赖必须合法解析，禁止关闭栈探测掩盖链接问题。Windows构建配置、目标、命令和产物摘要进入证据。

schema/wire14→15，Typed指纹域V15。新增Type tags19=u16、20=handle、21=wide string、22=foreign callback；Constant tags5=u16、6=null handle、7=null wide、8=wide units；Typed opcode60=ForeignFunctionRef；foreign元数据追加static_callbacks布尔。旧wire0–14及未来版本拒绝。084继续冻结schema14/f95历史性能身份，集成只搬移私有DB优化后另跑schema15功能门，绝不重标旧计时。

## 必需证据

源/CT/raw/wire负例覆盖伪造静态来源、闭包/foreign/teardown回调、错误签名、环境泄漏、普通间接调用、可变C静态指针写入、noescape与重入合同、窄量越界、非法UTF16/过量/截断。验证布局和窄返回的独立C ABI测试仅作测试oracle，不是产品UI宿主。原生Linux回归与Win64机器行为覆盖Unicode、初始化/调用失败、正常消息输入、状态变化、实际绘制和关闭清理；O0/O2与目标混淆负例绑定同一源码。正式全量、独审和可恢复交付分别记录，有限首窗不冒充完整ZUI/GPUI替代品或已完成内存管理突破。
