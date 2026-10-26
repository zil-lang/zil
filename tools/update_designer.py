#!/usr/bin/env python3
"""Build the offline designer from the supplied UI template and current authoritative documents.
Other languages are preserved as an input snapshot, not re-researched by this builder.
"""
from pathlib import Path
import json,re,hashlib,copy
ROOT=Path(__file__).resolve().parents[1]
TEMPLATE=ROOT/'ui/designer-shell.html'
text=TEMPLATE.read_text(encoding='utf-8')
def block(id):return json.loads(re.search(r'<script id="'+id+r'" type="application/json">(.*?)</script>',text,re.S).group(1))
d=json.loads((ROOT/'data/original-atlas.json').read_text()); original=copy.deepcopy(d)
# New language-wide variables are not required: the original 28 still span the current decisions.
def option(vid,value,label,meaning,cost):
 v=next(x for x in d['variables'] if x['id']==vid)
 if not any(o['value']==value for o in v['options']):v['options'].append(dict(value=value,label=label,meaning=meaning,cost=cost))
option('V02','description-copy','描述复制、责任不复制','复制对象入口；管理责任仍由原位置、包或管理器准确持有。','后续读写与关闭仍需权限、有效事实和准确管理路由。')
option('V05','accurate-carrier','准确责任载体','真实责任由控制流位置、完整包或明确管理协议承担，不与每个描述变量绑定。','条件消费可能需要状态，跨管理接口可能需要实际记录关联。')
option('V21','ordered-effects','有序操作合同','将先失效后使用的关系和全部可执行前缀纳入公开合同。','关系可能平方增长，公开合同变化影响相关消费者。')
newrules=[
('R37','操作顺序决定同源调用条件','合同与编译','同样的读和关闭集合，不同顺序会产生不同的同源合法性。','定义方推导前缀及逐结果的使用/失效关系；调用方代入真实来源。','固定use/kill片段用U/K/D；一般状态使用有限角色和已核分区。','use(a); close(b) 可以允许a=b；close(b); use(a)需要apart。','遗漏错误Drop或不返回递归前缀会放行坏访问。','先失效后使用 ⇒ 对应角色必须不相交','不枚举所有运行轨迹，不把顺序合同当任意业务正确性证明。'),
('R38','描述与准确清理责任分开','存活与回收','复制入口不应产生第二份释放责任，保守别名集合不能代替实际清理。','数据描述、当前操作资格和真实清理载体独立表示且在管理处关联。','关闭时只消费实际选中目标；退出按真实记录处理剩余责任。','a与b是两个资源；关闭选中的p只履行其中一项责任。','把may-kill全部标记Done会泄漏；全部再清理会重复释放。','描述复制不增加责任；实际责任恰好转交/履行','条件位、记录、路由和管理器是真实成本，不是全局隐形账本。'),
('R39','汇总位置只允许弱更新','值与存储','一个抽象家族可代表多个对象，单个候选项未必是单个位置。','只有唯一、精确且One的位置才能强更新；Many或多个候选保留旧事实。','源图固定，写入同位置的别名事实共同更新；循环实例不按源码点合并。','p属于Store.nodes，初始化p不能宣布整个Store.nodes已经初始化。','把singleton family当One会漏掉旧来源或未初始化对象。','StrongUpdate ⇒ exact ∧ One ∧ singleton','弱更新会降低精度，不能以静态不确定代替真实执行状态。'),
('R40','管理路径与数据身份配对','存活与回收','知道结果可能来自谁，不等于知道应该更新哪份清理记录。','管理调用同时核对payload、record、authority对应同一实际资源。','包移动重算当前投影；字段替换撤销旧绑定；多源返回同步选择路由。','pick返回原生入口；若还承诺manage(result)，须带可执行关联。','闭包捕获旧文件却从新字段取Debt，可能关闭错误资源。','Target(payload)=Responsible(record)=Managed(authority)','管理型接口可能有额外ABI；普通Ref布局不按消费者后续行为变化。')]
for rid,name,cat,why,definition,how,example,counter,formula,boundary in newrules:
 d['rules'].append(dict(id=rid,name=name,category=cat,why=why,definition=definition,how=how,example=example,counter=counter,formula=formula,boundary=boundary))
for vid,rr in {'V02':['R38'],'V05':['R38','R40'],'V06':['R38'],'V14':['R39','R40'],'V21':['R37'],'V15':['R39']}.items():
 v=next(x for x in d['variables'] if x['id']==vid);v['relatedRules']=list(dict.fromkeys(v['relatedRules']+rr))
