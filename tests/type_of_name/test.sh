#!/bin/bash
# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under the MIT license found in the
# LICENSE file in the root directory of this source tree.

# Basic types
printf "=== Basic types ===\n"
assert_ok "$FLOW" type-of-name-experimental basic.js myString --strip-root
assert_ok "$FLOW" type-of-name-experimental basic.js myNumber --strip-root
assert_ok "$FLOW" type-of-name-experimental basic.js myBoolean --strip-root
assert_ok "$FLOW" type-of-name-experimental basic.js myArray --strip-root
assert_ok "$FLOW" type-of-name-experimental basic.js myObject --strip-root

# Functions
printf "\n=== Functions ===\n"
assert_ok "$FLOW" type-of-name-experimental basic.js myFunction --strip-root
assert_ok "$FLOW" type-of-name-experimental basic.js arrowFunc --strip-root
assert_ok "$FLOW" type-of-name-experimental basic.js param --strip-root

# Classes
printf "\n=== Classes ===\n"
assert_ok "$FLOW" type-of-name-experimental basic.js MyClass --strip-root
assert_ok "$FLOW" type-of-name-experimental basic.js instance --strip-root
assert_ok "$FLOW" type-of-name-experimental basic.js methodName --strip-root

# Type aliases
printf "\n=== Type aliases ===\n"
assert_ok "$FLOW" type-of-name-experimental basic.js MyType --strip-root
assert_ok "$FLOW" type-of-name-experimental basic.js typeVar --strip-root

# Destructuring
printf "\n=== Destructuring ===\n"
assert_ok "$FLOW" type-of-name-experimental basic.js destructuredX --strip-root
assert_ok "$FLOW" type-of-name-experimental basic.js destructuredY --strip-root

# Error cases
printf "\n=== Error cases (muted output) ===\n"
assert_exit "$EXIT_USAGE" "$FLOW" type-of-name-experimental basic.js > /dev/null
assert_errors "$FLOW" type-of-name-experimental basic.js nonexistent # prints "not found"

# React components
printf "\n=== React Components ===\n"
assert_ok "$FLOW" type-of-name-experimental react.js BasicComponent --strip-root --unexpand-component-props
assert_ok "$FLOW" type-of-name-experimental react.js OptionalProps --strip-root --unexpand-component-props
assert_ok "$FLOW" type-of-name-experimental react.js GenericComponent --strip-root --unexpand-component-props
assert_ok "$FLOW" type-of-name-experimental react.js ConstrainedGeneric --strip-root --unexpand-component-props
assert_ok "$FLOW" type-of-name-experimental react.js InexactRest --strip-root --unexpand-component-props
assert_ok "$FLOW" type-of-name-experimental react.js IndexedRest --strip-root --unexpand-component-props
assert_ok "$FLOW" type-of-name-experimental react.js NamedProps --strip-root --unexpand-component-props
assert_ok "$FLOW" type-of-name-experimental react.js UserCard --strip-root --unexpand-component-props
assert_ok "$FLOW" type-of-name-experimental react.js DataDisplay --strip-root --unexpand-component-props
assert_ok "$FLOW" type-of-name-experimental react.js ProductItem --strip-root --unexpand-component-props
assert_ok "$FLOW" type-of-name-experimental react.js UserProfile --strip-root --unexpand-component-props
assert_ok "$FLOW" type-of-name-experimental react.js NotificationWithDocblock --strip-root --unexpand-component-props

printf "\n=== React Components (--hide-references) ===\n"
assert_ok "$FLOW" type-of-name-experimental react.js UserCard --strip-root --hide-references --unexpand-component-props
assert_ok "$FLOW" type-of-name-experimental react.js DataDisplay --strip-root --hide-references --unexpand-component-props

printf "\n=== React Components (--expand-component-props) ===\n"
assert_ok "$FLOW" type-of-name-experimental react.js UserCard --strip-root --expand-component-props
assert_ok "$FLOW" type-of-name-experimental react.js DataDisplay --strip-root --expand-component-props
assert_ok "$FLOW" type-of-name-experimental react.js ProductItem --strip-root --expand-component-props
assert_ok "$FLOW" type-of-name-experimental react.js UserProfile --strip-root --expand-component-props

