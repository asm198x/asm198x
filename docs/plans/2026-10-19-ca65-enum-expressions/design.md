# Retain enum bases on the existing constant path (#796)

The numeric-only enum reader loses local placeholders and the base expression.
Keep the authored record lines and the existing family-owned native payload.
A private expression-constant payload carries each member's qualified name and
Expr. During projection, lower that event to the existing Item::Equ path with
its original span and a private symbol node. This shares duplicate checks,
source width, incremental values, PC placement, final binding and debug logic;
there is no second evaluator or public AST/schema extension.

RecordBuild retains an optional enum base Expr and a running offset. An explicit
initializer replaces the base and resets the offset; a default clones the base
plus the offset. It does not reference the previous member or build an
unbounded expression chain. Allocate the target placeholder before reading an
explicit RHS; mark it defined afterwards and update the existing cheap anchor.
Use the existing constant-key and source-symbol visitors.

Named enums own an opened scope, while anonymous enums use their enclosing
scope. RecordBuild records whether it owns a scope, so closure cannot pop an
enclosing ordinary scope. Existing struct/union layout is retained. Reject the
measured nested enum/record positions. Enum closure defines no size property:
native reports enum and enum-member sizes unknown, rather than exposing the
numeric cursor as a record size. Enum members continue to set the cheap anchor;
closure does not invent a physical size anchor.

The #800 queue preserves known-width unused cycles and still validates numeric
uses and undefined names. This is necessary to retain the native valid enum
controls. Source-time query results are captured once in the base expression,
then cloned; default members do not replay name-query effects. Formatter nodes
keep the authored text and never see the private Item::Equ lowering.

Use all 94 completed research outcomes (39 accepted, 55 refused). Keep the four
bounded native direct-self timeouts as observations. Add selected stream,
macro/repeat, reserved-token, cheap-anchor and long-cursor controls before
qualification. The broader constancy/text parsing work is still #795 and
ordinary label/procedure sizes remain #798. No dependency or public contract
change is proposed; this uses the existing native payload and constant paths.

The formatter keeps enum member lines opaque. It does not execute repeat
counters or macros to read a generated identifier; a private placeholder
tracks an unexpanded named header solely for balanced scope closure. Assembly
continues to validate the expanded identifiers. Mnemonics are valid enum names
(the enum parser is not an instruction head); register tokens remain reserved.
The 42 extension probes pin that distinction and stream ordering.