# id: values / exact current decision. No inherited inferred-home, automatic retirement, or default Stack.
settings={
'V01':(['value','identity'],'普通值与共享身份分开；资源描述复制只复制入口，独立内容复制显式。'),
'V02':(['implicit-copy','affine-move','explicit-clone','shared-owner-copy','description-copy'],'Value按Copy/Move，Managed按责任转交，Desc复制入口；共享拥有与clone均显式。'),
'V03':(['whole-construction','definite-assignment','staged-publication','raw-storage'],'强更新需要精确One；预留、部分初始化、发布和真实清理分别核验。'),
'V04':(['static-range','checked-access','checked-view','caller-range-contract'],'范围与对齐按整次访问核验；受检分区显式返回保证，raw边界受审计。'),
'V05':(['accurate-carrier','unique-owner','region-owner','shared-owners'],'位置、完整包或管理器准确持责；描述本身不保活，显式共享协议另计成本。'),
'V06':(['explicit-release','scope-exit','last-owner','retire-boundary'],'显式关闭、确定出口、显式共享和有界暂存各有真实协议；不自动全局规划最早回收。'),
'V07':(['per-object','owner-group','declared-region'],'选择实际存储和显式稳定域；不根据逃逸自动提升所有对象到更长命区域。'),
'V08':(['borrowed-edge','weak-edge','strong-edge'],'Ref为非拥有描述，Key为可失效身份；强拥有边与Weak只在明确选择的库中出现。'),
'V09':(['unlink-only','move-out','retire-valid','invalidate-now'],'unlink、destroy/remove、detach及显式批次暂存不混同；旧访问按真实存储和保护判断。'),
'V10':(['direct-address','scoped-index','checked-key'],'稳定描述可直接地址/已选索引；独立可删Key走明确管理入口，不默认每对象generation。'),
'V11':(['stable-storage','pinned-window','logical-relocation'],'真实地址窗口约束搬迁；place可重新定位；包move保持身份，字段replace建立新身份。'),
'V12':(['immutable-sharing','exclusive-borrow','short-permission','dynamic-guard','synchronized-access'],'同上下文别名顺序操作合法；持续&/&mut、动态guard和并发同步保留各自约束。'),
'V13':(['whole-owner','known-fields','continuous-ranges','verified-partitions'],'有限字段、连续范围和已核分区细化足迹；未知链归相关存储族，不遍历运行时堆。'),
'V14':(['named-lifetimes','origin-paths','inferred-contract'],'局部来源与潜在行为推导；公开复杂关系通过contract固定，地址/lending保留必要life。'),
'V15':(['static-proof','checked-entry','checked-each-use','trusted-boundary'],'核心为有限静态充分条件；显式Key/范围/同步按实际协议检查，外部原语单独审计。'),
'V16':(['error-union','termination'],'E!T、Option和终止路径分开；?只传播对应错误；panic与挂起收口保持存储。'),
'V17':(['nominal'],'名义类型与唯一impl；匹配候选不是成功证据，安全失败不得改派发。'),
'V18':(['definition-bounds','staged-checking'],'普通泛型按bound检查；具体反射产物分别重检，纯品牌不单独造成机器实例。'),
'V19':(['static-direct','function-pointer','interface-table','tag-dispatch'],'静态路径直接派发；dyn、函数入口与标签分派明确选择并保留真实合同。'),
'V20':(['per-type','proved-code-need','shared-core'],'按真实布局、Drop、错误和调用需求决定实例；已证等价可共享，不默认以字典换全部运行成本。'),
'V21':(['declared-contract','inferred-export','parameterized-contract','ordered-effects'],'前缀、逐结果、访问、历史撤销、管理路由和潜在方法槽共同导出；固定合同不静默加强。'),
'V22':(['borrowed-environment','owned-environment','shared-environment','no-environment'],'短环境不逃逸；具体拥有环境可内联；异构/共享环境有显式存储与调用成本。'),
'V23':(['blocking-stack','stackful-task','state-machine'],'Direct与SABI接口明确；静态状态帧优先，递归/未知布局显式选择动态边界，Stack不是默认。'),
'V24':(['structured-join','registered-operation'],'控制流完成根保留任务/设备责任；ticket消失不删除登记；结束所有真实用户才回收。'),
'V25':(['declarations-only','audited-wrapper','checked-bridge'],'C导入只给声明；wrapper覆盖来源、保留、重入、线程、异常、完成及DMA平台义务。'),
'V26':(['hygienic-syntax','compile-time-program','external-generator','syntax-generation'],'#统一编译入口：#(...)求值、#{...}块、#[...]附着、#emit生成；阶段固定，产物重检。'),
'V27':(['file-module-cache','query-fingerprint','used-contract','shared-semantic-cache'],'语义/ABI/实现/位置依赖分离；生成与求解不形成活动环；只传播真实结果变化。'),
'V28':(['stop-required','request-evidence','explicit-runtime-route','skip-optional'],'必需义务不足拒绝或ResourceLimit；动态方案须明示，优化不足不改变安全接受范围。')}
for x in d['settings']:
 if x['language']=='zil':
  x['values'],x['summary']=settings[x['variable']];x['scope']='当前规范的设计选择；不表示生产实现、完整证明或性能已通过。';x['sources']=['SRC-ZIL-CURRENT'];x['state']='known'
