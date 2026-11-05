/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

//! Schema-driven node definitions for the Flow WASM parser binary protocol.
//!
//! This file is the **single source of truth** for the node kinds used in the
//! binary serialization format between the Rust WASM parser and the JS
//! deserializer (`FlowParserNodeDeserializers.js`).
//!
//! # Architecture
//!
//! The `define_nodes!` macro generates:
//! - `NodeKind` enum — used by `serializer.rs` to tag each node in the binary buffer
//! - `SCHEMA` array — used by the codegen binary to generate the JS deserializer
//! - `SERIALIZE_SCHEMA` array — used by the codegen binary to generate
//!   `serializer_dispatch.rs` (the Rust dispatch code)
//!
//! This means adding a new AST node only requires editing this file — both the
//! JS deserializer and the Rust serializer dispatch are generated from it.
//!
//! # Adding a new AST node
//!
//! 1. Add an entry to the `define_nodes!` invocation below. Use the next
//!    available ID (currently 279+). List properties in the exact order the
//!    serializer will write them.
//!
//!    For nodes in the top-level dispatch (Statement/Expression/Type), add a
//!    `from` clause describing the AST-to-wire-format mapping:
//!
//!    ```ignore
//!    // Leaf (no properties):
//!    EmptyStatement = 0 {}
//!        from Statement::Empty { loc, .. } {},
//!
//!    // Mechanical (header + expressions):
//!    ExpressionStatement = 1 { expression: Node, directive: String }
//!        from Statement::Expression { loc, inner } {
//!            self.serialize_expression(&inner.expression),
//!            self.write_str_opt(inner.directive.as_deref()),
//!        },
//!
//!    // Delegate (hand-written helper, no header write by codegen):
//!    ClassDeclaration = 22 { ... }
//!        from Statement::ClassDeclaration { loc, inner }
//!        {=> self.serialize_class(loc, inner, NodeKind::ClassDeclaration)},
//!    ```
//!
//! 2. Regenerate the JS deserializer and Rust dispatch:
//!
//!    ```sh
//!    buck run fbcode//flow/rust_port/crates/flow_parser_wasm:codegen > \
//!      fbcode/flow/packages/flow-parser/oxidized-src/FlowParserNodeDeserializers.js
//!    buck run fbcode//flow/rust_port/crates/flow_parser_wasm:codegen -- --rust > \
//!      fbcode/flow/rust_port/crates/flow_parser_wasm/src/serializer_dispatch.rs
//!    ```
//!
//! 3. Build and test:
//!
//!    ```sh
//!    buck test fbcode//flow/rust_port/crates/flow_parser_wasm:flow_parser_wasm
//!    ```
//!
//! # Property types
//!
//! | Schema type | Serializer method        | JS deserializer method         |
//! |-------------|--------------------------|--------------------------------|
//! | `Node`      | `serialize_*` / `write_node_header` | `this.deserializeNode()`  |
//! | `NodeList`  | `buf.push(len)` + loop   | `this.deserializeNodeList()`   |
//! | `String`    | `write_str` / `write_str_opt` | `this.deserializeString()` |
//! | `Boolean`   | `write_bool`             | `this.deserializeBoolean()`    |
//! | `Number`    | `write_number`           | `this.deserializeNumber()`     |

/// Property types for the schema — correspond to JS deserialization methods.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PropType {
    /// `this.deserializeNode()`
    Node,
    /// `this.deserializeNodeList()`
    NodeList,
    /// `this.deserializeString()`
    String,
    /// `this.deserializeBoolean()`
    Boolean,
    /// `this.deserializeNumber()`
    Number,
    /// Read a node slot and omit the JS property when it is null.
    OptionalNode,
    /// Read a boolean slot and omit the JS property when it is false.
    TrueBoolean,
    /// Read a node list and omit the JS property when it is empty.
    NonEmptyNodeList,
    /// Read a string slot and omit the JS property when it is null. Used for
    /// Babel-shaped nodes whose alignment baseline (the pinned `@babel/parser`)
    /// predates the property.
    MaybeString,
    /// Read a presence bit followed by a node slot, assigning even when null.
    MaybeNode,
    /// Read a presence bit followed by a boolean value.
    MaybeBoolean,
    /// `this.deserializeComments()`
    CommentList,
}

/// A single node definition: kind ID, ESTree type name, and property schema.
#[derive(Debug)]
pub struct NodeDef {
    pub kind_id: u32,
    pub name: &'static str,
    /// ESTree `type` field emitted by the JS deserializer.
    /// Defaults to `name`, but may differ for nodes that share an ESTree
    /// type (e.g. `Statement::ComponentDeclaration` with body=None and
    /// `Statement::DeclareComponent` both serialize to ESTree
    /// "DeclareComponent" but with different shapes).
    pub estree_type: &'static str,
    pub properties: &'static [(&'static str, PropType)],
    /// Properties emitted BEFORE the schema properties on the JS side
    /// (and consumed first off the wire). Used for nodes whose binary
    /// shape includes a leading field that can't be expressed inline in
    /// `properties` because the field name is a Rust reserved keyword.
    /// e.g. ConstructorTypeAnnotation's leading `abstract` Boolean.
    pub extra_pre_props: &'static [(&'static str, PropType)],
    /// Properties emitted AFTER the schema properties on the JS side.
    /// e.g. ForOfStatement's trailing `await` Boolean.
    pub extra_post_props: &'static [(&'static str, PropType)],
    /// Names of properties that should be nested inside a `value: { ... }`
    /// object in the JS deserializer output. Used by TemplateElement,
    /// whose ESTree shape wraps `cooked`/`raw` in a `value` object even
    /// though the binary wire format keeps them flat.
    pub nested_object_props: &'static [&'static str],
    /// If true, the JS deserializer for this node is hand-written rather
    /// than generated mechanically. Used by Literal, whose discriminant-
    /// based encoding can't be expressed as a flat property list.
    pub custom_emit: bool,
}

/// Dispatch category for serialize schema — determines which top-level
/// dispatch method (and inner enum) a `from` clause belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DispatchType {
    Statement,
    Expression,
    Type,
}

/// Body of a `from` clause in `define_nodes!`.
#[derive(Debug)]
pub enum SerializeBody {
    /// Leaf node: just write the header (no property expressions).
    Leaf,
    /// Mechanical: write the header, then execute each expression in order
    /// (one per property slot). Each `&str` is a stringified Rust expression.
    Mechanical(&'static [&'static str]),
    /// Delegate: call a hand-written helper (no header write by codegen).
    /// The `&str` is the stringified delegate expression.
    Delegate(&'static str),
}

/// Describes how one AST variant maps to a node kind in the binary protocol.
/// Generated by `from` clauses in `define_nodes!`, consumed by the codegen
/// binary to generate `serializer_dispatch.rs`.
#[derive(Debug)]
pub struct SerializeMapping {
    pub node_kind_name: &'static str,
    pub dispatch: DispatchType,
    pub variant: &'static str,
    pub bindings: &'static str,
    pub body: SerializeBody,
}

/// Helper macro: classifies a `from` clause body into a [`SerializeBody`] variant.
///
/// - Empty body `()` → `Leaf` (just write the node header)
/// - `=> delegate_expr` → `Delegate` (call a hand-written helper, no header write)
/// - `expr1, expr2, ...` → `Mechanical` (write header, then each expression)
macro_rules! define_serialize_body {
    () => {
        SerializeBody::Leaf
    };
    (=> $($delegate:tt)+) => {
        SerializeBody::Delegate(stringify!($($delegate)+))
    };
    ($($e:expr),+ $(,)?) => {
        SerializeBody::Mechanical(&[$(stringify!($e)),+])
    };
}

/// Helper macro: pick the optional `as "EsTreeName"` literal if present,
/// otherwise fall back to the stringified NodeKind name.
macro_rules! pick_estree_type {
    ($name:ident,) => {
        stringify!($name)
    };
    ($name:ident, $estree:literal) => {
        $estree
    };
}

/// Helper macro: build a `&[(&str, PropType)]` slice from a (possibly empty)
/// list of `name: Type` pairs. Used for `extra_pre_props` and
/// `extra_post_props`.
macro_rules! prop_list {
    () => {
        &[]
    };
    ( $( $prop:ident : $ty:ident ),+ $(,)? ) => {
        &[ $( (stringify!($prop), PropType::$ty), )+ ]
    };
}

/// Helper macro: produce `true` if any tokens are passed, otherwise `false`.
/// Used to derive `custom_emit` from the optional `custom` marker in
/// `define_nodes!`.
macro_rules! has_marker {
    () => {
        false
    };
    ( $($_marker:tt)+ ) => {
        true
    };
}