# Check indexer with requests from empty file
printf "\n=== React Components in empty file ===\n"
assert_ok "$FLOW" type-of-name-experimental empty_file.js BasicComponent --strip-root --unexpand-component-props
assert_ok "$FLOW" type-of-name-experimental empty_file.js OptionalProps --strip-root --unexpand-component-props
assert_ok "$FLOW" type-of-name-experimental empty_file.js GenericComponent --strip-root --unexpand-component-props
assert_ok "$FLOW" type-of-name-experimental empty_file.js ConstrainedGeneric --strip-root --unexpand-component-props
assert_ok "$FLOW" type-of-name-experimental empty_file.js InexactRest --strip-root --unexpand-component-props
assert_ok "$FLOW" type-of-name-experimental empty_file.js IndexedRest --strip-root --unexpand-component-props
assert_ok "$FLOW" type-of-name-experimental empty_file.js NamedProps --strip-root --unexpand-component-props
assert_ok "$FLOW" type-of-name-experimental empty_file.js UserCard --strip-root --unexpand-component-props
assert_ok "$FLOW" type-of-name-experimental empty_file.js DataDisplay --strip-root --unexpand-component-props
assert_ok "$FLOW" type-of-name-experimental empty_file.js ProductItem --strip-root --unexpand-component-props
assert_ok "$FLOW" type-of-name-experimental empty_file.js UserProfile --strip-root --unexpand-component-props
assert_ok "$FLOW" type-of-name-experimental empty_file.js NotificationWithDocblock --strip-root --unexpand-component-props

assert_ok "$FLOW" type-of-name-experimental empty_file.js UserCard --strip-root --expand-component-props
assert_ok "$FLOW" type-of-name-experimental empty_file.js DataDisplay --strip-root --expand-component-props
assert_ok "$FLOW" type-of-name-experimental empty_file.js ProductItem --strip-root --expand-component-props
assert_ok "$FLOW" type-of-name-experimental empty_file.js UserProfile --strip-root --expand-component-props
assert_ok "$FLOW" type-of-name-experimental empty_file.js UserCard --strip-root --exact-match-only --unexpand-component-props # returns result from react.js

assert_errors "$FLOW" type-of-name-experimental empty_file.js NO_UserProfile --strip-root --exact-match-only

# Check builtins
assert_ok "$FLOW" type-of-name-experimental empty_file.js Array --strip-root

# Enums
printf "\n=== Enums ===\n"
assert_ok "$FLOW" type-of-name-experimental basic.js MyStringEnum --strip-root
assert_ok "$FLOW" type-of-name-experimental basic.js MyNumberEnum --strip-root
assert_ok "$FLOW" type-of-name-experimental basic.js MyBooleanEnum --strip-root
assert_ok "$FLOW" type-of-name-experimental basic.js enumValue --strip-root
assert_ok "$FLOW" type-of-name-experimental basic.js LargeEnum --strip-root

# Prop documentation
printf "\n=== Prop Documentation ===\n"
assert_ok "$FLOW" type-of-name-experimental react.js DocumentedComponent --strip-root

# Ref expansion heuristics
printf "\n=== Ref Expansion ===\n"
# Component with refs of various kinds: simple union, larger union, large object, function
assert_ok "$FLOW" type-of-name-experimental types.js RefExpansionTest --strip-root
# Direct type aliases from current file
assert_ok "$FLOW" type-of-name-experimental types.js TextWrap --strip-root
assert_ok "$FLOW" type-of-name-experimental types.js SevenColors --strip-root
assert_ok "$FLOW" type-of-name-experimental types.js LargeObjType --strip-root
assert_ok "$FLOW" type-of-name-experimental types.js Formatter --strip-root
# Generic type alias (should show "(generic type)" or similar)
assert_ok "$FLOW" type-of-name-experimental types.js DataDisplayProps --strip-root

# Complex types
printf "\n=== Utility Types ===\n"
assert_ok "$FLOW" type-of-name-experimental complex_types.js ReadOnlyUser --strip-root
assert_ok "$FLOW" type-of-name-experimental complex_types.js PartialUser --strip-root
assert_ok "$FLOW" type-of-name-experimental complex_types.js RequiredUser --strip-root
assert_ok "$FLOW" type-of-name-experimental complex_types.js UserSummary --strip-root
assert_ok "$FLOW" type-of-name-experimental complex_types.js UserWithoutEmail --strip-root
assert_ok "$FLOW" type-of-name-experimental complex_types.js UserKeys --strip-root
assert_ok "$FLOW" type-of-name-experimental complex_types.js UserValues --strip-root
assert_ok "$FLOW" type-of-name-experimental complex_types.js StatusMap --strip-root