# Current language goal routes. Each entry retains the original goal's literal acceptance criterion.
rows=[
('G01','来源覆盖＋当前操作资格',['R04','R05','R37','R38','R39'],'派生值、字段和闭包保留来源，实际访问前要求目标仍有效。','未知目标集合必须全部满足条件；外部保留也进入合同。','来源与窗口分析；明确动态身份才支付解析。','描述存在不等于持续保活，也不能让普通局部资源自动逃逸。','ZIL-ORIGIN'),
('G02','描述复制不复制责任',['R03','R21','R38','R40'],'管理位置、包或容器准确持有一次责任，别名只复制入口。','payload、record与管理资格绑定同一实例。','条件消费的flag/选择器，管理接口可能携带记录。','不能把静态may-kill当作实际都已释放。','ZIL-CLEANUP'),
('G03','One/Many与分阶段初始化',['R01','R35','R39','R21'],'精确位置才能强更新，部分构造只发布已初始化合法值。','字段/变体合法；所有失败出口处理真实完成部分。','动态构造的计数或位图，以及合法值检查。','保留字节不等于保留已析构的T。','ZIL-ORIGIN'),
('G04','范围原语与受检分区',['R02','R18','R35'],'核对整次访问宽度、对齐和溢出，必要检查保留。','分区必须基于真实范围，不能只比较变量名字。','检查、分区与目标对齐成本计入。','SIMD、halo和设备访问不能仅检查起点。','ZIL-ACCESS'),
('G05','持续窗口与撤销历史',['R05','R13','R04'],'内部缓冲搬迁撤销旧地址见证，出口状态恢复不复活旧视图。','窗口所有使用包含保存、返回、Drop和外部使用。','静态窗口分析；逻辑位置可能需要重新定位。','Node外壳存活不能证明内部buffer地址稳定。','ZIL-ACCESS'),
('G06','执行权限与真实同步',['R06','R18','R19'],'描述共享不复制并行写权；每项相交访问受权限或同步保护。','实际字节不交或对应happens-before；迁移包含Cleanup。','必要锁/原子、分区和缓存一致成本。','两个图根不同不证明可达图不相交。','ZIL-CONCURRENCY'),
('G07','活动环境与结算协议',['R05','R20','R21'],'回调自移除/替换只提出明确请求，当前环境保留到调用结束。','Active/Settling状态及旧Cleanup依赖成立。','有限状态、待替换环境；额外队列显式。','普通赋值不自动变成延迟析构，嵌套相交调用可返回Busy。','ZIL-CAPTURE'),
('G08','外部合同与活入口',['R22','R12','R19'],'FFI wrapper核保留、回调、异常和实际完成，头文件不授信。','外部实现履行明确合同；不先读旧指针再验ID。','验证、封装、登记或同步按真实接口计费。','不保证隔离恶意C或任意设备写入。','ZIL-ASYNC'),
('G09','共享描述＋稳定存储',['R07','R08','R38'],'稳定存储持有节点；图边可成环，操作资格与连接分离。','来源族和当前权限成立，节点不独立失效。','节点/块真实分配；没有强制逐边RC或generation。','周期数据连接不保证任意析构依赖环可自动排序。','ZIL-CORE'),
('G10','显式detach或批次暂存',['R09','R21','R38','R40'],'移出成员关系后，完整对象及真实责任转到确定的暂存载体。','地址未改变，外借/allocator/清理依赖被覆盖。','实际暂存槽、关系维护和保留内存。','普通remove若真正失效，不能仍用旧Ref；不自动LCA延期。','ZIL-CLEANUP'),
('G11','独立回收的Store协议',['R09','R10','R14','R21'],'删除与准入互斥，确认真实使用结束后释放对应对象。','实际存储支持单项回收，旧Key不能复活。','索引/ID、保护、析构和关系清理。','Arena内部指针不能直接获得整块free权。','ZIL-ROUTE'),
('G12','独立身份＋活管理器准入',['R14','R19','R22'],'目标结束后Key可保留；使用时先验证，再在同一保护内访问。','身份空间耗尽处理，registry本身仍活。','每次准入可能查表，保护不能省略。','身份检查成功不证明验证后不会被释放。','ZIL-ACCESS'),
('G13','拥有包与内部投影',['R03','R04','R13','R40'],'返回完整管理存储及根；包移动重定位外壳关联，不扫描数据图。','责任无重复，内部/外借依赖闭合。','实际头与必要条件状态；直接稳定边可保持。','旧原生头地址仍被使用时，不能自由move。','ZIL-PACKAGE'),
('G14','显式载体的保留预算',['R07','R09','R24'],'按责任迫使保留的完整物理分配闭包核算空间。','有数量、payload及外部等待界才可给有限总界。','块、缓冲、环境、碎片、帧和暂存均计费。','K个节点不推出K×sizeof(Node)的完整内存上界。','ZIL-ACCEPT'),
('G15','规范出口与准确清理',['R21','R35','R38'],'正常、错误、panic和取消按各自出口保留责任与确定顺序。','清理期间进入Closing，残余责任有活载体。','flag、完成计数、清理代码与异常桥接。','不自动回滚已发生I/O，也不任意重排Drop。','ZIL-CLEANUP'),
('G16','迭代实例与实际容器',['R21','R38','R39'],'每轮结束清理或转交本轮资源，静态节点可复用但实例不合并。','回边保持不变量，跨轮状态进入真实Option/Store。','循环槽和容器真实容量。','无限轮数不等于可以无界隐藏暂存。','ZIL-CFG'),
('G17','逻辑操作与存储合同分开',['R08','R09','R23','R25'],'稳定图与动态Store共享可兼容操作，变化集中在真实身份/错误边界。','类型实际提供所需能力；首次抽象成本也计算。','适配、生成与表示成本全部计费。','不保证树→图→独立删除只改一行或零重构。','ZIL-TRAIT'),
('G18','短操作复用一份执行资格',['R08','R06','R37'],'多个描述可顺序修改同一节点，不先制造两个独占地址。','无冲突持续窗口，访问按固定顺序执行。','普通访问无逐边登记；可变别名不获noalias。','同时持有两个&mut仍需真实不交。','ZIL-ACCESS'),
('G19','有限字段足迹合同',['R18','R34','R37'],'Ref接收者的helper只导出实际相关的已声明字段足迹。','不透明公开接口须发布稳定抽象投影。','正文推导与接口依赖是编译工作。','代码开始触及items时，遍历期间的调用可能不再合法。','ZIL-CONTRACT'),
('G20','Value和Desc模式分开',['R17','R03','R38'],'普通Copy值有值语义；资源描述共享目标，独立内容clone明确。','模式在形状阶段已确定；不以赋值失败更换模式。','真实值复制和clone成本；描述不复制内容。','含Ref的结构浅复制不是整张图的深复制。','ZIL-MODES'),
('G21','返回来源与地址窗口',['R04','R05','R13'],'零复制视图依赖受保护输入，复杂来源由contract表达。','返回值来自仍活来源或完整拥有包，不延长局部栈。','来源/窗口静态信息；实际切片ptr/len。','from(a,b)只给来源上界，不提供管理路由。','ZIL-ROUTE'),
('G22','方法合同槽组合',['R20','R25','R30','R31'],'构造lazy适配器不调用回调；推进/销毁分别实例化已核槽。','固定捕获上界或自带资源包，lending保持每调用关系。','真实环境/迭代器状态和必要特化。','动态异构环境不能用类型擦除抹去清理依赖。','ZIL-CAPTURE'),
('G23','非逃逸借入环境',['R20','R30'],'本次调用内的具体环境可栈存或直接特化，不强制Box。','Call/Cleanup合同真实，无逃逸；可重复调用需状态闭合。','间接env+invoke若选择则单列。','非逃逸不自动等于纯、无挂起或可任意重入。','ZIL-CAPTURE'),
('G24','逻辑字段与真实地址投影',['R13','R23'],'已选布局按明确投影降低，缺少连续地址就拒绝对应借用。','类型/trait保持所需业务合同，公开ABI变化传播。','索引、转换和额外基址按布局实际计费。','SoA不能假造连续&Node或隐式复制回写。','ZIL-LOWERING'),
('G25','能力参数决定Direct/SABI',['R33','R25','R11'],'同一业务正文按已知Io能力生成直接调用或静态挂起帧。','接口先确定效果/调用模式，同步入口需真实执行根。','真实帧、调度、不同实例；无隐藏阻塞。','未知挂起递归必须显式选择动态存储或Stack。','ZIL-ASYNC'),
('G26','挂起帧保留地址和责任',['R04','R13','R11','R12'],'跨挂起状态在稳定帧，借用与外部依赖继续保留。','发布前稳定；返回借帧的值阻止销毁。','静态帧和必要动态边界，计峰值与驻留。','装箱外壳不会延长借来的父栈。','ZIL-ASYNC'),
('G27','共享读与结构化完成根',['R06','R11','R19'],'子任务共同读受保护范围，父相交修改暂停至归还。','来源覆盖整个任务，join是实际同步。','任务/登记/同步成本；不强制复制。','引用计数和Pin都不能单独证明内容并发安全。','ZIL-CONCURRENCY'),
('G28','实际范围分区',['R18','R19','R02'],'静态或受检分区建立实际不交范围，分别交给任务。','访问宽度、halo、对齐、溢出与清理足迹同时满足。','一次准入与实际发布/同步。','不同业务索引表达式不必然不同。','ZIL-CONCURRENCY'),
('G29','不可遗忘的完成根',['R11','R12','R21','R22'],'取消仅请求；所有submit/poll/callback/waker等真实用户结束才清理。','准入与登记线性化，raw入口保持到no-future。','真实完成登记和可能等待/保留。','只看active=0或dataDone不足以释放。','ZIL-ASYNC'),
('G30','有环境前提的取消预算',['R11','R12','R24'],'按驱动停止、调度、请求传播和清理的真实界计算退出预算。','每个有关外部动作有可用时间界。','等待与保留内存也计入，不用提前free达时限。','任意不响应设备不能保证有界退出。','ZIL-ASYNC'),
('G31','明确prepare/commit/notify',['R32','R21','R35'],'可能失败准备先行，在保护下核读集并提交，再通知。','提交步骤满足已发布协议；外部副作用不自动重放。','准备存储、冲突检查与锁/版本协议。','独占nosuspend不等于事务回滚。','ZIL-CONTRACT'),
('G32','静态描述不预付管理层',['R08','R13','R23','R38'],'稳定直接访问不默认RC、generation、租约或逐引用清理记录。','来源/权限证据成立；动态身份另选协议。','真实寻址、边界和布局成本仍存在。','去掉管理检查不自动得到noalias和全负载更快。','ZIL-LOWERING'),
('G33','静态派发与真实强窗口',['R18','R25','R26'],'具体目标直接调用；hot范围提供真实不交证据。','布局、Drop、错误、调用和目标属性合法。','必要实例和代码体积计编译账。','同尺寸不够共享实例，普通Ref不假装排他。','ZIL-TRAIT'),
('G34','已证代码需求共享',['R25','R26','R36'],'只合并真实机器行为等价的实例，证明信息不独立特化。','共享规则保持布局、有效值、Drop、错误与别名。','判定、codegen、共享入口真实成本。','实际命中率与净收益需测；未证明就保留具体实例。','ZIL-TRAIT'),
('G35','真实依赖与结果指纹',['R27','R28','R34'],'语义、ABI、实现和位置依赖分别传播，导出合同固定边界。','inline/comptime/布局消费者仍记录实际实现依赖。','缓存维护、序列化和核验计费。','仅换缓存框架不证明比Rust快。','ZIL-ARCH'),
('G36','有限分析与分阶段成本',['R25','R27','R31','R36'],'固定词汇、来源共享和有限接口避免任意全程序搜索。','四目标与公平负载profile同时成立。','前端、生成、核验、codegen和链接全部计时。','有限性不等于同速；不从宿主crate数推导ZIL速度。','ZIL-ARCH'),
('G37','确定逻辑配额与结果分类',['R31','R34'],'类型/CT/合同增长有可复现配额，未完成不输出程序。','冷热缓存记录同一逻辑费用，优化预算另列。','诊断和证据有界，不搜索全局最小反例。','ResourceLimit不等于NoImpl或已证明不安全。','ZIL-CFG'),
('G38','#统一入口、分阶段属性',['R29','R31','R34'],'#求值、块、属性与生成分别调度；卫生产物统一重检。','输入快照、阶段依赖、固定impl集与注册属性目标成立。','CT执行、生成节点、实例及首次准备全部计费。','#[repr]不等于任意函数，#[allow]不能禁用安全错误。','ZIL-ATTRIBUTE'),
('G39','合同替换与ABI版本',['R27','R28','R34','R40'],'新正文不加强前提、不扩大破坏、不减保证；管理路由改变ABI。','导出合同来自定义方已核正文。','真实消费者重查/重生成，不传播无关私有变化。','顺序重排即使普通签名相同，也可能破坏同源调用。','ZIL-MODULE'),
('G40','独立核验与可追溯原因',['R04','R28','R31','R34'],'诊断与构建共用事实；核验器检查转移覆盖而不重跑求解器自证。','可信schema与原语边界明确，缓存不能伪造Verified。','共享事实ID、原因图与核验费用计入。','schema类型检查和有限模型不等于全语言证明。','ZIL-ARCH'),
('G41','完整同语义产品对照',['R23','R24','R36'],'固定输入与资源/错误合同，比较构造到回收及应用加库负担。','合理Rust可用GhostCell/Arena/索引/封装。','快照、转换、清理、调度和峰值均计入。','本图谱不提供尚未产生的性能百分比。','ZIL-ACCEPT'),
('G42','实际责任最终收口',['R11','R12','R21','R38'],'有限正常执行且外部用户与Cleanup都结束时，载体枚举处理剩余责任。','没有显式泄漏、永久等待或无根共享循环等阻碍。','真实清理与等待，退役载体保留成本。','内存安全本身不保证任意程序最终无泄漏。','ZIL-CLEANUP')]
route_by={}
for gid,title,rules,summary,premise,cost,limit,ref in rows:
 route_by[gid]={'goal':gid,'language':'zil','status':'empirical' if gid in ['G34','G35','G36','G41'] else 'proposal','mechanism':title,'summary':summary,'rules':rules,'premises':[premise,'这是规范路线，不是生产实现或完整证明的通过状态。'],'cost':cost,'limit':limit,'sources':['SRC-ZIL-CURRENT'],'steps':[summary,premise,'拒绝或明确处理越过边界的操作：'+limit],'specRef':ref}
