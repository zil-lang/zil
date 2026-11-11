; ModuleID = '/workspace/scratch/abf690ce880c/research/thinlto-prototype/pilot-2unit-v1/leaf.o'
source_filename = "/workspace/scratch/abf690ce880c/research/thinlto-prototype/pilot-2unit-v1/leaf.ll"
target datalayout = "e-m:e-p270:32:32-p271:32:32-p272:64:64-i64:64-i128:128-f80:128-n8:16:32:64-S128"
target triple = "x86_64-unknown-linux-gnu"

; Function Attrs: mustprogress nofree norecurse nosync nounwind willreturn memory(none)
define i32 @f(i32 %x) local_unnamed_addr #0 !guid !1 {
entry:
  %r = add i32 %x, 1
  ret i32 %r
}

attributes #0 = { mustprogress nofree norecurse nosync nounwind willreturn memory(none) }

!llvm.module.flags = !{!0}

!0 = !{i32 1, !"EnableSplitLTOUnit", i32 0}
!1 = !{i64 -3706093650706652785}

^0 = module: (path: "/workspace/scratch/abf690ce880c/research/thinlto-prototype/pilot-2unit-v1/leaf.o", hash: (26582098, 2058221880, 1293104242, 1296178612, 3590130267))
^1 = gv: (name: "f", summaries: (function: (module: ^0, flags: (linkage: external, visibility: default, notEligibleToImport: 0, live: 0, dsoLocal: 0, canAutoHide: 0, importType: definition, noRenameOnPromotion: 0), insts: 2, funcFlags: (readNone: 1, readOnly: 0, noRecurse: 1, returnDoesNotAlias: 0, noInline: 0, alwaysInline: 0, noUnwind: 1, mayThrow: 0, hasUnknownCall: 0, mustBeUnreachable: 0)))) ; guid = 14740650423002898831
^2 = blockcount: 0
