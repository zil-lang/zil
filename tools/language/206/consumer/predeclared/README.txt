206 五模块完整拥有 child Package 消费者草稿
状态：DraftNotRun。这里只生成源码和独立预声明模型，未运行 ZIL 编译器、Verify、LLVM 或机器程序。
正式206规范和写集登记后，才可把选定字节晋升到仓库的新206测试/工具路径。遇到夹具错误须保留本原始版本，不回写或把新oracle说成最初预期。

1. 精确范围与模块

portable.zil：现有 Marker/Drop、putchar/getchar；E 表示第三个Copy参数失败。
leaf.zil：现有平坦 Leaf，拥有Marker和ByteBuffer，另含u8/i64/u16数据；make只用既有void!Leaf失败载荷。
product.zil：Middle按声明序拥有 I、Leaf A、J、Leaf B、K；Root仅有完整Middle child，自己无直接manages字段。
transitions.zil：Root/Middle/Leaf全值传递、取出child、在独立Leaf局部中take Mark并补X、再完整嵌回；checked取出后检查实际数据/缓冲区，再补全所有child。
main.zil：读取一个0–7字节，组合以上能力及真实?退出。

所有新嵌套值均为普通完整Value。没有E!Middle/E!Root、Store<Middle/Root>、捕获Middle/Root或其数组。没有从父对象直接取得深层字段地址。只通过直接whole-child take得到独立局部，部分移动只存在于独立局部。现有void!Leaf仅复用平坦Package的已支持错误载荷，不属于新Sum资格。

2. 预声明正常结果

模式  名称                                   stdout           退出  分配调用  peak live/payload
0     三层完整声明序清理                     PIAJBK           0     2        2 / 24 B
1     独立Leaf部分移动Marker后补X             tAPIXJBK         0     2        2 / 24 B
2     完整准备C/D后替换Root.child             IAJBKPLCMDN      0     4        4 / 80 B
3     C/D准备后第三个Copy参数失败             EDCIAJBK         1     4        4 / 80 B
4     whole child取出后提前返回               QIAJBK           0     2        2 / 24 B
5     独立Middle再取出左Leaf后提前返回        RAIJBK           0     2        2 / 24 B
6     (root)=relay(root)完整转交并赋回          HPIAJBK          0     2        2 / 24 B
7     child取出后在独立Middle替换左Leaf        APICJBK          0     3        3 / 48 B

P=全部正常数据检查结束；t=Leaf旧Marker已取走、X已补入；H=普通Root relay；Q/R=两种提前返回点；E=Copy参数主动失败；F=实际bytes_reserve失败；!=数据/layout检查不符，任何!均不在预期中。
大小写Marker属于不同真实责任。模式1的t之后A是独立旧Marker的析构，原A的ByteBuffer仍随补X的Leaf存活。模式2必须先准备完整新Middle，再出现旧IAJBK析构，最后P与新LCMDN析构。模式3尚未调用middle()，因此L/M/N尚未创建；准备好的D、C按逆序清理，然后旧Root仍按IAJBK完整清理。

3. 真实分配故障预案

每个Leaf工厂先创建Marker，再bytes_new，随后只做一次bytes_reserve；容量为A8/B16/C24/D32。保留成功reserve后的单字节push、len/capacity数据核对。资源Marker、Root/Middle/Leaf载荷自身均应内联，无隐藏堆分配。
Linux复用tools/language/173/allocator_probe.c，对实际生成LLVM的对应外部allocator调用做既有符号重定向；Win64/Wine复用tools/language/181/heap_probe.c。普通未改写可执行程序与独立instrumented程序分开，不能只运行模型。
每个模式每个O0/O2：普通一次；instrumented fail_at=0；再从1至该模式正常alloc_calls逐点失败；最后fail_at=N+1证明没有额外分配。所有执行比较真实stdout、退出值、alloc_calls/allocated/frees/failures/live/peak_live/peak_bytes；Windows额外bad_operations必须为0。

所有有效模式的第1次实际失败为FA：A的reserve失败，Marker A清理；第2次为FBA：B先失败并清理，再清理已准备A。
模式2/3/7的第3次为FCIAJBK：C失败，旧拥有树完整清理。
模式2/3的第4次为FDCIAJBK：D失败，准备C清理，旧树完整清理。
第1–4次失败的峰值payload分别为0/8/24/48字节；失败请求本身不算已分配。
模式3的普通失败退出来自Copy错误而不是allocation失败，不能算进真实allocator失败次数。