d['coverage']=[route_by[c['goal']] if c['language']=='zil' else c for c in d['coverage']]
zl=next(x for x in d['languages'] if x['id']=='zil');zl.update(summary='共享描述、有限操作合同与准确清理；显式存储策略；#编译入口和分阶段属性。',scope='当前 _zil_design.md；不默认通用世代/租约，不采用自动归属提升与自动退役规划。',maturity='规范路线；实现、证明和产品验收分开。',sources=['SRC-ZIL-CURRENT'])
# Replace only ZIL example code; preserve the other languages as the existing explanatory survey.
codes={
'E01':'let a = Buffer::<i32>.new(alloc)?\nlet b = a // Describe：同一资源入口，不复制头和责任\nb.push(9)?\n// 普通拥有包的按值传递仍移动责任；\n// 独立内容复制必须显式clone。',
'E02':'let graph = StableStore::<Node>.new(alloc)?\nlet a = graph.insert(Node { next: None })?\nlet b = graph.insert(Node { next: Some(a) })?\na.next = Some(b)\nlet root2 = b // 复制描述，不复制清理责任',
'E03':'fn bump(a: Ref<Node>, b: Ref<Node>) {\n    a.value += 1\n    b.value += 1\n}\nbump(n, n) // 同上下文短操作；无冲突持续窗口',
'E04':'let p = nodes.resolve(key)?\nlet removed = nodes.detach(key)?\nconsume(p.value)\nclose(removed)\n// 只有detach保持实际地址并转交完整责任时成立；\n// 不自动把destroy改成延期清理。',
'E05':'fn record(engine: Ref<Engine>, msg: &Message) {\n    engine.statistics.count += msg.weight\n}\nfor msg in &engine.messages {\n    record(engine, msg)\n}\n// helper发布statistics足迹，不隐式借走全部engine。',
'E06':'fn prefix(x: &[u8], n: usize) -> &[u8]\ncontract { result(from(x)); } {\n    return &x[0..n]\n}\n// 单输入关系可推导；这里展示显式固定合同。',
'E07':'fn visit<F: FnMut(i32)->void>(xs: &[i32], f: &mut F) {\n    for x in xs { f(x) }\n}\nvar each = (x: i32) => { total += x }\nvisit(xs, &mut each)\n// 重复调用仍检查Call状态闭合；fn指针不能存捕获环境。'}
for e in d['examples']:
 e['code']['zil']=codes[e['id']];e['note']='ZIL为规范示例，未在生产编译器执行；各语言展示机制差异，不作为同语义性能或行数基准。'