printf "\n=== Nullable, NonNullable, Exclude, Extract ===\n"
assert_ok "$FLOW" type-of-name-experimental complex_types.js MaybeString --strip-root
assert_ok "$FLOW" type-of-name-experimental complex_types.js DefinitelyString --strip-root
assert_ok "$FLOW" type-of-name-experimental complex_types.js ActiveStatus --strip-root
assert_ok "$FLOW" type-of-name-experimental complex_types.js DangerStatus --strip-root

printf "\n=== ReturnType, Parameters, Class ===\n"
assert_ok "$FLOW" type-of-name-experimental complex_types.js SampleReturn --strip-root
assert_ok "$FLOW" type-of-name-experimental complex_types.js SampleParams --strip-root
assert_ok "$FLOW" type-of-name-experimental complex_types.js AnimalClass --strip-root

printf "\n=== Exact and KeyMirror ===\n"
assert_ok "$FLOW" type-of-name-experimental complex_types.js ExactFromInexact --strip-root
assert_ok "$FLOW" type-of-name-experimental complex_types.js KeyMirrorExample --strip-root

printf "\n=== Intersection Types ===\n"
assert_ok "$FLOW" type-of-name-experimental complex_types.js Named --strip-root

printf "\n=== Tuple Types ===\n"
assert_ok "$FLOW" type-of-name-experimental complex_types.js Pair --strip-root
assert_ok "$FLOW" type-of-name-experimental complex_types.js LabeledTuple --strip-root
assert_ok "$FLOW" type-of-name-experimental complex_types.js ReadOnlyTuple --strip-root
assert_ok "$FLOW" type-of-name-experimental complex_types.js OptionalTuple --strip-root

printf "\n=== Promise Types ===\n"
assert_ok "$FLOW" type-of-name-experimental complex_types.js StringPromise --strip-root
assert_ok "$FLOW" type-of-name-experimental complex_types.js NestedPromise --strip-root

printf "\n=== Readonly Collections ===\n"
assert_ok "$FLOW" type-of-name-experimental complex_types.js ReadOnlyArray_t --strip-root
assert_ok "$FLOW" type-of-name-experimental complex_types.js ReadOnlyMap_t --strip-root
assert_ok "$FLOW" type-of-name-experimental complex_types.js ReadOnlySet_t --strip-root

printf "\n=== Interfaces ===\n"
assert_ok "$FLOW" type-of-name-experimental complex_types.js Serializable --strip-root
assert_ok "$FLOW" type-of-name-experimental complex_types.js Comparable --strip-root
assert_ok "$FLOW" type-of-name-experimental complex_types.js Printable --strip-root

printf "\n=== Opaque Types ===\n"
assert_ok "$FLOW" type-of-name-experimental complex_types.js UserID --strip-root
assert_ok "$FLOW" type-of-name-experimental complex_types.js Timestamp --strip-root

printf "\n=== Complex Generics ===\n"
assert_ok "$FLOW" type-of-name-experimental complex_types.js Result --strip-root
assert_ok "$FLOW" type-of-name-experimental complex_types.js TreeNode --strip-root
assert_ok "$FLOW" type-of-name-experimental complex_types.js Container --strip-root
assert_ok "$FLOW" type-of-name-experimental complex_types.js Animal --strip-root

printf "\n=== Mapped Types ===\n"
assert_ok "$FLOW" type-of-name-experimental complex_types.js MakeReadonly --strip-root
assert_ok "$FLOW" type-of-name-experimental complex_types.js MakeOptional --strip-root
assert_ok "$FLOW" type-of-name-experimental complex_types.js Methodify --strip-root
assert_ok "$FLOW" type-of-name-experimental complex_types.js Validators --strip-root

printf "\n=== Conditional Types ===\n"
assert_ok "$FLOW" type-of-name-experimental complex_types.js UnwrapPromise --strip-root
assert_ok "$FLOW" type-of-name-experimental complex_types.js UnwrapArray --strip-root
assert_ok "$FLOW" type-of-name-experimental complex_types.js IsString --strip-root

printf "\n=== Complex Function Types ===\n"
assert_ok "$FLOW" type-of-name-experimental complex_types.js Middleware --strip-root
assert_ok "$FLOW" type-of-name-experimental complex_types.js Callback --strip-root
assert_ok "$FLOW" type-of-name-experimental complex_types.js AsyncFn --strip-root

printf "\n=== Nested/Deep Object Types ===\n"
assert_ok "$FLOW" type-of-name-experimental complex_types.js DeepConfig --strip-root
assert_ok "$FLOW" type-of-name-experimental complex_types.js ExactObj --strip-root
assert_ok "$FLOW" type-of-name-experimental complex_types.js InexactObj --strip-root