模型ORACLE.json提前列出全部预期。每平台计划20普通+74instrumented=94次执行，其中42次实际allocation失败；Linux与Wine分别记录，不相加冒充一个平台成绩。原生Windows仍NotRun。这里的payload峰值不是RSS或栈大小。

4. 预期内联布局与实际检查

按当前x64自然布局预声明（最终需从实际LLVM/机器核对）：Marker4/align4；Leaf48/align8；Middle128/align8；Root128/align8。Leaf字段lead0、mark1、wide2、bytes3、tail4。Middle字段before0、left1、between2、right3、after4、stamp5；Root.child0。
Root的清理GEP叶路径应按以下声明序：0/0 Marker I，0/1/1 Marker A，0/1/3 ByteBuffer A，0/2 Marker J，0/3/1 Marker B，0/3/3 ByteBuffer B，0/4 Marker K。
嵌入child携带完整payload，不携带内部partial flags。Root局部若发生child操作，仅需一个直接child field live flag；独立Middle可有其5个直接拥有字段flag；独立Leaf可有Marker/ByteBuffer两个flag。LLVM whole/local flag必须先退休，再调用任意用户Drop；child被take后旧父flag不能复活。
wide/stamp使用超过i32范围的真实i64数据，另含u8/u16和ByteBuffer头；checked逐层取出完整child、检查实际数据并补回，覆盖嵌套padding/调用返回/重写，而非仅运行析构marker。

5. 源负例与独立核

negative/包含12个未执行源，NEGATIVE-MANIFEST.json记录每份字节/SHA和语义义务：重复child、partial child/leaf嵌入、同布局不同名义、Incoming经嵌套take假装Create、旧Desc跨整体搬运，以及新Store/capture/array/sum/child-borrow/deep-lvalue闭边界。尚未猜测具体诊断阶段或做泛化阶段白名单；206登记与实现后按真正承诺固定精确诊断。安全负例禁止native。
controls/保留原120字节package-direct-nested输入（SHA8f82dcba19831acf7417ee2fd4c852b8086d2dc377c8fc470131f87a603d2dba）、parent-only-child和同函数字面构造的Created叶经take仍为Created控制。RAW-OBLIGATIONS.json列出从实际接受源码派生的raw/Wire最近攻击；手填IR不能替代源贯通。

6. Lower/LLVM实施定位（尚未改生产）

建议最小生产写集：crates/zil-lower/src/lib.rs；crates/zil-backend-llvm/src/emit.rs；crates/zil-backend-llvm/src/emit/packages.rs。
保留旧package_type / packages::supported_type专供已有capture/error/array资格；新增普通完整Product局部/SSA/参数/返回的递归predicate。Lower及LLVM函数直接值入口调用新predicate，旧capture/error_type/owned_array_type以及LLVM errors.rs/arrays.rs继续调用旧predicate。
在packages::owned_field中把已核的child Package当一个直接拥有字段。现有package_flags、take_package_field、store_package_field、drop_package_field、drop_package和teardown_paths可沿这一真实边界复用；teardown_paths已有Package递归分支，目前父owned_field跳过直接Package，需补这一入口。
现有llvm_type/aggregate_field_type/alignment、MakePackage的insertvalue可表达递归内联载荷。BorrowPackageField仍限Copy/Closure；DescribePackageField仍只Managed。禁止借此打开深层AddressField、capture/Store/array/Sum的新资格。
resource_closures::flag_count/drop_functions/package_capture_storage仍只处理直接Managed；dynamic_store::runtime元素Drop仍只Managed/Closure，因此必须保证新普通Product不能误入这些ABI。无需为本片修改这些旧机器协议。
Verify须在允许生成嵌套cleanup前预费完整shape/layout与叶路径深度工作；teardown_paths的path.clone及GEP输出不能只按一个父child位计费。前端与Analyze/Verify独立证明嵌入child完整和Created/Incoming叶来源，LLVM flags=true不是证明。

本草稿未改变主树规范、工单、生产文件或已冻结205证据。完整1.4 NotSatisfied、B0 Doing、产品和原生Windows NotRun保持。