d['examples'].extend([
 {'id':'E08','name':'拥有模式相同，不制造比较差异','goals':['G02','G13','G39'],'intent':'双方输入和输出均为按值转交的拥有包，不与借入Ref混比。','code':{'zil':'fn pick(first: bool, a: OwnedNode, b: OwnedNode)\n    -> (OwnedNode, OwnedNode) {\n    if first { return (a, b) }\n    return (b, a)\n}','rust':'fn pick(first: bool, a: OwnedNode, b: OwnedNode)\n    -> (OwnedNode, OwnedNode) {\n    if first { return (a, b); }\n    (b, a)\n}'},'note':'假设OwnedNode为相同责任语义的拥有包；这一类转交基本持平。'},
 {'id':'E09','name':'#编译入口不与属性冲突','goals':['G38','G39'],'intent':'定界符区分求值与附着请求；属性类别决定消费阶段。','code':{'zil':'#[repr(C)]\n#[align(16)]\nstruct Packet { code: u32; size: u32 }\n\nlet n = #(square(12))\nlet xs = #([1, 2])\n// #[1,2]不能当编译期数组。'},'note':'统一的是#入口与基础设施，不是任意编译器访问权限。'},
 {'id':'E10','name':'顺序进入公开安全合同','goals':['G01','G02','G18','G39'],'intent':'参数模式相同但操作顺序不同，允许的同源绑定也不同。','code':{'zil':'fn before(a: Ref<Node>, b: Ref<Node>) {\n    inspect(a.value)\n    b.destroy()\n}\nfn after(a: Ref<Node>, b: Ref<Node>) {\n    b.destroy()\n    inspect(a.value)\n}\nbefore(x, x) // 可接受\nafter(x, x)  // 拒绝'},'note':'当前管理资格、无残余地址窗口与真实函数效果必须成立；不是按名字推断安全。'}])