/// Defines the `NodeKind` enum AND the `SCHEMA` metadata array from a single source of truth.
///
/// Usage:
/// ```ignore
/// define_nodes! {
///     EmptyStatement = 0 {},
///     ExpressionStatement = 1 {
///         expression: Node,
///         directive: String,
///     },
/// }
/// ```
///
/// Optional clauses on each node:
/// - `as "EsTreeName"` — override the JS `type` field (default: NodeKind name).
/// - `pre { foo: Boolean }` — extra properties emitted BEFORE schema props on
///   the JS side. Used for fields whose name is a Rust reserved keyword.
/// - `post { bar: Boolean }` — extra properties emitted AFTER schema props.
/// - `nested value { cooked: String, raw: String }` — listed properties are
///   nested under a `value: { ... }` object in the JS deserializer output.
/// - `custom { }` — opt out of mechanical JS emission entirely. Codegen
///   routes to a per-kind hand-written deserializer. (Empty braces are
///   required to disambiguate from the following `from` clause.)
/// - `from Path::Variant { bindings } { body }` — emit a serializer dispatch
///   arm. May appear multiple times for nodes with multiple AST variants.
///
/// Expands to:
/// - `pub enum NodeKind { EmptyStatement = 0, ExpressionStatement = 1, ... }` with `#[repr(u32)]`
/// - `pub const SCHEMA: &[NodeDef]` containing property metadata
/// - `pub const SERIALIZE_SCHEMA: &[SerializeMapping]` containing dispatch metadata
/// - `NodeKind::COUNT` — total count validated at compile time
macro_rules! define_nodes {
    (
        $(
            $name:ident = $id:literal
            $( as $estree:literal )?
            { $( $prop:ident : $ty:ident ),* $(,)? }
            $( pre { $( $pre_prop:ident : $pre_ty:ident ),* $(,)? } )?
            $( post { $( $post_prop:ident : $post_ty:ident ),* $(,)? } )?
            $( nested value { $( $nested_prop:ident : $nested_ty:ident ),* $(,)? } )?
            $( custom { $($custom_emit_marker:tt)* } )?
            $(
                from $dispatch:ident :: $variant:ident { $($bindings:tt)* } { $($body:tt)* }
            )*
        ),*
        $(,)?
    ) => {
        /// Node kinds for binary serialization, indexed from 0.
        /// These are generated by the `define_nodes!` macro.
        ///
        /// IMPORTANT: Never reorder or remove entries — only append new ones.
        ///
        /// In the programBuffer, nodes are encoded as (nodeKind + 1), reserving 0 for null.
        /// On the JS side, NODE_DESERIALIZERS[nodeKind] gives the deserializer function.
        #[repr(u32)]
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub enum NodeKind {
            $( $name = $id, )*
        }

        impl NodeKind {
            /// Total number of node kinds.
            pub const COUNT: usize = {
                let ids: &[u32] = &[ $( $id, )* ];
                ids.len()
            };
        }

        /// Schema metadata for all node kinds, in order of kind ID.
        ///
        /// Each entry describes the ESTree type name and the properties that the JS
        /// deserializer must read (in exact order).
        ///
        /// The Literal node (75) has `custom_emit = true` and empty `properties`
        /// because its encoding uses a discriminant-based format that can't be
        /// expressed as a flat property list.
        pub const SCHEMA: &[NodeDef] = &[
            $(
                NodeDef {
                    kind_id: $id,
                    name: stringify!($name),
                    // Optional `as "EsTreeName"` overrides the JS `type` field.
                    // Used when two NodeKinds (with different binary shapes)
                    // serialize to the same ESTree type — e.g. the ambient
                    // `Statement::ComponentDeclaration` (body=None, full shape)
                    // vs. `Statement::DeclareComponent` (short shape) which
                    // both emit ESTree "DeclareComponent". Defaults to the
                    // NodeKind name when not specified.
                    estree_type: pick_estree_type!($name, $($estree)?),
                    properties: &[
                        $( (stringify!($prop), PropType::$ty), )*
                    ],
                    extra_pre_props: prop_list!( $( $( $pre_prop : $pre_ty ),* )? ),
                    extra_post_props: prop_list!( $( $( $post_prop : $post_ty ),* )? ),
                    nested_object_props: &[
                        $( $( stringify!($nested_prop), )* )?
                    ],
                    custom_emit: has_marker!( $( custom $($custom_emit_marker)* )? ),
                },
            )*
        ];

        /// Serialization dispatch schema — maps AST variants to node kinds.
        ///
        /// Generated from `from` clauses in `define_nodes!`. Used by the codegen
        /// binary to generate `serializer_dispatch.rs`.
        pub const SERIALIZE_SCHEMA: &[SerializeMapping] = &[
            $(
                $(
                    SerializeMapping {
                        node_kind_name: stringify!($name),
                        dispatch: DispatchType::$dispatch,
                        variant: stringify!($variant),
                        bindings: stringify!($($bindings)*),
                        body: define_serialize_body!($($body)*),
                    },
                )*
            )*
        ];
    };
}

