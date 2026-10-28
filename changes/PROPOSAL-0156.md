# 156 Defining-module concrete methods

Normative source: _zil_design.md §§14,15.4,18. Imported public nominal Copy record types resolve public methods from their defining module, preserving exact method FunctionId and all typed receiver permissions. Private or missing methods do not gain permission from import. No re-exported type ownership changes, universal extension search, dyn, generic trait or owning receiver support is implied.