d['examples'].extend([{'id': 'E11', 'name': '组默认接收者与词法成员', 'goals': ['G20', 'G39'], 'intent': '签名先定receiver，缩短重复，不依正文猜能力。', 'code': {'zil': 'struct Point { x: i32; y: i32 }\nimpl Point(self: &Self) {\n    fn sum() -> i32 => .x + .y\n    fn shift(self: &mut Self, dx: i32) { .x += dx }\n}\nimpl Point { fn origin() -> Self => Point { x: 0, y: 0 } }'}, 'note': 'ZIL-RECEIVER；规范样本，未由生产编译器执行。'}, {'id': 'E12', 'name': '自动捕获与显式快照', 'goals': ['G22', 'G23'], 'intent': '捕获、调用能力、目标状态三者分开。', 'code': {'zil': 'var n = 1\nlet snapshot = move () => n\nn = 2\nlet old = snapshot() // 1\nvar add = (x: i32) => { n += x }\nadd(3)\n// 直接Desc写目标并不自动变成长&mut。'}, 'note': '所有源码放在正常函数内；move地址不延寿。'}, {'id': 'E13', 'name': '记录与一元管道', 'goals': ['G20', 'G38'], 'intent': '省略类型与调用糖保持确定求值和清理。', 'code': {'zil': 'struct Point { x: i32; y: i32 }\nfn shift(p: Point) -> Point => ({ ...p, x: p.x + 1 })\nfn total(p: Point) -> i32 => p.x + p.y\nfn main() -> i32 {\n    let p: Point = { x: 1, y: 2 }\n    return p |> shift |> total\n}'}, 'note': '不可Copy base在spread时移动；没有默认clone、插参或错误传播。'}, {'id': 'E14', 'name': '返回静态闭包环境', 'goals': ['G13', 'G22'], 'intent': '隐藏具体类型，不隐藏借用和责任合同。', 'code': {'zil': 'fn add_by(n: i32) -> impl Fn(i32)->i32 {\n    return move (x: i32) => x + n\n}\nfn main() -> i32 {\n    let f = add_by(5)\n    return f(2) + f(3)\n}'}, 'note': '设计示例：C1尚不支持闭包返回或opaque callable。一个返回槽对应一个隐藏具体类型；不自动Box或合并不同闭包。'}])
for s in d['sources']:
 if s['id']=='SRC-ZIL-R2':s.update(id='SRC-ZIL-CURRENT',title='ZIL 当前语言与编译器规范',url='_zil_design.md',kind='provided-and-designed',note='当前规范与其具名附件；设计要求不等于实现、证明或性能已完成。')
