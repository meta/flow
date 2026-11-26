---
title: Type Hierarchy
slug: /lang/type-hierarchy
description: "A visual overview of Flow's type hierarchy showing the subtyping relationships between all built-in types."
---

Types in Flow form a hierarchy based on [subtyping](./subtypes.md):

```mermaid
graph BT

unknown -.- any

symbol --> unknown
null --> unknown
maybe["Maybe:
  ?string"]
maybe --> unknown
null --> maybe
undefined --> maybe
undefined --> void
void --> unknown
string --> maybe
string --> unknown

union["Union:
  number | bigint"]
number --> union
number --> unknown
union --> unknown
bigint --> unknown
bigint --> union

boolean --> unknown
true --> boolean
false --> boolean

empty-interface["interface {}"] --> unknown
some-interface["interface {prop: string}"] --> empty-interface
someclass["class A {prop: string}"] --> some-interface
inexact-empty-obj["Inexact empty object:
  {...}"]
inexact-empty-obj --> empty-interface
inexact-some-obj["Inexact object:
  {prop: string, ...}"] --> inexact-empty-obj
inexact-some-obj --> some-interface
exact-some-obj["Exact object:
  {prop: string}"] --> inexact-some-obj
exact-empty-obj["Exact empty object:
  {}"]
exact-empty-obj --> inexact-empty-obj
roarray-unknown["ReadonlyArray&lt;unknown>"] --> empty-interface
inexact-empty-tuple["Inexact empty tuple:
  [...]"]
some-tuple["Tuple:
  [string, number]"]
inexact-empty-tuple --> roarray-unknown
some-tuple --> inexact-empty-tuple
some-array["Array&lt;string>"] --> roarray-unknown

any-func["Function:
  (...ReadonlyArray&lt;empty>) => unknown"]
any-func --> empty-interface
some-func["(number) => boolean"] --> any-func
some-func2["(string) => string"] --> any-func

inter["Intersection:
  (number => boolean) & (string => string)"]
inter --> some-func
inter --> some-func2

empty --> inter
empty --> null
empty --> undefined
empty --> void
empty --> true
empty --> false
empty --> exact-some-obj
empty --> exact-empty-obj
empty --> some-tuple
empty --> some-array
empty --> string
empty --> number
empty --> bigint
empty --> someclass
empty --> symbol
any-bottom["any"] -.- empty

click unknown "../../types/unknown"
click any "../../types/any"
click any-bottom "../../types/any"
click empty "../../types/empty"
click boolean "../../types/primitives/#toc-booleans"
click number "../../types/primitives/#toc-numbers"
click string "../../types/primitives/#toc-strings"
click symbol "../../types/primitives/#toc-symbols"
click bigint "../../types/primitives/#toc-bigints"
click null "../../types/primitives/#toc-null-and-void"
click undefined "../../types/primitives/#toc-null-and-void"
click void "../../types/functions/#toc-void-and-undefined"
click true "../../types/literals"
click false "../../types/literals"
click union "../../types/unions"
click inter "../../types/intersections"
click maybe "../../types/maybe"
click some-array "../../types/arrays"
click roarray-unknown "../../types/arrays/#toc-readonlyarray"
click inexact-empty-tuple "../../types/tuples/#inexact-tuples"
click some-tuple "../../types/tuples"
click someclass "../../types/classes"
click empty-interface "../../types/interfaces"
click some-interface "../../types/interfaces"
click exact-some-obj "../../types/objects"
click exact-empty-obj "../../types/objects"
click inexact-some-obj "../../types/objects/#exact-and-inexact-object-types"
click inexact-empty-obj "../../types/objects/#exact-and-inexact-object-types"
click any-func "../../types/functions"
click some-func "../../types/functions"
click some-func2 "../../types/functions"

classDef default fill:#eee, stroke:#000, stroke-width:1px, color:#000
```

Click on a node to go to the documentation for that type.

Types appearing higher in this graph are more general, while those appearing lower are more specific.
An arrow pointing from type `A` to type `B` means that `A` is a subtype of `B`.
For example, the type `string` is a subtype of `?string`.

How can `any` be at both the top and the bottom? Because [it is unsafe](../types/any.md). This is denoted with a dotted line.

## See Also {#toc-see-also}

- [Subtypes](./subtypes.md) — the theory behind how types relate in the hierarchy
- [Any](../types/any.md) — the unsafe type that is both top and bottom
- [Unknown](../types/unknown.md) — the safe top type (supertype of all types)
- [Empty](../types/empty.md) — the bottom type (subtype of all types)
- [Variance](./variance.md) — how covariance and contravariance affect subtype relationships
