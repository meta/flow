/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use dupe::Dupe;
use flow_aloc::ALoc;
use flow_aloc::ALocMap;
use flow_analysis::infer_type_hoister::ImpliedBound;
use flow_common::enclosing_context::EnclosingContext;
use flow_common::hint::Hint;
use flow_common::reason::Reason;
use flow_common::reason::VirtualReason;
use flow_data_structure_wrapper::ord_map::FlowOrdMap;
use flow_data_structure_wrapper::ord_set::FlowOrdSet;
use flow_data_structure_wrapper::smol_str::FlowSmolStr;
use flow_data_structure_wrapper::vector::FlowVector;
use flow_parser::ast;
use flow_parser::ast_visitor::TypeParamsContext;
use vec1::Vec1;

use crate::env_api::DefLocType;
use crate::env_api::EnvKey;
use crate::env_api::EnvMap;
use crate::env_api::EnvSet;
use crate::selector::Selector;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScopeKind {
    Ordinary,                 // function or module
    Async,                    // async function
    Generator,                // generator function
    AsyncGenerator,           // async generator function
    Module,                   // module scope
    DeclareModule,            // module scope
    DeclareNamespace,         // namespace scope
    Global,                   // global scope
    Ctor,                     // constructor function
    ComponentOrHookBody,      // component or hook syntax
    AsyncComponentOrHookBody, // async component or hook syntax
}

pub type ClassStack = FlowVector<ALoc>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ForKind {
    In,
    Of { await_: bool },
}

/// A map from location of tparam to name.
pub type TparamsMap = FlowOrdMap<ALoc, FlowSmolStr>;

#[derive(Debug, Clone)]
pub enum HintNode {
    AnnotationHint(TparamsMap, ast::types::Annotation<ALoc, ALoc>),
    ValueHint(EnclosingContext, ast::expression::Expression<ALoc, ALoc>),
    ProvidersHint(Vec1<ALoc>),
    WriteLocHint(DefLocType, ALoc),
    StringLiteralType(FlowSmolStr),
    ReactFragmentType,
    ReactNodeType,
    AnyErrorHint(Reason),
    ComposedArrayPatternHint(ALoc, Vec<ArrayElementPatternHint>),
    ComposedObjectPatternHint(ALoc, Vec<ObjectPropPatternHint>),
}

#[derive(Debug, Clone)]
pub enum ArrayElementPatternHint {
    ArrayElementPatternHint(HintNode),
    ArrayRestElementPatternHint(HintNode),
}

#[derive(Debug, Clone)]
pub enum ObjectPropPatternHint {
    ObjectPropPatternHint(FlowSmolStr, ALoc, HintNode),
    ObjectSpreadPropPatternHint(HintNode),
}

impl HintNode {
    /// Whether this hint originates from a user-written type annotation,
    /// looking through pattern-hint decompositions.
    pub fn is_syntactic(&self) -> bool {
        match self {
            HintNode::AnnotationHint(_, _) => true,
            HintNode::ComposedArrayPatternHint(_, elements) => {
                elements.iter().any(|element| match element {
                    ArrayElementPatternHint::ArrayElementPatternHint(node)
                    | ArrayElementPatternHint::ArrayRestElementPatternHint(node) => {
                        node.is_syntactic()
                    }
                })
            }
            HintNode::ComposedObjectPatternHint(_, props) => props.iter().any(|prop| match prop {
                ObjectPropPatternHint::ObjectPropPatternHint(_, _, node)
                | ObjectPropPatternHint::ObjectSpreadPropPatternHint(node) => node.is_syntactic(),
            }),
            HintNode::ValueHint(_, _)
            | HintNode::ProvidersHint(_)
            | HintNode::WriteLocHint(_, _)
            | HintNode::StringLiteralType(_)
            | HintNode::ReactFragmentType
            | HintNode::ReactNodeType
            | HintNode::AnyErrorHint(_) => false,
        }
    }
}