define_nodes! {
    // ---------------------------------------------------------------
    // Statements
    // ---------------------------------------------------------------
    EmptyStatement = 0 {}
        from Statement::Empty { loc, .. } {},
    ExpressionStatement = 1 {
        expression: Node,
        directive: String,
    } from Statement::Expression { loc, inner }
        {=> self.serialize_expression_statement(loc, inner)},
    BlockStatement = 2 {
        body: NodeList,
    } from Statement::Block { loc, inner }
        {=> self.serialize_block_statement(loc, inner)},
    IfStatement = 3 {
        test: Node,
        consequent: Node,
        alternate: Node,
    } from Statement::If { loc, inner } {
            self.serialize_expression(&inner.test),
            self.serialize_statement(&inner.consequent),
            match &inner.alternate {
                Some(alt) => self.serialize_statement(&alt.body),
                None => self.write_null_node(),
            },
        },
    LabeledStatement = 4 {
        label: Node,
        body: Node,
    } from Statement::Labeled { loc, inner } {
            self.serialize_identifier_node(&inner.label),
            self.serialize_statement(&inner.body),
        },
    BreakStatement = 5 {
        label: Node,
    } from Statement::Break { loc, inner } {
            match &inner.label {
                Some(id) => self.serialize_identifier_node(id),
                None => self.write_null_node(),
            },
        },
    ContinueStatement = 6 {
        label: Node,
    } from Statement::Continue { loc, inner } {
            match &inner.label {
                Some(id) => self.serialize_identifier_node(id),
                None => self.write_null_node(),
            },
        },
    WithStatement = 7 {
        object: Node,
        body: Node,
    } from Statement::With { loc, inner } {
            self.serialize_expression(&inner.object),
            self.serialize_statement(&inner.body),
        },
    SwitchStatement = 8 {
        discriminant: Node,
        cases: NodeList,
    } from Statement::Switch { loc, inner } {
            self.serialize_expression(&inner.discriminant),
            {
                self.buf.push(inner.cases.len() as u32);
                for case in inner.cases.iter() {
                    self.serialize_switch_case(case);
                }
            },
        },
    ReturnStatement = 9 {
        argument: Node,
    } from Statement::Return { loc, inner } {
            match &inner.argument {
                Some(expr) => self.serialize_expression(expr),
                None => self.write_null_node(),
            },
        },
    ThrowStatement = 10 {
        argument: Node,
    } from Statement::Throw { loc, inner } {
            self.serialize_expression(&inner.argument),
        },
    TryStatement = 11 {
        block: Node,
        handler: Node,
        finalizer: Node,
    } from Statement::Try { loc, inner } {
            self.serialize_block_statement(&inner.block.0, &inner.block.1),
            match &inner.handler {
                Some(catch) => self.serialize_catch_clause(catch),
                None => self.write_null_node(),
            },
            match &inner.finalizer {
                Some((floc, fblock)) => self.serialize_block_statement(floc, fblock),
                None => self.write_null_node(),
            },
        },
    WhileStatement = 12 {
        body: Node,
        test: Node,
    } from Statement::While { loc, inner } {
            self.serialize_statement(&inner.body),
            self.serialize_expression(&inner.test),
        },
    DoWhileStatement = 13 {
        body: Node,
        test: Node,
    } from Statement::DoWhile { loc, inner } {
            self.serialize_statement(&inner.body),
            self.serialize_expression(&inner.test),
        },
    ForStatement = 14 {
        init: Node,
        test: Node,
        update: Node,
        body: Node,
    } from Statement::For { loc, inner } {
            self.serialize_for_init(&inner.init),
            match &inner.test {
                Some(expr) => self.serialize_expression(expr),
                None => self.write_null_node(),
            },
            match &inner.update {
                Some(expr) => self.serialize_expression(expr),
                None => self.write_null_node(),
            },
            self.serialize_statement(&inner.body),
        },
    ForInStatement = 15 {
        left: Node,
        right: Node,
        body: Node,
    } from Statement::ForIn { loc, inner } {
            self.serialize_for_in_left(&inner.left),
            self.serialize_expression(&inner.right),
            self.serialize_statement(&inner.body),
        },
    // ForOfStatement has a trailing `await` Boolean that's a JS keyword;
    // it can be a JS object key but not a Rust ident inside the schema
    // properties block. We declare it via `post { await: Boolean }` so
    // the JS deserializer reads it after the schema fields.
    ForOfStatement = 16 {
        left: Node,
        right: Node,
        body: Node,
    } post { await: Boolean }
        from Statement::ForOf { loc, inner } {
            self.serialize_for_of_left(&inner.left),
            self.serialize_expression(&inner.right),
            self.serialize_statement(&inner.body),
            self.write_bool(inner.await_),
        },
    DebuggerStatement = 17 {}
        from Statement::Debugger { loc, .. } {},
    MatchStatement = 18 {
        argument: Node,
        cases: NodeList,
    } from Statement::Match { loc, inner } {
            self.serialize_expression(&inner.arg),
            {
                self.buf.push(inner.cases.len() as u32);
                for case in inner.cases.iter() {
                    self.serialize_match_statement_case(case);
                }
            },
        },

    // ---------------------------------------------------------------
    // Declarations
    // ---------------------------------------------------------------
    FunctionDeclaration = 19 {
        id: Node,
        params: NodeList,
        body: Node,
        typeParameters: Node,
        returnType: Node,
        generator: Boolean,
        async: Boolean,
        predicate: Node,
        expression: Boolean,
    } from Statement::FunctionDeclaration { loc, inner }
        {=> self.serialize_function_decl(loc, inner, NodeKind::FunctionDeclaration)},
    VariableDeclaration = 20 {
        kind: String,
        declarations: NodeList,
    } from Statement::VariableDeclaration { loc, inner }
        {=> self.serialize_variable_declaration(loc, inner)},
    VariableDeclarator = 21 {
        init: Node,
        id: Node,
    },
    ClassDeclaration = 22 {
        id: Node,
        typeParameters: Node,
        superClass: Node,
        implements: NodeList,
        body: Node,
        superTypeArguments: Node,
        decorators: NodeList,
        abstract: TrueBoolean,
    } from Statement::ClassDeclaration { loc, inner }
        {=> self.serialize_class(loc, inner, NodeKind::ClassDeclaration)},
    ComponentDeclaration = 23 {
        body: Node,
        id: Node,
        implicitDeclare: TrueBoolean,
        params: NodeList,
        rendersType: Node,
        typeParameters: Node,
        async: Boolean,
    } from Statement::ComponentDeclaration { loc, inner }
        {=> self.serialize_component_declaration(loc, inner)},
    HookDeclaration = 24 {
        id: Node,
        params: NodeList,
        body: Node,
        returnType: Node,
        typeParameters: Node,
        async: Boolean,
    },
    EnumDeclaration = 25 {
        id: Node,
        body: Node,
    } post { const: TrueBoolean }
        from Statement::EnumDeclaration { loc, inner }
        {=> self.serialize_enum_declaration(loc, inner)},
    InterfaceDeclaration = 26 {
        id: Node,
        typeParameters: Node,
        extends: NodeList,
        body: Node,
    } from Statement::InterfaceDeclaration { loc, inner }
        {=> self.serialize_interface_declaration(loc, inner)},
    TypeAlias = 27 {
        id: Node,
        typeParameters: Node,
        right: Node,
    } from Statement::TypeAlias { loc, inner } {
            self.serialize_identifier_node(&inner.id),
            self.serialize_type_params_opt(&inner.tparams),
            self.serialize_type(&inner.right),
        },
    RecordDeclaration = 28 {
        id: Node,
        typeParameters: Node,
        implements: NodeList,
        body: Node,
    } from Statement::RecordDeclaration { loc, inner }
        {=> self.serialize_record_declaration(loc, inner)},

    // ---------------------------------------------------------------
    // Import / Export
    // ---------------------------------------------------------------
    ImportDeclaration = 29 {
        specifiers: NodeList,
        source: Node,
        importKind: String,
        attributes: NodeList,
        phase: String,
    } from Statement::ImportDeclaration { loc, inner }
        {=> self.serialize_import_declaration(loc, inner)},
    ImportDefaultSpecifier = 30 {
        local: Node,
    },
    ImportNamespaceSpecifier = 31 {
        local: Node,
    },
    ImportSpecifier = 32 {
        imported: Node,
        local: Node,
        importKind: String,
    },
    ImportAttribute = 33 {
        key: Node,
        value: Node,
    },
    ImportEqualsDeclaration = 34 {
        id: Node,
        moduleReference: Node,
        importKind: String,
        isExport: Boolean,
    } from Statement::ImportEqualsDeclaration { loc, inner }
        {=> self.serialize_import_equals_declaration(loc, inner)},
    ExternalModuleReference = 35 {
        expression: Node,
    },
    ExportNamedDeclaration = 36 {
        declaration: Node,
        specifiers: NodeList,
        source: Node,
        exportKind: String,
        attributes: NonEmptyNodeList,
    } from Statement::ExportNamedDeclaration { loc, inner }
        {=> self.serialize_export_named_declaration(loc, inner)},
    ExportDefaultDeclaration = 37 {
        declaration: Node,
        exportKind: MaybeString,
    } from Statement::ExportDefaultDeclaration { loc, inner }
        {=> self.serialize_export_default_declaration(loc, inner)},
    ExportAllDeclaration = 38 {
        source: Node,
        exported: Node,
        exportKind: String,
        attributes: NonEmptyNodeList,
    },
    ExportSpecifier = 39 {
        exported: Node,
        local: Node,
        exportKind: String,
    },
    ExportNamespaceSpecifier = 40 {
        exported: Node,
    },
    ExportAssignment = 41 {
        expression: Node,
    } from Statement::ExportAssignment { loc, inner }
        {=> self.serialize_export_assignment(loc, inner)},

    // ---------------------------------------------------------------
    // Expressions
    // ---------------------------------------------------------------
    ThisExpression = 42 {}
        from Expression::This { loc, .. } {},
    Super = 43 {}
        from Expression::Super { loc, .. } {},
    ArrayExpression = 44 {
        elements: NodeList,
        trailingComma: Boolean,
    } from Expression::Array { loc, inner }
        {=> self.serialize_array_expression(loc, inner)},
    ObjectExpression = 45 {
        properties: NodeList,
    } from Expression::Object { loc, inner }
        {=> self.serialize_object_expression(loc, inner)},
    FunctionExpression = 46 {
        id: Node,
        params: NodeList,
        body: Node,
        typeParameters: Node,
        returnType: Node,
        generator: Boolean,
        async: Boolean,
        predicate: Node,
        expression: Boolean,
    } from Expression::Function { loc, inner }
        {=> self.serialize_function_expr(loc, inner, NodeKind::FunctionExpression)},
    ArrowFunctionExpression = 47 {
        params: NodeList,
        body: Node,
        typeParameters: Node,
        returnType: Node,
        generator: Boolean,
        async: Boolean,
        id: Node,
        predicate: Node,
        expression: Boolean,
    } from Expression::ArrowFunction { loc, inner }
        {=> self.serialize_function_expr(loc, inner, NodeKind::ArrowFunctionExpression)},
    SequenceExpression = 48 {
        expressions: NodeList,
    } from Expression::Sequence { loc, inner }
        {=> self.serialize_sequence_expression(loc, inner)},
    UnaryExpression = 49 {
        operator: String,
        argument: Node,
        prefix: Boolean,
    } from Expression::Unary { loc, inner }
        {=> self.serialize_unary_dispatch(loc, inner)},
    BinaryExpression = 50 {
        left: Node,
        right: Node,
        operator: String,
    } from Expression::Binary { loc, inner } {
            self.serialize_expression(&inner.left),
            self.serialize_expression(&inner.right),
            self.write_str(inner.operator.as_str()),
        },
    LogicalExpression = 51 {
        left: Node,
        right: Node,
        operator: String,
    } from Expression::Logical { loc, inner } {
            self.serialize_expression(&inner.left),
            self.serialize_expression(&inner.right),
            self.write_str(match inner.operator {
                ast::expression::LogicalOperator::Or => "||",
                ast::expression::LogicalOperator::And => "&&",
                ast::expression::LogicalOperator::NullishCoalesce => "??",
            }),
        },
    ConditionalExpression = 52 {
        test: Node,
        alternate: Node,
        consequent: Node,
    } from Expression::Conditional { loc, inner } {
            self.serialize_expression(&inner.test),
            self.serialize_expression(&inner.alternate),
            self.serialize_expression(&inner.consequent),
        },
    UpdateExpression = 53 {
        operator: String,
        argument: Node,
        prefix: Boolean,
    } from Expression::Update { loc, inner } {
            self.write_str(match inner.operator {
                ast::expression::UpdateOperator::Increment => "++",
                ast::expression::UpdateOperator::Decrement => "--",
            }),
            self.serialize_expression(&inner.argument),
            self.write_bool(inner.prefix),
        },
    AssignmentExpression = 54 {
        operator: String,
        left: Node,
        right: Node,
    } from Expression::Assignment { loc, inner } {
            self.write_str(match &inner.operator {
                Some(op) => op.as_str(),
                None => "=",
            }),
            self.serialize_pattern(&inner.left),
            self.serialize_expression(&inner.right),
        },
    MemberExpression = 55 {
        object: Node,
        property: Node,
        computed: Boolean,
        optional: Boolean,
    } from Expression::Member { loc, inner }
        {=> self.serialize_member_expression(loc, inner)},
    // The wire never emits OptionalMemberExpression (kind 56): the optional-
    // chain rewrite in `serialize_expression` (mirroring upstream Hermes'
    // mapChainExpression) emits MemberExpression (with `optional: true`)
    // wrapped in a single ChainExpression at the chain root. The from-clause
    // here exists only to keep the auto-generated dispatch exhaustive on the
    // AST enum; the dispatch path is unreachable in practice because
    // `serialize_expression` intercepts OptionalMember before calling
    // dispatch. The schema fields stay so codegen artifacts (visitor keys,
    // predicates, selectors, types) remain byte-aligned with upstream.
    OptionalMemberExpression = 56 {
        object: Node,
        property: Node,
        computed: Boolean,
        optional: Boolean,
    } from Expression::OptionalMember { loc, inner }
        {=> self.serialize_optional_member_as_member_expression(loc, inner)},
    CallExpression = 57 {
        callee: Node,
        typeArguments: Node,
        arguments: NodeList,
        optional: Boolean,
    } from Expression::Call { loc, inner }
        {=> self.serialize_call_expression(loc, inner)},
    // OptionalCallExpression: same as OptionalMemberExpression — the wire
    // never emits this kind; serializer routes through the chain-rewrite
    // path. The schema stays for codegen artifact parity.
    OptionalCallExpression = 58 {
        callee: Node,
        typeArguments: Node,
        arguments: NodeList,
        optional: Boolean,
    } from Expression::OptionalCall { loc, inner }
        {=> self.serialize_optional_call_as_call_expression(loc, inner)},
    NewExpression = 59 {
        callee: Node,
        typeArguments: Node,
        arguments: NodeList,
    } from Expression::New { loc, inner } {
            self.serialize_expression(&inner.callee),
            self.serialize_call_type_args_opt(&inner.targs),
            match &inner.arguments {
                Some(args) => self.serialize_arg_list(args),
                None => self.buf.push(0),
            },
        },
    YieldExpression = 60 {
        argument: Node,
        delegate: Boolean,
    } from Expression::Yield { loc, inner } {
            match &inner.argument {
                Some(expr) => self.serialize_expression(expr),
                None => self.write_null_node(),
            },
            self.write_bool(inner.delegate),
        },
    AwaitExpression = 61 {
        argument: Node,
    },
    ImportExpression = 62 {
        source: Node,
        options: Node,
        phase: String,
    } from Expression::Import { loc, inner }
        {=> self.serialize_import_expression(loc, inner)},
    MetaProperty = 63 {
        meta: Node,
        property: Node,
    } from Expression::MetaProperty { loc, inner } {
            self.serialize_identifier_node(&inner.meta),
            self.serialize_identifier_node(&inner.property),
        },
    TaggedTemplateExpression = 64 {
        tag: Node,
        quasi: Node,
        typeArguments: MaybeNode,
    } from Expression::TaggedTemplate { loc, inner } {
            self.serialize_expression(&inner.tag),
            self.serialize_template_literal(&inner.quasi.0, &inner.quasi.1),
            self.serialize_tagged_template_type_arguments(&inner.targs),
        },
    TemplateLiteral = 65 {
        quasis: NodeList,
        expressions: NodeList,
    } from Expression::TemplateLiteral { loc, inner }
        {=> self.serialize_template_literal(loc, inner)},
    // TemplateElement's `cooked`/`raw` fields are nested under a
    // `value: { cooked, raw }` object in the ESTree shape, even though
    // the binary wire format keeps them flat. We declare the nesting
    // via `nested value { cooked: String, raw: String }`.
    TemplateElement = 66 {
        tail: Boolean,
        cooked: String,
        raw: String,
    } nested value {
        cooked: String,
        raw: String,
    },
    TypeCastExpression = 67 {
        expression: Node,
        typeAnnotation: Node,
    } from Expression::TypeCast { loc, inner }
        {=> self.serialize_type_cast_expression(loc, inner)},
    AsExpression = 68 {
        expression: Node,
        typeAnnotation: Node,
    } from Expression::AsExpression { loc, inner }
        {=> self.serialize_as_expression(loc, inner)},
    SatisfiesExpression = 69 {
        expression: Node,
        typeAnnotation: Node,
    } from Expression::TSSatisfies { loc, inner } {
            self.serialize_expression(&inner.expression),
            self.serialize_type(&inner.annot.annotation),
        },
    AsConstExpression = 70 {
        expression: Node,
    } from Expression::AsConstExpression { loc, inner }
        {=> self.serialize_as_const_expression(loc, inner)},
    NonNullExpression = 71 {
        expression: Node,
    },
    MatchExpression = 72 {
        argument: Node,
        cases: NodeList,
    } from Expression::Match { loc, inner }
        {=> self.serialize_match_expression(loc, inner)},
    RecordExpression = 73 {
        recordConstructor: Node,
        typeArguments: Node,
        properties: Node,
    } from Expression::Record { loc, inner }
        {=> self.serialize_record_expression(loc, inner)},
    RecordExpressionProperties = 74 {
        properties: NodeList,
    },

    // ---------------------------------------------------------------
    // Literals
    // ---------------------------------------------------------------
    // Literal has custom encoding (discriminant-based), so its schema
    // properties are empty. The `custom { }` marker tells codegen to route
    // JS deserialization to a hand-written per-kind handler.
    Literal = 75 {} custom { }
        from Expression::NullLiteral { loc, .. }
            {=> self.write_null_literal(loc)}
        from Expression::BooleanLiteral { loc, inner }
            {=> self.write_boolean_literal(loc, inner.value)}
        from Expression::NumberLiteral { loc, inner }
            {=> self.write_number_literal(loc, inner.value, &inner.raw)}
        from Expression::StringLiteral { loc, inner }
            {=> self.write_string_literal(loc, &inner.value, &inner.raw)}
        from Expression::BigIntLiteral { loc, inner }
            {=> self.write_bigint_literal(loc, &inner.raw)}
        from Expression::RegExpLiteral { loc, inner }
            {=> self.write_regex_literal(loc, &inner.raw, &inner.pattern, &inner.flags)}
        from Expression::ModuleRefLiteral { loc, inner }
            {=> self.write_string_literal(loc, &inner.value, &inner.raw)},
    Identifier = 76 {
        name: String,
        typeAnnotation: Node,
        optional: Boolean,
    } from Expression::Identifier { inner, .. }
        {=> self.serialize_identifier_node(inner)},
    PrivateIdentifier = 77 {
        name: String,
        typeAnnotation: Node,
        optional: Boolean,
    },

    // ---------------------------------------------------------------
    // Patterns
    // ---------------------------------------------------------------
    ObjectPattern = 78 {
        properties: NodeList,
        typeAnnotation: Node,
        optional: TrueBoolean,
    },
    ArrayPattern = 79 {
        elements: NodeList,
        typeAnnotation: Node,
        optional: TrueBoolean,
    },
    RestElement = 80 {
        argument: Node,
    },
    AssignmentPattern = 81 {
        left: Node,
        right: Node,
    },

    // ---------------------------------------------------------------
    // Properties / Elements
    // ---------------------------------------------------------------
    Property = 82 {
        key: Node,
        value: Node,
        kind: String,
        method: Boolean,
        shorthand: Boolean,
        computed: Boolean,
    },
    SpreadElement = 83 {
        argument: Node,
    },

    // ---------------------------------------------------------------
    // Class-related
    // ---------------------------------------------------------------
    ClassExpression = 84 {
        id: Node,
        typeParameters: Node,
        superClass: Node,
        implements: NodeList,
        body: Node,
        superTypeArguments: Node,
        decorators: NodeList,
        abstract: TrueBoolean,
    } from Expression::Class { loc, inner }
        {=> self.serialize_class(loc, inner, NodeKind::ClassExpression)},
    ClassBody = 85 {
        body: NodeList,
    },
    ClassImplements = 86 {
        id: Node,
        typeArguments: Node,
    },
    MethodDefinition = 87 {
        key: Node,
        value: Node,
        kind: String,
        static: Boolean,
        computed: Boolean,
        decorators: NodeList,
        override: Boolean,
        tsAccessibility: String,
    },
    PropertyDefinition = 88 {
        key: Node,
        value: Node,
        typeAnnotation: Node,
        computed: Boolean,
        static: Boolean,
        variance: Node,
        tsAccessibility: String,
        declare: Boolean,
        optional: Boolean,
        override: Boolean,
        decorators: NodeList,
    },
    StaticBlock = 89 {
        body: NodeList,
    },
    Decorator = 90 {
        expression: Node,
    },
    ParameterProperty = 91 {
        key: Node,
        value: Node,
        typeAnnotation: Node,
        computed: Boolean,
        static: Boolean,
        variance: Node,
        tsAccessibility: String,
        declare: Boolean,
        optional: Boolean,
        decorators: NodeList,
    },
    DeclareMethodDefinition = 92 {
        key: Node,
        value: Node,
        static: Boolean,
        optional: Boolean,
        computed: Boolean,
        kind: String,
        override: Boolean,
        tsAccessibility: MaybeString,
    },
    AbstractMethodDefinition = 93 {
        key: Node,
        value: Node,
        computed: Boolean,
        override: TrueBoolean,
        tsAccessibility: MaybeString,
    },
    AbstractPropertyDefinition = 94 {
        key: Node,
        value: Node,
        typeAnnotation: Node,
        computed: Boolean,
        variance: Node,
        override: TrueBoolean,
        tsAccessibility: MaybeString,
    },

    // ---------------------------------------------------------------
    // Switch / Catch / Function params
    // ---------------------------------------------------------------
    SwitchCase = 95 {
        test: Node,
        consequent: NodeList,
    },
    CatchClause = 96 {
        param: Node,
        body: Node,
    },
    ComponentParameter = 97 {
        name: Node,
        local: Node,
        shorthand: Boolean,
    },

    // ---------------------------------------------------------------
    // Flow Declarations
    // ---------------------------------------------------------------
    DeclareVariable = 98 {
        declarations: NodeList,
        kind: String,
        implicitDeclare: TrueBoolean,
    } from Statement::DeclareVariable { loc, inner }
        {=> self.serialize_declare_variable(loc, inner)},
    DeclareFunction = 99 {
        id: Node,
        typeAnnotation: OptionalNode,
        implicitDeclare: TrueBoolean,
        predicate: Node,
    } from Statement::DeclareFunction { loc, inner }
        {=> self.serialize_declare_function(loc, inner)},
    DeclareClass = 100 {
        id: Node,
        typeParameters: Node,
        extends: NodeList,
        implements: NodeList,
        mixins: NodeList,
        body: Node,
        implicitDeclare: TrueBoolean,
        abstract: TrueBoolean,
    } from Statement::DeclareClass { loc, inner }
        {=> self.serialize_declare_class(loc, inner)},
    DeclareComponent = 101 {
        id: Node,
        params: NodeList,
        rest: Node,
        rendersType: Node,
        typeParameters: Node,
        implicitDeclare: TrueBoolean,
    } from Statement::DeclareComponent { loc, inner }
        {=> self.serialize_declare_component(loc, inner)},
    DeclareHook = 102 {
        id: Node,
        implicitDeclare: TrueBoolean,
    },
    DeclareModule = 103 {
        id: Node,
        body: Node,
    } from Statement::DeclareModule { loc, inner }
        {=> self.serialize_declare_module(loc, inner)},
    DeclareModuleExports = 104 {
        typeAnnotation: Node,
    } from Statement::DeclareModuleExports { loc, inner } {
            self.serialize_type_annotation(&inner.annot),
        },
    DeclareExportDeclaration = 105 {
        default: Boolean,
        declaration: Node,
        specifiers: NodeList,
        source: Node,
        implicitDeclare: TrueBoolean,
    } from Statement::DeclareExportDeclaration { loc, inner }
        {=> self.serialize_declare_export_declaration(loc, inner)},
    DeclareExportAllDeclaration = 106 {
        source: Node,
        implicitDeclare: TrueBoolean,
    },
    DeclareNamespace = 107 {
        global: Boolean,
        id: Node,
        body: Node,
        implicitDeclare: TrueBoolean,
        keyword: String,
    }
        from Statement::DeclareNamespace { loc, inner }
        {=> self.serialize_declare_namespace(loc, inner)},
    DeclareInterface = 108 {
        id: Node,
        typeParameters: Node,
        body: Node,
        extends: NodeList,
        implicitDeclare: TrueBoolean,
    } from Statement::DeclareInterface { loc, inner }
        {=> self.serialize_declare_interface(loc, inner)},
    DeclareTypeAlias = 109 {
        id: Node,
        typeParameters: Node,
        right: Node,
        implicitDeclare: TrueBoolean,
    } from Statement::DeclareTypeAlias { loc, inner } {
            self.serialize_identifier_node(&inner.id),
            self.serialize_type_params_opt(&inner.tparams),
            self.serialize_type(&inner.right),
            self.write_bool(self.implicit_declare_from_loc(loc)),
        },
    DeclareEnum = 110 {
        id: Node,
        body: Node,
        implicitDeclare: TrueBoolean,
    } post { const: TrueBoolean }
        from Statement::DeclareEnum { loc, inner } {
            self.serialize_identifier_node(&inner.id),
            self.serialize_enum_body(&inner.body),
            self.write_bool(self.implicit_declare_from_loc(loc)),
            self.write_bool(inner.const_),
        },

    // ---------------------------------------------------------------
    // Type Annotations
    // ---------------------------------------------------------------
    TypeAnnotation = 111 {
        typeAnnotation: Node,
    },
    Variance = 112 {
        kind: String,
    },
    AnyTypeAnnotation = 113 {}
        from Type::Any { loc, .. } {},
    MixedTypeAnnotation = 114 {}
        from Type::Mixed { loc, .. } {},
    EmptyTypeAnnotation = 115 {}
        from Type::Empty { loc, .. } {},
    VoidTypeAnnotation = 116 {}
        from Type::Void { loc, .. } {},
    NullLiteralTypeAnnotation = 117 {}
        from Type::Null { loc, .. } {},
    SymbolTypeAnnotation = 118 {}
        from Type::Symbol { loc, .. } {},
    NumberTypeAnnotation = 119 {}
        from Type::Number { loc, .. } {},
    BigIntTypeAnnotation = 120 {}
        from Type::BigInt { loc, .. } {},
    StringTypeAnnotation = 121 {}
        from Type::String { loc, .. } {},
    BooleanTypeAnnotation = 122 {}
        from Type::Boolean { loc, .. } {},
    NullableTypeAnnotation = 123 {
        typeAnnotation: Node,
    } from Type::Nullable { loc, inner } {
            self.serialize_type(&inner.argument),
        },
    ArrayTypeAnnotation = 124 {
        elementType: Node,
    } from Type::Array { loc, inner } {
            self.serialize_type(&inner.argument),
        },
    IndexedAccessType = 125 {
        objectType: Node,
        indexType: Node,
    } from Type::IndexedAccess { loc, inner } {
            self.serialize_type(&inner.object),
            self.serialize_type(&inner.index),
        },
    OptionalIndexedAccessType = 126 {
        objectType: Node,
        indexType: Node,
        optional: Boolean,
    } from Type::OptionalIndexedAccess { loc, inner } {
            self.serialize_type(&inner.indexed_access.object),
            self.serialize_type(&inner.indexed_access.index),
            self.write_bool(inner.optional),
        },
    UnionTypeAnnotation = 127 {
        types: NodeList,
    } from Type::Union { loc, inner }
        {=> self.serialize_union_type(loc, inner)},
    IntersectionTypeAnnotation = 128 {
        types: NodeList,
    } from Type::Intersection { loc, inner }
        {=> self.serialize_intersection_type(loc, inner)},
    KeyofTypeAnnotation = 129 {
        argument: Node,
    } from Type::Keyof { loc, inner } {
            self.serialize_type(&inner.argument),
        },
    TypeOperator = 130 {
        operator: String,
        typeAnnotation: Node,
    } from Type::ReadOnly { loc, inner } {
            self.write_str("readonly"),
            self.serialize_type(&inner.argument),
        }
      from Type::UniqueSymbol { loc, symbol_loc, .. } {
            self.write_str("unique"),
            self.write_node_header(NodeKind::SymbolTypeAnnotation, symbol_loc),
        }
      from Type::Renders { loc, inner }
        {=> self.serialize_renders_type_dispatch(loc, inner)},
    StringLiteralTypeAnnotation = 131 {
        value: String,
        raw: String,
    } from Type::StringLiteral { loc, literal } {
            self.write_string_literal_value(&literal.value, &literal.raw),
            self.write_str(&literal.raw),
        },
    NumberLiteralTypeAnnotation = 132 {
        value: Number,
        raw: String,
    } from Type::NumberLiteral { loc, literal } {
            self.write_number(literal.value),
            self.write_str(&literal.raw),
        },
    BigIntLiteralTypeAnnotation = 133 {
        value: Node,
        raw: String,
        bigint: String,
    } custom { } from Type::BigIntLiteral { loc, literal } {
            self.write_null_node(),
            self.write_str(&literal.raw),
            // Mirror upstream Hermes' BigIntLiteralTypeAnnotation: emit
            // `bigint` as the cleaned numeric string (strip trailing `n`
            // and `_` separators from `raw`). The JS deserializer coerces
            // `value` to BigInt(bigint) inline (matching upstream's
            // HermesToESTreeAdapter.mapBigIntLiteralTypeAnnotation) —
            // `value: BigInt(...)` cannot be JSON-serialized so it can't
            // live on the wire.
            self.write_str(&clean_bigint_raw(&literal.raw)),
        },
    BooleanLiteralTypeAnnotation = 134 {
        value: Boolean,
        raw: String,
    } from Type::BooleanLiteral { loc, literal } {
            self.write_bool(literal.value),
            self.write_str(if literal.value { "true" } else { "false" }),
        },
    ExistsTypeAnnotation = 135 {}
        from Type::Exists { loc, .. } {},
    UnknownTypeAnnotation = 136 {}
        from Type::Unknown { loc, .. } {},
    NeverTypeAnnotation = 137 {}
        from Type::Never { loc, .. } {},
    UndefinedTypeAnnotation = 138 {}
        from Type::Undefined { loc, .. } {},
    GenericTypeAnnotation = 139 {
        id: Node,
        typeArguments: Node,
    } from Type::Generic { loc, inner }
        {=> self.serialize_generic_type(loc, inner)},
    QualifiedTypeIdentifier = 140 {
        qualification: Node,
        id: Node,
    },
    QualifiedTypeofIdentifier = 141 {
        qualification: Node,
        id: Node,
    },
    TypeofTypeAnnotation = 142 {
        argument: Node,
        typeArguments: Node,
    } from Type::Typeof { loc, inner } {
            self.serialize_typeof_target(&inner.argument),
            self.serialize_type_args_opt(&inner.targs),
        },
    ImportType = 143 {
        source: Node,
    },
    TupleTypeAnnotation = 144 {
        elementTypes: NodeList,
        inexact: Boolean,
    } from Type::Tuple { loc, inner }
        {=> self.serialize_tuple_type(loc, inner)},
    TupleTypeLabeledElement = 145 {
        label: Node,
        elementType: Node,
        variance: Node,
        optional: Boolean,
    },
    TupleTypeSpreadElement = 146 {
        label: Node,
        typeAnnotation: Node,
    },
    ObjectTypeAnnotation = 147 {
        properties: NodeList,
        indexers: NodeList,
        callProperties: NodeList,
        internalSlots: NodeList,
        inexact: Boolean,
        exact: Boolean,
    } from Type::Object { loc, inner }
        {=> self.serialize_type_object(loc, inner)},
    ObjectTypeProperty = 148 {
        key: Node,
        value: Node,
        method: Boolean,
        optional: Boolean,
        static: Boolean,
        proto: Boolean,
        variance: Node,
        kind: String,
        computed: TrueBoolean,
        abstract: TrueBoolean,
        override: TrueBoolean,
        tsAccessibility: MaybeString,
        init: OptionalNode,
    },
    ObjectTypeSpreadProperty = 149 {
        argument: Node,
    },
    ObjectTypeIndexer = 150 {
        id: Node,
        key: Node,
        value: Node,
        static: Boolean,
        variance: Node,
        optional: Boolean,
    },
    ObjectTypeCallProperty = 151 {
        value: Node,
        static: Boolean,
    },
    ObjectTypeMappedTypeProperty = 152 {
        keyTparam: Node,
        propType: Node,
        sourceType: Node,
        nameType: Node,
        variance: Node,
        varianceOp: String,
        optional: String,
    },
    ObjectTypeInternalSlot = 153 {
        id: Node,
        optional: Boolean,
        static: Boolean,
        method: Boolean,
        value: Node,
    },
    FunctionTypeParam = 154 {
        name: Node,
        typeAnnotation: Node,
        optional: Boolean,
    },
    TypePredicate = 155 {
        parameterName: Node,
        typeAnnotation: Node,
        kind: String,
    },
    ConditionalTypeAnnotation = 156 {
        checkType: Node,
        extendsType: Node,
        trueType: Node,
        falseType: Node,
    } from Type::Conditional { loc, inner } {
            self.serialize_type(&inner.check_type),
            self.serialize_type(&inner.extends_type),
            self.serialize_type(&inner.true_type),
            self.serialize_type(&inner.false_type),
        },
    InferTypeAnnotation = 157 {
        typeParameter: Node,
    } from Type::Infer { loc, inner } {
            self.serialize_type_parameter(&inner.tparam),
        },
    InterfaceExtends = 158 {
        id: Node,
        typeArguments: Node,
    },
    InterfaceTypeAnnotation = 159 {
        extends: NodeList,
        body: Node,
    } from Type::Interface { loc, inner }
        {=> self.serialize_interface_type(loc, inner)},
    TypeParameterDeclaration = 160 {
        params: NodeList,
    },
    TypeParameter = 161 {
        name: String,
        const: Boolean,
        bound: Node,
        variance: Node,
        default: Node,
        usesExtendsBound: Boolean,
    },
    TypeParameterInstantiation = 162 {
        params: NodeList,
    },
    ComponentTypeAnnotation = 163 {
        params: NodeList,
        rest: Node,
        rendersType: Node,
        typeParameters: Node,
    } from Type::Component { loc, inner }
        {=> self.serialize_component_type(loc, inner)},
    ComponentTypeParameter = 164 {
        name: Node,
        typeAnnotation: Node,
        optional: Boolean,
    },

    // ---------------------------------------------------------------
    // Predicates
    // ---------------------------------------------------------------
    DeclaredPredicate = 165 {
        value: Node,
    },
    InferredPredicate = 166 {},

    // ---------------------------------------------------------------
    // JSX
    // ---------------------------------------------------------------
    JSXElement = 167 {
        openingElement: Node,
        children: NodeList,
        closingElement: Node,
    } from Expression::JSXElement { loc, inner }
        {=> self.serialize_jsx_element(loc, inner)},
    JSXFragment = 168 {
        openingFragment: Node,
        children: NodeList,
        closingFragment: Node,
    } from Expression::JSXFragment { loc, inner }
        {=> self.serialize_jsx_fragment(loc, inner)},
    JSXOpeningElement = 169 {
        name: Node,
        attributes: NodeList,
        selfClosing: Boolean,
        typeArguments: Node,
    },
    JSXOpeningFragment = 170 {},
    JSXClosingElement = 171 {
        name: Node,
    },
    JSXClosingFragment = 172 {},
    JSXAttribute = 173 {
        name: Node,
        value: Node,
    },
    JSXSpreadAttribute = 174 {
        argument: Node,
    },
    JSXEmptyExpression = 175 {},
    JSXExpressionContainer = 176 {
        expression: Node,
    },
    JSXSpreadChild = 177 {
        expression: Node,
    },
    JSXText = 178 {
        value: String,
        raw: String,
    },
    JSXMemberExpression = 179 {
        object: Node,
        property: Node,
    },
    JSXNamespacedName = 180 {
        namespace: Node,
        name: Node,
    },
    JSXIdentifier = 181 {
        name: String,
    },

    // ---------------------------------------------------------------
    // Enums
    // ---------------------------------------------------------------
    BabelEnumBooleanBody = 182 as "EnumBooleanBody" {
        members: NodeList,
        explicitType: Boolean,
        hasUnknownMembers: Boolean,
    },
    EnumBooleanMember = 183 {
        id: Node,
        init: Node,
    },
    BabelEnumNumberBody = 184 as "EnumNumberBody" {
        members: NodeList,
        explicitType: Boolean,
        hasUnknownMembers: Boolean,
    },
    EnumNumberMember = 185 {
        id: Node,
        init: Node,
    },
    BabelEnumStringBody = 186 as "EnumStringBody" {
        members: NodeList,
        explicitType: Boolean,
        hasUnknownMembers: Boolean,
    },
    EnumStringMember = 187 {
        id: Node,
        init: Node,
    },
    BabelEnumSymbolBody = 188 as "EnumSymbolBody" {
        members: NodeList,
        hasUnknownMembers: Boolean,
    },
    EnumDefaultedMember = 189 {
        id: Node,
    },
    BabelEnumBigIntBody = 190 as "EnumBigIntBody" {
        members: NodeList,
        explicitType: Boolean,
        hasUnknownMembers: Boolean,
    },
    EnumBigIntMember = 191 {
        id: Node,
        init: Node,
    },

    // ---------------------------------------------------------------
    // Record types (Flow)
    // ---------------------------------------------------------------
    RecordDeclarationBody = 192 {
        elements: NodeList,
    },
    RecordDeclarationProperty = 193 {
        key: Node,
        typeAnnotation: Node,
        defaultValue: Node,
    },
    RecordDeclarationStaticProperty = 194 {
        key: Node,
        typeAnnotation: Node,
        value: Node,
    },
    RecordDeclarationImplements = 195 {
        id: Node,
        typeArguments: Node,
    },

    // ---------------------------------------------------------------
    // Match patterns (Flow)
    // ---------------------------------------------------------------
    MatchExpressionCase = 196 {
        pattern: Node,
        body: Node,
        guard: Node,
    },
    MatchStatementCase = 197 {
        pattern: Node,
        body: Node,
        guard: Node,
    },
    MatchWildcardPattern = 198 {},
    MatchLiteralPattern = 199 {
        literal: Node,
    },
    MatchUnaryPattern = 200 {
        operator: String,
        argument: Node,
    },
    MatchObjectPattern = 201 {
        properties: NodeList,
        rest: Node,
    },
    MatchInstancePattern = 202 {
        targetConstructor: Node,
        properties: Node,
    },
    MatchInstanceObjectPattern = 203 {
        properties: NodeList,
        rest: Node,
    },
    MatchOrPattern = 204 {
        patterns: NodeList,
    },
    MatchAsPattern = 205 {
        pattern: Node,
        target: Node,
    },
    MatchIdentifierPattern = 206 {
        id: Node,
    },
    MatchMemberPattern = 207 {
        base: Node,
        property: Node,
    },
    MatchBindingPattern = 208 {
        id: Node,
        kind: String,
    },
    MatchArrayPattern = 209 {
        elements: NodeList,
        rest: Node,
    },
    MatchObjectPatternProperty = 210 {
        key: Node,
        pattern: Node,
        shorthand: Boolean,
    },
    MatchRestPattern = 211 {
        argument: Node,
    },

    // ---------------------------------------------------------------
    // Program
    // ---------------------------------------------------------------
    Program = 212 {
        body: NodeList,
    },

    // ---------------------------------------------------------------
    // InterpreterDirective
    // ---------------------------------------------------------------
    InterpreterDirective = 213 {
        value: String,
    },

    // ---------------------------------------------------------------
    // Function type annotation
    // ---------------------------------------------------------------
    FunctionTypeAnnotation = 214 {
        params: NodeList,
        this: Node,
        returnType: Node,
        rest: Node,
        typeParameters: Node,
    } from Type::Function { loc, inner }
        {=> self.serialize_function_type(loc, inner)},

    // ---------------------------------------------------------------
    // Reserved renders-type wire slots
    // ---------------------------------------------------------------
    // These node kinds are no longer emitted: renders annotations serialize
    // as TypeOperator. Keep their slots so later wire IDs remain stable.
    RendersType = 215 {
        argument: Node,
    },
    RendersMaybeType = 216 {
        argument: Node,
    },
    RendersStarType = 217 {
        argument: Node,
    },

    // ---------------------------------------------------------------
    // Template literal type annotation
    // ---------------------------------------------------------------
    TemplateLiteralTypeAnnotation = 218 {
        quasis: NodeList,
        types: NodeList,
    } from Type::TemplateLiteral { loc, inner }
        {=> self.serialize_template_literal_type(loc, inner)},

    // ---------------------------------------------------------------
    // Append-only — do not reorder; IDs are wire-format positions.
    // ---------------------------------------------------------------
    NamespaceExportDeclaration = 219 {
        id: Node,
    } from Statement::NamespaceExportDeclaration { loc, inner } {
            self.serialize_identifier_node(&inner.id),
        },
    DeclareClassExtendsCall = 220 {
        callee: Node,
        argument: Node,
    },
    OpaqueType = 221 {
        id: Node,
        typeParameters: Node,
        impltype: Node,
        lowerBound: Node,
        upperBound: Node,
        supertype: Node,
    } from Statement::OpaqueType { loc, inner }
        {=> self.serialize_opaque_type(loc, inner)},
    DeclareOpaqueType = 222 {
        id: Node,
        typeParameters: Node,
        impltype: Node,
        lowerBound: Node,
        upperBound: Node,
        supertype: Node,
        implicitDeclare: TrueBoolean,
    } from Statement::DeclareOpaqueType { loc, inner }
        {=> self.serialize_declare_opaque_type(loc, inner)},
    ESTreeEnumBody = 223 as "EnumBody" {
        members: NodeList,
        explicitType: String,
        hasUnknownMembers: Boolean,
    },
    HookTypeAnnotation = 224 {
        params: NodeList,
        returnType: Node,
        rest: Node,
        typeParameters: Node,
    },
    // ConstructorTypeAnnotation has a leading `abstract` Boolean that's
    // a Rust reserved keyword; we declare it via `pre { abstract: Boolean }`
    // so the JS deserializer reads it before the schema fields. The
    // serializer writes `abstract` first (in `serialize_constructor_type`),
    // then the schema-listed fields.
    ConstructorTypeAnnotation = 225 {
        params: NodeList,
        returnType: Node,
        rest: Node,
        typeParameters: Node,
    } pre { abstract: Boolean }
        from Type::ConstructorType { loc, abstract_, inner }
        {=> self.serialize_constructor_type(loc, *abstract_, inner)},
    ObjectTypePrivateField = 226 {
        key: Node,
    },
    TupleTypeElement = 227 {
        elementType: Node,
        optional: Boolean,
    },
    // Ambient component declaration. Emitted by `serialize_component_declaration`
    // when `Statement::ComponentDeclaration` has body=None (the ambient
    // `component X(...) renders Y;` form without a body block).
    //
    // Two binary shapes both deserialize to ESTree `"DeclareComponent"` but
    // have different property layouts:
    // - DeclareComponent (NodeKind 101): SHORT shape (id, params, rendersType, typeParameters).
    // - DeclareComponentAmbient (this kind, 228): FULL shape (body, id, implicitDeclare,
    //   params, rendersType, typeParameters), same as ComponentDeclaration (NodeKind 23)
    //   but with body=null and implicitDeclare=true.
    //
    // Two binary layouts need two kind slots, but the JS `type` string must
    // match, hence the `as "DeclareComponent"` override.
    DeclareComponentAmbient = 228 as "DeclareComponent" {
        body: Node,
        id: Node,
        implicitDeclare: TrueBoolean,
        params: NodeList,
        rest: Node,
        rendersType: Node,
        typeParameters: Node,
        async: Boolean,
    },
    // ThisTypeAnnotation is the ESTree node upstream uses for `type T = this`.
    // Defined here so the schema-driven codegen artifacts (predicates,
    // selectors, visitor keys, types) include it natively and stay byte-for-
    // byte aligned with upstream — without hand-coded workarounds.
    //
    // No `from` clause: there is no AST variant for ThisTypeAnnotation. The
    // parser produces a `Type::Generic` with an `Unqualified` `this` id and
    // `targs: None` for both `type T = this` AND `(this) => void` / `m(): this`.
    // The Rust `serialize_type` collapses that case to ThisTypeAnnotation
    // before falling through to the auto-dispatch (mirroring upstream
    // `HermesToESTreeAdapter.mapGenericTypeAnnotation` and the
    // `estree_translator::generic_type` collapse).
    ThisTypeAnnotation = 229 {},
    // ChainExpression is the ESTree wrapper around an optional chain root.
    // No `from` clause: there is no AST variant for ChainExpression. The
    // Rust `serialize_expression` emits a ChainExpression header before the
    // root MemberExpression/CallExpression of an optional chain (mirroring
    // upstream Hermes' `mapChainExpression`). Defined here so the
    // schema-driven codegen artifacts include it natively.
    ChainExpression = 230 {
        expression: Node,
    },

    // Babel wire nodes. These are emitted only by the Rust serializer in
    // Babel mode; the JS side performs mechanical field decoding only.
    BabelExpressionStatement = 231 as "ExpressionStatement" {
        expression: Node,
    },
    BabelBlockStatement = 232 as "BlockStatement" {
        body: NodeList,
        directives: NodeList,
    },
    BabelDirective = 233 as "Directive" {
        value: Node,
    },
    BabelDirectiveLiteral = 234 as "DirectiveLiteral" {
        value: String,
        rawValue: String,
        raw: String,
    } custom { },
    BabelObjectProperty = 235 as "ObjectProperty" {
        computed: Boolean,
        key: Node,
        value: Node,
        method: Boolean,
        shorthand: Boolean,
    },
    BabelObjectMethod = 236 as "ObjectMethod" {
        kind: String,
        method: Boolean,
        computed: Boolean,
        key: Node,
        id: Node,
        params: NodeList,
        body: Node,
        async: Boolean,
        generator: Boolean,
        returnType: OptionalNode,
        typeParameters: OptionalNode,
        variance: MaybeNode,
    },
    BabelClassMethod = 237 as "ClassMethod" {
        kind: String,
        computed: Boolean,
        static: Boolean,
        key: Node,
        id: Node,
        params: NodeList,
        body: Node,
        async: Boolean,
        generator: Boolean,
        returnType: OptionalNode,
        typeParameters: OptionalNode,
        predicate: OptionalNode,
    },
    BabelClassPrivateMethod = 238 as "ClassPrivateMethod" {
        kind: String,
        static: Boolean,
        key: Node,
        id: Node,
        params: NodeList,
        body: Node,
        async: Boolean,
        generator: Boolean,
        returnType: OptionalNode,
        typeParameters: OptionalNode,
        predicate: OptionalNode,
    },
    BabelClassProperty = 239 as "ClassProperty" {
        key: Node,
        value: Node,
        typeAnnotation: OptionalNode,
        static: Boolean,
        variance: Node,
        declare: TrueBoolean,
        optional: TrueBoolean,
        computed: Boolean,
    },
    BabelClassPrivateProperty = 240 as "ClassPrivateProperty" {
        key: Node,
        value: Node,
        typeAnnotation: OptionalNode,
        static: Boolean,
        variance: Node,
    },
    BabelPrivateName = 241 as "PrivateName" {
        id: Node,
    },
    BabelStringLiteral = 242 as "StringLiteral" {} custom { },
    BabelNumericLiteral = 243 as "NumericLiteral" {} custom { },
    BabelBigIntLiteral = 244 as "BigIntLiteral" {} custom { },
    BabelBooleanLiteral = 245 as "BooleanLiteral" {
        value: Boolean,
    },
    BabelNullLiteral = 246 as "NullLiteral" {},
    BabelRegExpLiteral = 247 as "RegExpLiteral" {} custom { },
    BabelImport = 248 as "Import" {},
    BabelTupleTypeAnnotation = 249 as "TupleTypeAnnotation" {
        types: NodeList,
    },
    BabelStringLiteralTypeAnnotation = 250 as "StringLiteralTypeAnnotation" {} custom { },
    BabelNumberLiteralTypeAnnotation = 251 as "NumberLiteralTypeAnnotation" {} custom { },
    BabelBigIntLiteralTypeAnnotation = 252 as "BigIntLiteralTypeAnnotation" {} custom { },
    BabelBooleanLiteralTypeAnnotation = 253 as "BooleanLiteralTypeAnnotation" {
        value: Boolean,
    },
    BabelJSXText = 254 as "JSXText" {} custom { },
    BabelClassDeclaration = 255 as "ClassDeclaration" {
        id: Node,
        typeParameters: OptionalNode,
        superClass: Node,
        implements: NonEmptyNodeList,
        body: Node,
        superTypeParameters: OptionalNode,
        decorators: NonEmptyNodeList,
    },
    BabelClassExpression = 256 as "ClassExpression" {
        id: Node,
        typeParameters: OptionalNode,
        superClass: Node,
        implements: NonEmptyNodeList,
        body: Node,
        superTypeParameters: OptionalNode,
        decorators: NonEmptyNodeList,
    },
    BabelFunctionDeclaration = 257 as "FunctionDeclaration" {
        id: Node,
        params: NodeList,
        body: Node,
        typeParameters: OptionalNode,
        returnType: OptionalNode,
        generator: Boolean,
        async: Boolean,
        predicate: OptionalNode,
        __componentDeclaration: TrueBoolean,
        __hookDeclaration: TrueBoolean,
    },
    BabelFunctionExpression = 258 as "FunctionExpression" {
        id: Node,
        params: NodeList,
        body: Node,
        typeParameters: OptionalNode,
        returnType: OptionalNode,
        generator: Boolean,
        async: Boolean,
        predicate: OptionalNode,
    },
    BabelArrowFunctionExpression = 259 as "ArrowFunctionExpression" {
        params: NodeList,
        body: Node,
        typeParameters: OptionalNode,
        returnType: OptionalNode,
        async: Boolean,
        id: Node,
        predicate: OptionalNode,
    },
    BabelMemberExpression = 260 as "MemberExpression" {
        object: Node,
        property: Node,
        computed: Boolean,
    },
    BabelArrayExpression = 261 as "ArrayExpression" {
        elements: NodeList,
    },
    BabelRestElement = 262 as "RestElement" {
        argument: Node,
        typeAnnotation: MaybeNode,
    },
    BabelOpaqueType = 263 as "OpaqueType" {
        id: Node,
        typeParameters: Node,
        impltype: Node,
        supertype: Node,
    },
    BabelDeclareOpaqueType = 264 as "DeclareOpaqueType" {
        id: Node,
        typeParameters: Node,
        impltype: Node,
        supertype: Node,
        implicitDeclare: TrueBoolean,
    },
    BabelExportAllDeclaration = 265 as "ExportAllDeclaration" {
        source: Node,
        exportKind: String,
        attributes: NonEmptyNodeList,
    },
    BabelDeclareVariable = 266 as "DeclareVariable" {
        id: Node,
    },
    BabelIdentifier = 267 as "Identifier" {
        name: String,
        typeAnnotation: OptionalNode,
        optional: TrueBoolean,
    },
    BabelObjectPattern = 268 as "ObjectPattern" {
        properties: NodeList,
        typeAnnotation: OptionalNode,
        optional: MaybeBoolean,
    },
    BabelArrayPattern = 269 as "ArrayPattern" {
        elements: NodeList,
        typeAnnotation: OptionalNode,
        optional: MaybeBoolean,
    },
    BabelCallExpression = 270 as "CallExpression" {
        callee: Node,
        typeArguments: OptionalNode,
        arguments: NodeList,
        optional: TrueBoolean,
    },
    BabelOptionalCallExpression = 271 as "OptionalCallExpression" {
        callee: Node,
        typeArguments: OptionalNode,
        arguments: NodeList,
        optional: Boolean,
    },
    BabelImportDeclaration = 272 as "ImportDeclaration" {
        specifiers: NodeList,
        source: Node,
        importKind: String,
        attributes: NonEmptyNodeList,
        phase: MaybeString,
    },
    BabelExportNamedDeclaration = 273 as "ExportNamedDeclaration" {
        declaration: OptionalNode,
        specifiers: NodeList,
        source: Node,
        exportKind: String,
        attributes: NonEmptyNodeList,
    },
    BabelJSXOpeningElement = 274 as "JSXOpeningElement" {
        name: Node,
        attributes: NodeList,
        selfClosing: Boolean,
    },
    BabelTypeofTypeAnnotation = 275 as "TypeofTypeAnnotation" {
        argument: Node,
    },
    BabelEnumRuntime = 276 {} custom { },
    BabelProgram = 277 as "Program" {
        body: NodeList,
        directives: NodeList,
        sourceType: String,
        interpreter: Node,
    },
    BabelFile = 278 as "File" {
        program: Node,
        comments: CommentList,
    },
    BabelGenericTypeAnnotation = 279 as "GenericTypeAnnotation" {
        id: Node,
        typeParameters: Node,
    },
    BabelInterfaceExtends = 280 as "InterfaceExtends" {
        id: Node,
        typeParameters: Node,
    },
    BabelClassImplements = 281 as "ClassImplements" {
        id: Node,
        typeParameters: Node,
    },
}

// Compile-time check: SCHEMA entries must be in `kind_id` order starting
// from 0 with no gaps. The wire format encodes nodes as (kind_id + 1) and
// the JS deserializer indexes `NODE_DESERIALIZERS` by `kind_id`, so any
// hole or reorder would silently shift every downstream node ID and break
// the protocol. The runtime test `schema_is_complete_and_contiguous` in
// serializer.rs duplicates this check; the const-eval version catches
// violations at compile time so they can never reach a test.
const _: () = {
    let mut i = 0;
    while i < SCHEMA.len() {
        assert!(
            SCHEMA[i].kind_id as usize == i,
            "SCHEMA entries must be ordered by kind_id starting from 0 with no gaps"
        );
        i += 1;
    }
};
