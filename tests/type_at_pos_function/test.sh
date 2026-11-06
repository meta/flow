#!/bin/bash
# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under the MIT license found in the
# LICENSE file in the root directory of this source tree.

# declare.js
printf "declare.js:3:18 = "
assert_ok "$FLOW" type-at-pos declare.js 3 18 --strip-root
printf "declare.js:5:18 = "
assert_ok "$FLOW" type-at-pos declare.js 5 18 --strip-root
printf "declare.js:6:18 = "
assert_ok "$FLOW" type-at-pos declare.js 6 18 --strip-root
printf "declare.js:8:18 = "
assert_ok "$FLOW" type-at-pos declare.js 8 18 --strip-root
printf "declare.js:9:18 = "
assert_ok "$FLOW" type-at-pos declare.js 9 18 --strip-root

# function.js
printf "function.js:4:3 = "
assert_ok "$FLOW" type-at-pos function.js 4 3 --strip-root
printf "function.js:8:3 = "
assert_ok "$FLOW" type-at-pos function.js 8 3 --strip-root
printf "function.js:12:3 = "
assert_ok "$FLOW" type-at-pos function.js 12 3 --strip-root
printf "function.js:16:3 = "
assert_ok "$FLOW" type-at-pos function.js 16 3 --strip-root
# A `this` parameter binds no identifier. Queried in friendly mode to observe
# the framing.
printf "function.js:24:20 (this parameter, framed) = "
assert_ok "$FLOW" type-at-pos function.js 24 20 --strip-root
printf "function.js:24:27 (this annotation, unframed) = "
assert_ok "$FLOW" type-at-pos function.js 24 27 --strip-root

# default.js
printf "default.js:4:16 = "
assert_ok "$FLOW" type-at-pos default.js 4 16 --strip-root
printf "default.js:5:17 = "
assert_ok "$FLOW" type-at-pos default.js 5 17 --strip-root

# function-poly-0.js
printf "function-poly-0.js:3:10 = "
assert_ok "$FLOW" type-at-pos function-poly-0.js 3 10 --strip-root
printf "function-poly-0.js:3:30 = "
assert_ok "$FLOW" type-at-pos function-poly-0.js 3 33 --strip-root
printf "function-poly-0.js:4:7 = "
assert_ok "$FLOW" type-at-pos function-poly-0.js 4 7 --strip-root

# function-poly-1.js
printf "function-poly-1.js:3:10 = "
assert_ok "$FLOW" type-at-pos function-poly-1.js 3 10 --strip-root
printf "function-poly-1.js:3:3 = "
assert_ok "$FLOW" type-at-pos function-poly-1.js 3 33 --strip-root
printf "function-poly-1.js:4:7 = "
assert_ok "$FLOW" type-at-pos function-poly-1.js 4 7 --strip-root

# function-poly-2.js
printf "function-poly-2.js:3:10 = "
assert_ok "$FLOW" type-at-pos function-poly-2.js 3 10 --strip-root
printf "function-poly-2.js:4:12 = "
assert_ok "$FLOW" type-at-pos function-poly-2.js 4 12 --strip-root
printf "function-poly-2.js:5:5 = "
assert_ok "$FLOW" type-at-pos function-poly-2.js 5 5 --strip-root
printf "function-poly-2.js:6:5 = "
assert_ok "$FLOW" type-at-pos function-poly-2.js 6 5 --strip-root
printf "function-poly-2.js:7:12 = "
assert_ok "$FLOW" type-at-pos function-poly-2.js 7 12 --strip-root
printf "function-poly-2.js:9:13 = "
assert_ok "$FLOW" type-at-pos function-poly-2.js 9 13 --strip-root
printf "function-poly-2.js:11:12 = "
assert_ok "$FLOW" type-at-pos function-poly-2.js 11 12 --strip-root

# function-poly-4.js
printf "function-poly-4.js:3:11 = "
assert_ok "$FLOW" type-at-pos function-poly-4.js 3 11 --strip-root
printf "function-poly-4.js:7:7 = "
assert_ok "$FLOW" type-at-pos function-poly-4.js 7 7 --strip-root
printf "function-poly-4.js:9:7 = "
assert_ok "$FLOW" type-at-pos function-poly-4.js 9 7 --strip-root

# function-poly-5.js
printf "function-poly-5.js:3:10 = "
assert_ok "$FLOW" type-at-pos function-poly-5.js 3 10 --strip-root

# function-poly-6.js
printf "function-poly-6.js:7:6 = "
assert_ok "$FLOW" type-at-pos function-poly-6.js 7 6 --strip-root

# function.js
# Friendly mode renders the declaration head; the bare type is pinned in
# type_at_pos_json_pretty.
printf "function.js:21:18 (framed) = "
assert_ok "$FLOW" type-at-pos function.js 21 18 --strip-root

# overloads.js
# Flow spells an overload as a name declared more than once. A call selects one
# signature, so hover prints that one and counts the rest; anywhere the whole
# set is in view it is listed a signature per line and nothing is counted.
printf "overloads.js:3:18 (declaration site) = "
assert_ok "$FLOW" type-at-pos overloads.js 3 18 --strip-root
printf "overloads.js:7:1 (call, number arm) = "
assert_ok "$FLOW" type-at-pos overloads.js 7 1 --strip-root
printf "overloads.js:8:1 (call, string arm) = "
assert_ok "$FLOW" type-at-pos overloads.js 8 1 --strip-root
printf "overloads.js:12:11 (reference, not a call) = "
assert_ok "$FLOW" type-at-pos overloads.js 12 11 --strip-root
printf "overloads.js:15:1 (call, not overloaded) = "
assert_ok "$FLOW" type-at-pos overloads.js 15 1 --strip-root
printf "overloads.js:22:3 (overloaded method call) = "
assert_ok "$FLOW" type-at-pos overloads.js 22 3 --strip-root

# function-poly-call.js
# A call to a polymorphic function prints the signature it instantiated, with
# the type arguments of that instantiation.
printf "function-poly-call.js:4:1 (implicit) = "
assert_ok "$FLOW" type-at-pos function-poly-call.js 4 1 --strip-root
printf "function-poly-call.js:5:1 (explicit) = "
assert_ok "$FLOW" type-at-pos function-poly-call.js 5 1 --strip-root
printf "function-poly-call.js:8:1 (two type parameters) = "
assert_ok "$FLOW" type-at-pos function-poly-call.js 8 1 --strip-root
printf "function-poly-call.js:14:3 (method) = "
assert_ok "$FLOW" type-at-pos function-poly-call.js 14 3 --strip-root
printf "function-poly-call.js:18:1 (polymorphic overload) = "
assert_ok "$FLOW" type-at-pos function-poly-call.js 18 1 --strip-root
printf "function-poly-call.js:19:1 (monomorphic overload) = "
assert_ok "$FLOW" type-at-pos function-poly-call.js 19 1 --strip-root
# A callee bound by `const`, held in a property or passed as a parameter prints
# as an arrow type, with the type arguments in place of its type parameters.
printf "function-poly-call.js:22:1 (const) = "
assert_ok "$FLOW" type-at-pos function-poly-call.js 22 1 --strip-root
printf "function-poly-call.js:25:3 (property) = "
assert_ok "$FLOW" type-at-pos function-poly-call.js 25 3 --strip-root
printf "function-poly-call.js:28:3 (parameter) = "
assert_ok "$FLOW" type-at-pos function-poly-call.js 28 3 --strip-root