pub type AstHint = Hint<
    (),
    HintNode,
    Option<ast::expression::CallTypeArgs<ALoc, ALoc>>,
    ast::expression::ArgList<ALoc, ALoc>,
    (
        std::sync::Arc<[ast::jsx::OpeningAttribute<ALoc, ALoc>]>,
        (ALoc, Vec<ast::jsx::Child<ALoc, ALoc>>),
    ),
>;

pub type AstHints = Vec<AstHint>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FunctionSynthKind {
    FunctionSynthesizable,
    MissingReturn(ALoc),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ObjectMissingAnnot {
    FuncMissingAnnot(ALoc),
    OtherMissingAnnot(ALoc),
}

#[derive(Debug, Clone)]
pub enum ObjectSynthKind {
    ObjectSynthesizable {
        /// A set of this write locations that can be resolved by resolving the object.
        this_write_locs: EnvSet<ALoc>,
    },
    MissingMemberAnnots {
        locs: Vec1<ObjectMissingAnnot>,
    },
    Unsynthesizable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DroAnnot {
    Hook,
    Comp,
}

#[derive(Clone)]
pub struct Value {
    pub hints: AstHints,
    pub expr: ast::expression::Expression<ALoc, ALoc>,
    pub decl_kind: Option<ast::VariableKind>,
    pub as_const: bool,
}

#[derive(Clone)]
pub struct AnnotationData {
    pub tparams_map: TparamsMap,
    pub optional: bool,
    pub has_default_expression: bool,
    pub react_deep_read_only: Option<DroAnnot>,
    pub param_loc: Option<ALoc>,
    pub annot: (ALoc, ast::types::Type<ALoc, ALoc>),
    pub concrete: Option<Box<Root>>,
}

#[derive(Clone)]
pub struct MatchCaseRootData {
    pub case_match_root_loc: ALoc,
    pub root_pattern_loc: ALoc,
    pub prev_pattern_loc: Option<ALoc>,
}

#[derive(Clone)]
pub struct FunctionValueData {
    pub hints: AstHints,
    pub synthesizable_from_annotation: FunctionSynthKind,
    pub function_loc: ALoc,
    pub function_: ast::function::Function<ALoc, ALoc>,
    pub statics: BTreeMap<FlowSmolStr, EnvKey<ALoc>>,
    pub arrow: bool,
    pub tparams_map: TparamsMap,
}

#[derive(Clone)]
pub struct ObjectValueData {
    pub synthesizable: ObjectSynthKind,
    pub obj_loc: ALoc,
    pub obj: ast::expression::Object<ALoc, ALoc>,
}

#[derive(Clone)]
pub struct EmptyArrayData {
    pub array_providers: FlowOrdSet<ALoc>,
    pub arr_loc: ALoc,
}

#[derive(Clone)]
pub struct ContextualData {
    pub reason: Reason,
    pub hints: AstHints,
    pub optional: bool,
    pub default_expression: Option<(ALoc, ast::expression::Expression<ALoc, ALoc>)>,
}

#[derive(Clone)]
pub enum Root {
    Annotation(Box<AnnotationData>),
    Value(Box<Value>),
    MatchCaseRoot(Box<MatchCaseRootData>),
    FunctionValue(Box<FunctionValueData>),
    ObjectValue(Box<ObjectValueData>),
    EmptyArray(Box<EmptyArrayData>),
    Contextual(Box<ContextualData>),
    CatchUnannotated,
    DeclareVariableMissingAnnotationAndInit,
    UnannotatedParameter(Reason),
    For(Box<(ForKind, (ALoc, ast::expression::Expression<ALoc, ALoc>))>),
}

pub fn mk_value(
    hints: Option<AstHints>,
    decl_kind: Option<ast::VariableKind>,
    as_const: bool,
    expr: ast::expression::Expression<ALoc, ALoc>,
) -> Root {
    Root::Value(Box::new(Value {
        hints: hints.unwrap_or_default(),
        expr,
        decl_kind,
        as_const,
    }))
}

#[derive(Clone)]
pub enum Binding {
    Root(Root),
    Hooklike(Box<Binding>),
    Select {
        selector: Selector<ALoc, ALoc>,
        parent: (ALoc, Box<Binding>),
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Import {
    Named {
        kind: Option<ast::statement::ImportKind>,
        remote: FlowSmolStr,
        local: FlowSmolStr,
    },
    Namespace(FlowSmolStr),
    Default(FlowSmolStr),
}

#[derive(Debug, Clone)]
pub struct GeneratorAnnot {
    pub tparams_map: TparamsMap,
    pub return_annot: (ALoc, ast::types::Type<ALoc, ALoc>),
    pub async_: bool,
}

#[derive(Clone)]
pub struct ExpressionDef {
    pub cond_context: EnclosingContext,
    pub chain: bool,
    pub expr: ast::expression::Expression<ALoc, ALoc>,
    pub hints: AstHints,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClassKind {
    Class,
    Record {
        defaulted_props: BTreeSet<FlowSmolStr>,
    },
}

#[derive(Clone)]
pub struct MatchCasePatternData {
    pub case_match_root_loc: ALoc,
    pub has_guard: bool,
    pub pattern: (ALoc, ast::match_pattern::MatchPattern<ALoc, ALoc>),
    pub prev_pattern_loc: Option<ALoc>,
}

#[derive(Clone)]
pub struct MemberAssignData {
    pub member_loc: ALoc,
    pub member: ast::expression::Member<ALoc, ALoc>,
    pub rhs: (ALoc, ast::expression::Expression<ALoc, ALoc>),
}

#[derive(Clone)]
pub struct OpAssignData {
    pub exp_loc: ALoc,
    pub lhs: (ALoc, ast::pattern::Pattern<ALoc, ALoc>),
    pub op: ast::expression::AssignmentOperator,
    pub rhs: (ALoc, ast::expression::Expression<ALoc, ALoc>),
    pub assertion: bool,
}

#[derive(Clone)]
pub struct FunctionDefData {
    pub hints: AstHints,
    pub synthesizable_from_annotation: FunctionSynthKind,
    pub arrow: bool,
    pub has_this_def: bool,
    pub function_loc: ALoc,
    pub function_: ast::function::Function<ALoc, ALoc>,
    pub statics: BTreeMap<FlowSmolStr, EnvKey<ALoc>>,
    pub namespace_types: BTreeMap<FlowSmolStr, EnvKey<ALoc>>,
    pub tparams_map: TparamsMap,
}

#[derive(Clone)]
pub struct DeclaredFunctionDefData {
    pub declarations: Vec<(ALoc, ast::statement::DeclareFunction<ALoc, ALoc>)>,
    pub statics: BTreeMap<FlowSmolStr, EnvKey<ALoc>>,
    pub namespace_types: BTreeMap<FlowSmolStr, EnvKey<ALoc>>,
}

#[derive(Clone)]
pub struct ComponentDefData {
    pub tparams_map: TparamsMap,
    pub component_loc: ALoc,
    pub component: ast::statement::ComponentDeclaration<ALoc, ALoc>,
}

#[derive(Clone, Default)]
pub struct ClassNamespaceMembers {
    pub values: BTreeMap<FlowSmolStr, EnvKey<ALoc>>,
    pub types: BTreeMap<FlowSmolStr, EnvKey<ALoc>>,
}

#[derive(Clone)]
pub struct ClassDefData {
    pub class_: ast::class::Class<ALoc, ALoc>,
    pub class_loc: ALoc,
    /// A set of this and super write locations that can be resolved by resolving the class.
    pub this_super_write_locs: EnvSet<ALoc>,
    pub kind: ClassKind,
    pub namespace_members: ClassNamespaceMembers,
}

#[derive(Clone)]
pub struct DeclaredClassDefData {
    pub loc: ALoc,
    pub decl: ast::statement::DeclareClass<ALoc, ALoc>,
    pub namespace_members: ClassNamespaceMembers,
}

#[derive(Clone)]
pub struct RecordDefData {
    pub record: ast::statement::RecordDeclaration<ALoc, ALoc>,
    pub record_loc: ALoc,
    /// A set of this and super write locations that can be resolved by resolving the record.
    pub this_super_write_locs: EnvSet<ALoc>,
    pub defaulted_props: BTreeSet<FlowSmolStr>,
}

#[derive(Clone)]
pub struct TypeParamData {
    pub tparams_map: TparamsMap,
    pub kind: TypeParamsContext,
    pub implied_bound: ImpliedBound,
    pub tparam: (ALoc, ast::types::TypeParam<ALoc, ALoc>),
}

#[derive(Clone)]
pub struct ImportData {
    pub import_kind: ast::statement::ImportKind,
    pub import: Import,
    pub source: FlowSmolStr,
    pub source_loc: ALoc,
}

#[derive(Clone)]
pub enum Def {
    Binding(Box<Binding>),
    MatchCasePattern(Box<MatchCasePatternData>),
    ExpressionDef(Box<ExpressionDef>),
    MemberAssign(Box<MemberAssignData>),
    OpAssign(Box<OpAssignData>),
    Update {
        exp_loc: ALoc,
        op: ast::expression::UpdateOperator,
    },
    Function(Box<FunctionDefData>),
    DeclaredFunction(Box<DeclaredFunctionDefData>),
    Component(Box<ComponentDefData>),
    Class(Box<ClassDefData>),
    Record(Box<RecordDefData>),
    DeclaredClass(Box<DeclaredClassDefData>),
    DeclaredComponent(ALoc, ast::statement::DeclareComponent<ALoc, ALoc>),
    TypeAlias(ALoc, ast::statement::TypeAlias<ALoc, ALoc>),
    OpaqueType(ALoc, ast::statement::OpaqueType<ALoc, ALoc>),
    TypeParam(Box<TypeParamData>),
    Interface(ALoc, ast::statement::Interface<ALoc, ALoc>),
    Enum {
        loc: ALoc,
        name: FlowSmolStr,
        /// Whether this is a `declare enum` (ambient regardless of file
        /// extension), which affects TS enum member numbering.
        declared: bool,
        body: ast::statement::enum_declaration::Body<ALoc>,
    },
    Import(Box<ImportData>),
    GeneratorNext(Box<Option<GeneratorAnnot>>),
    DeclaredNamespace(ALoc, ast::statement::DeclareNamespace<ALoc, ALoc>),
    MissingThisAnnot,
}

pub mod print {
    use std::fmt::Write;

    use flow_aloc::ALoc;
    use flow_parser::loc_sig::LocSig;

    use super::*;

    pub fn string_of_root(root: &Root) -> String {
        match root {
            Root::Contextual(_) => "contextual".to_string(),
            Root::EmptyArray(_) => "[]".to_string(),
            Root::CatchUnannotated => "unannotated catch param".to_string(),
            Root::DeclareVariableMissingAnnotationAndInit => {
                "declare var missing annotation or init".to_string()
            }
            Root::UnannotatedParameter(r) => format!("{:?}", r),
            Root::Annotation(data) => {
                let (loc, _) = &data.annot;
                format!("annot {}", loc.debug_to_string(false))
            }
            Root::Value(val) => {
                format!("val {}", val.expr.loc().debug_to_string(false))
            }
            Root::MatchCaseRoot(data) => {
                format!(
                    "match root for case at {}",
                    data.case_match_root_loc.debug_to_string(false)
                )
            }
            Root::FunctionValue(data) => {
                format!("function val {}", data.function_loc.debug_to_string(false))
            }
            Root::ObjectValue(_) => "object".to_string(),
            Root::For(data) => match &data.0 {
                ForKind::In => format!("for in {}", data.1.0.debug_to_string(false)),
                ForKind::Of { .. } => format!("for of {}", data.1.0.debug_to_string(false)),
            },
        }
    }

    pub fn string_of_selector(sel: &Selector<ALoc, ALoc>) -> String {
        sel.to_string()
    }

    pub fn string_of_binding(binding: &Binding) -> String {
        match binding {
            Binding::Root(r) => string_of_root(r),
            Binding::Hooklike(b) => format!("({})<as hooklike>", string_of_binding(b)),
            Binding::Select {
                selector,
                parent: (_, binding),
                ..
            } => {
                format!(
                    "({}){}",
                    string_of_binding(binding),
                    string_of_selector(selector)
                )
            }
        }
    }

    pub fn string_of_import_kind(kind: &ast::statement::ImportKind) -> &'static str {
        use ast::statement::ImportKind;
        match kind {
            ImportKind::ImportTypeof => "typeof ",
            ImportKind::ImportType => "type ",
            ImportKind::ImportValue => "",
        }
    }

    pub fn string_of_import(import: &Import) -> String {
        match import {
            Import::Named { kind, remote, .. } => {
                let k = kind.as_ref().map(string_of_import_kind).unwrap_or("");
                format!("{}{}", k, remote)
            }
            Import::Namespace(_) => "namespace".to_string(),
            Import::Default(_) => "default".to_string(),
        }
    }

    pub fn on_hint(hint: &HintNode) -> String {
        match hint {
            HintNode::AnnotationHint(_, _) => "annot hint".to_string(),
            HintNode::ValueHint(_, _) => "value hint".to_string(),
            HintNode::ProvidersHint(_) => "providers hint".to_string(),
            HintNode::WriteLocHint(_, _) => "write loc hint".to_string(),
            HintNode::StringLiteralType(s) => format!("string literal hint: {}", s),
            HintNode::ReactFragmentType => "react fragment hint".to_string(),
            HintNode::ReactNodeType => "react node hint".to_string(),
            HintNode::AnyErrorHint(_) => "any type hint".to_string(),
            HintNode::ComposedArrayPatternHint(_, elements) => {
                let mut result = String::from("[");
                for (i, elem) in elements.iter().enumerate() {
                    if i > 0 {
                        result.push_str(", ");
                    }
                    match elem {
                        ArrayElementPatternHint::ArrayElementPatternHint(h) => {
                            write!(result, "({})", on_hint(h)).unwrap();
                        }
                        ArrayElementPatternHint::ArrayRestElementPatternHint(h) => {
                            write!(result, "...({})", on_hint(h)).unwrap();
                        }
                    }
                }
                result.push(']');
                result
            }
            HintNode::ComposedObjectPatternHint(_, props) => {
                let mut result = String::from("{");
                for (i, prop) in props.iter().enumerate() {
                    if i > 0 {
                        result.push_str(", ");
                    }
                    match prop {
                        ObjectPropPatternHint::ObjectPropPatternHint(n, _, h) => {
                            write!(result, "{}: {}", n, on_hint(h)).unwrap();
                        }
                        ObjectPropPatternHint::ObjectSpreadPropPatternHint(h) => {
                            write!(result, "...({})", on_hint(h)).unwrap();
                        }
                    }
                }
                result.push('}');
                result
            }
        }
    }

    pub fn string_of_source(def: &Def) -> String {
        match def {
            Def::Binding(b) => string_of_binding(b),
            Def::MatchCasePattern(data) => {
                let (loc, _) = &data.pattern;
                format!("match pattern {}", loc.debug_to_string(false))
            }
            Def::ExpressionDef(data) => {
                format!(
                    "exp {} (hint = {})",
                    data.expr.loc().debug_to_string(false),
                    string_of_hints(&data.hints)
                )
            }
            Def::Update { .. } => "[in/de]crement".to_string(),
            Def::MemberAssign(_) => "member_assign".to_string(),
            Def::OpAssign(_) => "opassign".to_string(),
            Def::Function(data) => {
                let name = data
                    .function_
                    .id
                    .as_ref()
                    .map(|id| id.name.as_str())
                    .unwrap_or("<anonymous>");
                format!("fun {}", name)
            }
            Def::Component(data) => {
                format!("component {}", data.component.id.name)
            }
            Def::DeclaredClass(data) => {
                format!("declared class {}", data.decl.id.name)
            }
            Def::DeclaredComponent(_, decl) => {
                format!("declared component {}", decl.id.name)
            }
            Def::Class(data) => {
                let kind_str = match &data.kind {
                    ClassKind::Class => "class",
                    ClassKind::Record { .. } => "record",
                };
                let name = data
                    .class_
                    .id
                    .as_ref()
                    .map(|id| id.name.as_str())
                    .unwrap_or("<anonymous>");
                format!("{} {}", kind_str, name)
            }
            Def::Record(data) => {
                format!("record {}", data.record.id.name)
            }
            Def::DeclaredFunction(data) => {
                let name = data
                    .declarations
                    .first()
                    .and_then(|(_, decl)| decl.id.as_ref())
                    .map(|id| id.name.as_str())
                    .unwrap_or("<anonymous>");
                format!("declare function {}", name)
            }
            Def::TypeAlias(_, alias) => {
                format!("alias {}", alias.right.loc().debug_to_string(false))
            }
            Def::OpaqueType(_, opaque) => {
                format!("opaque {}", opaque.id.loc.debug_to_string(false))
            }
            Def::TypeParam(data) => {
                let (loc, _) = &data.tparam;
                format!("tparam {}", loc.debug_to_string(false))
            }
            Def::Enum { loc, name, .. } => {
                format!("enum {} {}", name, loc.debug_to_string(false))
            }
            Def::Interface(_, _) => "interface".to_string(),
            Def::GeneratorNext(_) => "next".to_string(),
            Def::DeclaredNamespace(_, ns) => {
                use ast::statement::declare_namespace::Id;
                match &ns.id {
                    Id::Local(id) => {
                        format!(
                            "declare namespace {} {}",
                            id.name,
                            id.loc.debug_to_string(false)
                        )
                    }
                    Id::Global(_) => "declare global".to_string(),
                }
            }
            Def::Import(data) => {
                format!(
                    "import {}{} from {}",
                    string_of_import_kind(&data.import_kind),
                    string_of_import(&data.import),
                    data.source
                )
            }
            Def::MissingThisAnnot => "this (missing)".to_string(),
        }
    }

    fn string_of_hints(hints: &AstHints) -> String {
        use flow_common::hint::Hint;
        let strs: Vec<String> = hints
            .iter()
            .map(|hint| match hint {
                Hint::HintT(t, _) => format!("Hint_t ({})", on_hint(t)),
                Hint::HintDecomp(_, t, _) => format!("Hint_Decomp ({})", on_hint(t)),
                Hint::HintPlaceholder => "Hint_Placeholder".to_string(),
            })
            .collect();
        format!("[{}]", strs.join(", "))
    }
}

/// Semantic description of a definition used when reporting dependency cycles.
#[derive(
    Debug,
    Clone,
    Dupe,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    serde::Serialize,
    serde::Deserialize
)]
pub enum DefinitionReferenceKind {
    Name(FlowSmolStr),
    Function { async_: bool, generator: bool },
    Component(FlowSmolStr),
    Enum(FlowSmolStr),
    Interface,
    This,
    Destructuring,
    Match,
    Other,
}

pub type NameDefEntry = (
    Def,
    ScopeKind,
    ClassStack,
    VirtualReason<ALoc>,
    DefinitionReferenceKind,
);

pub type EnvEntriesMap = EnvMap<ALoc, NameDefEntry>;

pub type HintMap = ALocMap<AstHints>;
