#!/bin/bash
# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under the MIT license found in the
# LICENSE file in the root directory of this source tree.

# shellcheck disable=SC2094

queries_in_file autocomplete "member_class_extends_constructor.js" --pretty
queries_in_file autocomplete "member_class_extends_constructor_static.js" --pretty
queries_in_file autocomplete "member_class_extends_intersection.js" --pretty
queries_in_file autocomplete "member_class_extends_mixin.js" --pretty
queries_in_file autocomplete "member_class_property.js" --lsp
queries_in_file autocomplete "member_class_static.js" --pretty
queries_in_file autocomplete "member_interface_property.js" --lsp
queries_in_file autocomplete "member_middle.js" --pretty
queries_in_file autocomplete "method_signature.js" --lsp
queries_in_file autocomplete "namespace.js" --pretty
queries_in_file autocomplete "normalize-1.js" --pretty
queries_in_file autocomplete "normalize-2.js" --pretty
queries_in_file autocomplete "num.js" --pretty
queries_in_file autocomplete "object_builtins.js" --pretty
queries_in_file autocomplete "object_keys_spread.js" --pretty
queries_in_file autocomplete "object_literal.js" --pretty
queries_in_file autocomplete "object-spread-1.js" --pretty
queries_in_file autocomplete "object-spread-2.js" --pretty
queries_in_file autocomplete "opaque_types.js" --pretty
queries_in_file autocomplete "optional_chaining_continue.js" --pretty
queries_in_file autocomplete "optional_chaining_middle.js" --pretty
queries_in_file autocomplete "optional_chaining_new.js" --pretty
queries_in_file autocomplete "optional.js" --pretty
queries_in_file autocomplete "optional_object.js" --pretty
queries_in_file autocomplete "override.js" --pretty
queries_in_file autocomplete "pattern.js" --pretty
queries_in_file autocomplete "pattern_object_key_middle.js" --pretty
queries_in_file autocomplete "poly.js" --pretty
queries_in_file autocomplete "poly_no_args.js" --pretty
queries_in_file autocomplete "qualified-type-annotation.js" --pretty
queries_in_file autocomplete "qualified-type-annotation-require.js" --pretty
queries_in_file autocomplete "qux.js" --pretty