printf "\n=== Component with Complex Refs ===\n"
assert_ok "$FLOW" type-of-name-experimental complex_types.js ComplexComponent --strip-root

printf "\n=== Misc Complex Types ===\n"
assert_ok "$FLOW" type-of-name-experimental complex_types.js Direction --strip-root
assert_ok "$FLOW" type-of-name-experimental complex_types.js DatabaseHost --strip-root
assert_ok "$FLOW" type-of-name-experimental complex_types.js CacheTTL --strip-root
assert_ok "$FLOW" type-of-name-experimental complex_types.js ConfigType --strip-root

printf "\n=== StringPrefix and StringSuffix ===\n"
assert_ok "$FLOW" type-of-name-experimental complex_types.js DataAttr --strip-root
assert_ok "$FLOW" type-of-name-experimental complex_types.js ExclaimStr --strip-root
assert_ok "$FLOW" type-of-name-experimental complex_types.js CSSVar --strip-root

printf "\n=== Mapped Types over Tuples ===\n"
assert_ok "$FLOW" type-of-name-experimental complex_types.js NumberTuple --strip-root
assert_ok "$FLOW" type-of-name-experimental complex_types.js StringifiedTuple --strip-root

printf "\n=== Complex Types from empty file (via index) ===\n"
assert_ok "$FLOW" type-of-name-experimental empty_file.js ReadOnlyUser --strip-root
assert_ok "$FLOW" type-of-name-experimental empty_file.js Result --strip-root
assert_ok "$FLOW" type-of-name-experimental empty_file.js ComplexComponent --strip-root --unexpand-component-props
assert_ok "$FLOW" type-of-name-experimental empty_file.js Container --strip-root

printf "\n=== File that does not exist ===\n"
# In a directory that does not exist either
assert_ok "$FLOW" type-of-name-experimental new/dir/new_file.js BasicComponent --strip-root --unexpand-component-props
# Next to existing files
assert_ok "$FLOW" type-of-name-experimental new_file.js myString --strip-root
# Batch and member lookups
assert_ok "$FLOW" type-of-name-experimental new/dir/new_file.js BasicComponent.title DocumentedComponent --strip-root --unexpand-component-props
# A misspelled path is a new empty file, so names local to basic.js are not found
assert_errors "$FLOW" type-of-name-experimental basci.js param --strip-root --exact-match-only

# Batch queries
printf "\n=== Batch Queries ===\n"
# Multiple names from same file, all found
assert_ok "$FLOW" type-of-name-experimental basic.js myString myNumber myBoolean --strip-root
# Multiple names, mixed found/not-found (exits non-zero due to error)
assert_errors "$FLOW" type-of-name-experimental basic.js myString nonexistent myNumber --strip-root
# Multiple names from empty file (index lookup)
assert_ok "$FLOW" type-of-name-experimental empty_file.js UserCard DataDisplay --strip-root --unexpand-component-props

# Member lookup
printf "\n=== Member Lookup ===\n"
# Component prop lookup (from current file)
assert_ok "$FLOW" type-of-name-experimental react.js BasicComponent.title --strip-root
assert_ok "$FLOW" type-of-name-experimental react.js BasicComponent.count --strip-root
# Object property lookup
assert_ok "$FLOW" type-of-name-experimental basic.js myObject.x --strip-root
assert_ok "$FLOW" type-of-name-experimental basic.js myObject.y --strip-root
# Class method/property lookup
assert_ok "$FLOW" type-of-name-experimental basic.js MyClass.methodName --strip-root
assert_ok "$FLOW" type-of-name-experimental basic.js instance.methodName --strip-root
# Type alias property lookup
assert_ok "$FLOW" type-of-name-experimental types.js UserCardProps.name --strip-root
# Nonexistent member
assert_errors "$FLOW" type-of-name-experimental react.js BasicComponent.nonexistent --strip-root
# Batch with members
assert_ok "$FLOW" type-of-name-experimental react.js BasicComponent.title BasicComponent.count --strip-root
# Member from index lookup (empty file)
assert_ok "$FLOW" type-of-name-experimental empty_file.js BasicComponent.title --strip-root
# Member with prop-level documentation
assert_ok "$FLOW" type-of-name-experimental react.js DocumentedComponent.displayName --strip-root
assert_ok "$FLOW" type-of-name-experimental react.js DocumentedComponent.userId --strip-root