d['sources'].extend([
 {'id':'SRC-ATTR-RUST','title':'Rust Reference · Attributes','url':'https://doc.rust-lang.org/reference/attributes.html','kind':'official','note':'属性有不同类别与阶段；ZIL只参考机制，不复制完整语义。'},
 {'id':'SRC-ATTR-CS','title':'C# specification · Attributes','url':'https://learn.microsoft.com/en-us/dotnet/csharp/language-reference/language-specification/attributes','kind':'official','note':'裸[]属性可行；C#运行时元数据模型未被自动采用。'},
 {'id':'SRC-LEAN-SYNTAX','title':'Lean · Notations and Macros','url':'https://lean-lang.org/doc/reference/latest/Notations-and-Macros/','kind':'official','note':'语法、宏、卫生与elaboration分层参考。'}])
d['meta'].update(version='ZIL-LANG-1.3',date='2026-10-23',rules=len(d['rules']),sourceSpec='_zil_design.md',scope='Current normative ZIL routes; other language survey preserved from input, not reverified')
spec=(ROOT/'_zil_design.md').read_text();digest=hashlib.sha256(spec.encode()).hexdigest()[:12];d['meta']['specSha256']=hashlib.sha256(spec.encode()).hexdigest()
workflow=(ROOT/'docs/WORKFLOW.md').read_text();goal=(ROOT/'_zil_goal.md').read_text()
guide='''# 阅读与使用

## 从目标查看方案

顶部多选语言；场景页保留原42个目标及其验收原文。点击关系节点可查看规则、前提与成本；线宽只表示依赖，不表示性能。

## 当前规范与资料范围

ZIL数据、变量选择、代码示例和完整正文同步来自当前规则。所有ZIL路线标为设计或待实测，不把规范中的“必须”显示成已实现。其余七种语言的原有方案与来源沿用用户输入，没有在本次逐项重新研究。

## 施工文件的分工

终态规范只定义规则；施工总计划定义阶段；工单定义单项实现与状态。变更审查、实际检查记录有单独页。HTML是生成视图，不手工维护另一份规范。

## 可下载内容

页面下载规范与独立Markdown字节一致。设计/来源页提供施工计划、协作规程、语法与属性附件。完整ZIP另含任务单、门禁工具和证据；单文件HTML离线阅读无需网络。

## 比较口径

参数模式、错误、回收与并发合同必须一致。拥有值对拥有值、借入描述对借入描述；Rust允许合理Cell/GhostCell、索引、Arena和封装。不同机制能实现同一业务，不等于语言没有差异；省几个token也不自动证明净收益。
'''
syntax='''# 语法与属性实施入口

规范性解释见完整规范第15—18章。以下附件由同一构建过程嵌入，不是另一套语法。

## 文法产生式

```ebnf
'''+(ROOT/'spec/grammar.ebnf').read_text()+'''\n```\n
## 属性注册表

```json
'''+(ROOT/'spec/attributes.json').read_text()+'\n```\n'
verification=(ROOT/'evidence/REPORT.md').read_text() if (ROOT/'evidence/REPORT.md').exists() else '# 检查范围\n\n文档与阅读系统检查记录由构建生成，不代表语言实现。'
docs={'spec':spec,'guide':guide,'goal':goal,'workflow':workflow,'syntax':syntax,'changes':(ROOT/'changes/REVIEW.md').read_text(),'verification':verification}
# JSON embedding must not close its script or execute source snippets.
def safe(obj):return json.dumps(obj,ensure_ascii=False,separators=(',',':')).replace('<','\\u003c').replace('>','\\u003e').replace('&','\\u0026')
out=re.sub(r'(<script id="atlas-data" type="application/json">).*?(</script>)',lambda m:m[1]+safe(d)+m[2],text,flags=re.S)
out=re.sub(r'(<script id="atlas-docs" type="application/json">).*?(</script>)',lambda m:m[1]+safe(docs)+m[2],out,flags=re.S)
out=out.replace('<span class="edition">D2</span>',f'<span class="edition">规范1.3 · {digest}</span>')
out=out.replace('语言设计图谱 D2 · 2026-10-19','语言设计图谱 · 2026-10-23')
out=out.replace('ZIL 按 R2 设计合同登记','ZIL 按当前规范路线登记')
out=out.replace('language-atlas-d2','language-atlas-current-v1')
out=out.replace("proposal:'设计目标'","proposal:'规范路线（未验收）'")
out=out.replace("['guide','sources','changes','verification'].includes(id)","['guide','goal','workflow','syntax','sources','changes','verification'].includes(id)")
out=out.replace("[{id:'0',title:'定位与阅读口径'","[{id:'intro',title:'规范权威与阅读口径'")
out=out.replace("[['guide','使用与设计说明'],['spec','ZIL 完整语言设计'],['sources','官方资料与证据'],['changes','本版修改'],['verification','验证记录']]","[['guide','使用说明'],['spec','终态规范'],['goal','施工总计划'],['workflow','协作规程'],['syntax','语法与属性'],['sources','资料与来源'],['changes','变更审查'],['verification','实际检查']]")
out=out.replace('ZIL R2 原始规范','ZIL 当前终态规范').replace('保留最新找回的 R2 原文；展示体系的修改不改写语言合同。','与独立 _zil_design.md 同源生成；正文不混入排期、版本过程和测试成绩。')
out=out.replace('ZIL 来自本次找回的 R2 设计；其他语言的方案判断是对来源的归纳，不等于本轮逐语言编译与性能实测。','ZIL 对应当前规范；其他语言沿用上传图谱的资料快照，未在本次逐项重新核实，也不是本次编译或性能实测。')
out=out.replace("download('zil-language-design.md',DOCS.spec","download('_zil_design.md',DOCS.spec")
# New documentation pages are still single-source files. Download them directly, without requiring ZIP extraction.
out=out.replace("else content=`<article class=\"panel doc\">${markdown(DOCS[state.doc]||'暂无内容。')}</article>`;","else content=`<article class=\"panel doc\"><div class=\"doc-toolbar\"><p>此页由对应源文件生成，不另行维护规则。</p>${btn('download-doc','下载本页 Markdown')}</div>${markdown(DOCS[state.doc]||'暂无内容。')}</article>`;")
out=out.replace("if(action==='download-spec'){", "if(action==='download-doc'){download(({goal:'_zil_goal.md',workflow:'WORKFLOW.md',syntax:'SYNTAX.md',changes:'REVIEW.md',verification:'REPORT.md',guide:'GUIDE.md'})[state.doc]||'document.md',DOCS[state.doc]||'', 'text/markdown;charset=utf-8');return;}\n if(action==='download-spec'){")
# Stable requirement anchor navigation from Markdown is supported without making HTML authoritative.
out=out.replace("if(a.specAnchor){e.preventDefault();const n=/^#s?(\\d+)/.exec(a.specAnchor);if(n&&specChapters.some(c=>c.id===n[1])){state.view='docs';state.doc='spec';state.chapter=n[1];render({top:true});}return;}","if(a.specAnchor){e.preventDefault();const id=a.specAnchor.slice(1);const chapter=specChapters.find(c=>c.text.includes('id=\"'+id+'\"'))||specChapters.find(c=>c.id===id.replace(/^s/,''));if(chapter){state.view='docs';state.doc='spec';state.chapter=chapter.id;render({top:true});}return;}")
# Proposal filter includes ZIL empirical goals under an explicitly broader design switch.
out=out.replace("state.includeProposals&&c.status==='proposal'","state.includeProposals&&['proposal','empirical'].includes(c.status)")
out=out.replace('同时包含 ZIL 设计目标','同时包含 ZIL 规范路线与待实测目标')
out=out.replace('查看 ZIL 设计目标','查看 ZIL 规范路线')
# Avoid overflow from long SHA/edition in narrow header.
out=out.replace('</style>','.edition{max-width:132px;overflow-wrap:anywhere}.primary-nav{min-width:0}@media(max-width:650px){.brand{min-width:0}.brand .edition{max-width:86px;font-size:9px}.top-download{max-width:132px}.topbar{align-items:flex-start}}\n</style>')
(ROOT/'lang-designer.html').write_text(out,encoding='utf-8')
(ROOT/'data/designer-data.json').write_text(json.dumps(d,ensure_ascii=False,indent=2)+'\n')
(ROOT/'data/designer-docs.json').write_text(json.dumps(docs,ensure_ascii=False,indent=2)+'\n')
report={'goals':len(d['goals']),'variables':len(d['variables']),'languages':len(d['languages']),'rules':len(d['rules']),'coverage':len(d['coverage']),'zil_routes_replaced':42,'zil_settings_replaced':28,'original_goal_records_equal':d['goals']==original['goals'],'non_zil_coverage_equal':[x for x in d['coverage'] if x['language']!='zil']==[x for x in original['coverage'] if x['language']!='zil'],'non_zil_settings_equal':[x for x in d['settings'] if x['language']!='zil']==[x for x in original['settings'] if x['language']!='zil'],'spec_sha256':d['meta']['specSha256']}
(ROOT/'evidence/designer-data.json').write_text(json.dumps(report,ensure_ascii=False,indent=2)+'\n')
print(json.dumps(report,ensure_ascii=False,indent=2))
