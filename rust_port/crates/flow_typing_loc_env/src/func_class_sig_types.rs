/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

// =============================================================================
// Func_class_sig_types (ported from func_class_sig_types.rs)
// =============================================================================

use std::collections::BTreeMap;
use std::rc::Rc;

use flow_aloc::ALoc;
use flow_aloc::ALocId;
use flow_common::error_ref::FunctionReferenceKind;
use flow_common::polarity::Polarity;
use flow_common::reason::Name;
use flow_common::reason::Reason;
use flow_common::subst_name::SubstName;
use flow_data_structure_wrapper::smol_str::FlowSmolStr;
use flow_parser::ast::expression::Expression;
use flow_parser::ast::function::Body as FunctionBody;
use flow_parser::ast::function::Param as AstFunctionParam;
use flow_parser::ast::function::Params as AstFunctionParams;
use flow_parser::ast::function::RestParam as AstFunctionRestParam;
use flow_parser::ast::function::ThisParam as AstFunctionThisParam;
use flow_typing_type::type_::AnnotatedOrInferred;
use flow_typing_type::type_::ReactEffectType;
use flow_typing_type::type_::Type;
use flow_typing_type::type_::TypeGuard;
use flow_typing_type::type_::TypeParam;
use flow_typing_type::type_::TypeParams;
use flow_typing_type::type_::object::Dict as ObjectDict;
use vec1::Vec1;

use crate::func_stmt_config_types;

pub type Ast<T> = AstFunctionParams<ALoc, T>;
pub type ParamAst<T> = AstFunctionParam<ALoc, T>;
pub type RestAst<T> = AstFunctionRestParam<ALoc, T>;
pub type ThisAst<T> = AstFunctionThisParam<ALoc, T>;

pub trait ConfigTypes {
    type Param: Clone;
    type Rest: Clone;
    type ThisParam: Clone;
    type ParamAst;
    type RestAst;
    type ThisAst;
    type Ast;
}

pub struct StmtConfigTypes;

impl ConfigTypes for StmtConfigTypes {
    type Param = func_stmt_config_types::Param;
    type Rest = func_stmt_config_types::Rest;
    type ThisParam = func_stmt_config_types::ThisParam;
    type ParamAst = ParamAst<(ALoc, Type)>;
    type RestAst = RestAst<(ALoc, Type)>;
    type ThisAst = ThisAst<(ALoc, Type)>;
    type Ast = Ast<(ALoc, Type)>;
}

pub mod param {
    use super::*;

    #[allow(type_alias_bounds)]
    pub type Reconstruct<C: ConfigTypes> =
        Rc<dyn Fn(Vec<C::ParamAst>, Option<C::RestAst>, Option<C::ThisAst>) -> Option<C::Ast>>;

    pub struct Param<C: ConfigTypes> {
        pub params: Vec<C::Param>,
        pub rest: Option<C::Rest>,
        pub this_: Option<C::ThisParam>,
        pub reconstruct: Reconstruct<C>,
    }

    impl<C: ConfigTypes> Clone for Param<C> {
        fn clone(&self) -> Self {
            Param {
                params: self.params.clone(),
                rest: self.rest.clone(),
                this_: self.this_.clone(),
                reconstruct: self.reconstruct.clone(),
            }
        }
    }
}

// =============================================================================
// Func
// =============================================================================

pub mod func {
    use flow_parser::ast;

    use super::*;

    #[derive(Debug, Clone)]
    pub enum Kind {
        Ordinary,
        Async,
        Generator {
            return_loc: ALoc,
        },
        AsyncGenerator {
            return_loc: ALoc,
        },
        FieldInit {
            /// The field's name as written, e.g. `x`, `#x` or `[sym]`.
            name: FlowSmolStr,
            expr: Expression<ALoc, ALoc>,
            /// Whether the field is annotated with a bare `unique symbol`, in
            /// which case an initializer that calls the symbol constructor
            /// produces the annotated symbol rather than one of its own.
            fills_annotated_symbol: bool,
        },
        TypeGuard(TypeGuard),
        Ctor,
    }

    pub fn string_of_kind(kind: &Kind) -> &'static str {
        match kind {
            Kind::Ordinary => "ordinary",
            Kind::Async => "async",
            Kind::Generator { .. } => "generator",
            Kind::AsyncGenerator { .. } => "async generator",
            Kind::FieldInit { .. } => "field initializer",
            Kind::TypeGuard(_) => "type guard",
            Kind::Ctor => "constructor",
        }
    }

    // Concrete type aliases for the statement path
    pub type FuncParams = param::Param<StmtConfigTypes>;

    pub type FuncParamsTast = Ast<(ALoc, Type)>;

    pub type Reconstruct = Rc<
        dyn Fn(
            ast::function::Params<ALoc, (ALoc, Type)>,
            ast::function::Body<ALoc, (ALoc, Type)>,
            Type,
        ) -> ast::function::Function<ALoc, (ALoc, Type)>,
    >;

    // In OCaml, Func.Make(Config)(Param) is a functor producing a Func.t
    // parameterized by Config. In Rust, we make Func generic over C: ConfigTypes.
    pub struct Func<C: ConfigTypes> {
        pub reason: Reason,
        pub function_reference_kind: FunctionReferenceKind,
        pub kind: Kind,
        pub tparams: TypeParams,
        pub fparams: param::Param<C>,
        pub body: Option<FunctionBody<ALoc, ALoc>>,
        pub return_t: AnnotatedOrInferred,
        pub effect_: ReactEffectType,
        pub ret_annot_loc: ALoc,
        pub statics: Option<Type>,
        pub literal: bool,
    }

    impl<C: ConfigTypes> Clone for Func<C> {
        fn clone(&self) -> Self {
            Func {
                reason: self.reason.clone(),
                function_reference_kind: self.function_reference_kind,
                kind: self.kind.clone(),
                tparams: self.tparams.clone(),
                fparams: self.fparams.clone(),
                body: self.body.clone(),
                return_t: self.return_t.clone(),
                effect_: self.effect_.clone(),
                ret_annot_loc: self.ret_annot_loc.clone(),
                statics: self.statics.clone(),
                literal: self.literal,
            }
        }
    }
}

// =============================================================================
// Class
// =============================================================================

pub mod class {
    use super::*;

    /// Tag identifying whether a [typeapp] in an [extends]/[implements] clause
    /// resolves to a class-like binding ([class] / [declare class] / [interface]
    /// / [record]). [ClassLikeMono]/[ClassLikePoly] mean we can wrap the parent
    /// in [TypeUtil.this_typeapp] driven by binding-kind metadata alone (no
    /// forcing of the resolved type tvar). [BindingOther] means the parent is a
    /// type alias / variable / etc. — don't wrap. [BindingUnknown] is the
    /// fallback when binding kind isn't statically visible.
    ///
    /// [ClassLikeRawMono]/[ClassLikeRawPoly] carry an explicit *raw*
    /// ([ClassT(ThisInstanceT(..))]-shaped) override of the converted [c]. Used
    /// when [c] from [convert_qualification] is in canonicalized form (cross-
    /// module imports go through [import_named_specifier_type] which calls
    /// [canonicalize_imported_type]'s [fix_this_instance], destroying the
    /// polymorphic-[this] shape). The raw override is computed via the parallel
    /// [_for_extends] import path that bypasses canonicalization. The class-sig
    /// consumer ([class_sig.rs]) wraps this raw override in [TypeUtil.this_typeapp]
    /// so polymorphic [this] rebinds correctly across module boundaries. For
    /// namespace-member references like [NS.I] the converted [c] is already raw
    /// (the namespace's [types_tmap] stores raw exporter-side types), so the
    /// override carries the same [c] — the variant is the marker that triggers
    /// the [this_typeapp] wrap path.
    #[derive(Debug, Clone)]
    pub enum ClassLikeBindingKind {
        ClassLikeMono,
        ClassLikePoly,
        ClassLikeRawMono(Type),
        ClassLikeRawPoly(Type),
        BindingOther,
        BindingUnknown,
    }

    pub type FuncSig<C> = func::Func<C>;

    #[allow(type_alias_bounds)]
    pub type SetAsts<C: ConfigTypes> = Rc<
        dyn Fn(
            (
                Option<C::Ast>,
                Option<FunctionBody<ALoc, (ALoc, Type)>>,
                Option<Expression<ALoc, (ALoc, Type)>>,
            ),
        ),
    >;

    pub type SetType = Rc<dyn Fn(Type)>;

    pub enum Field<C: ConfigTypes> {
        Annot(Type),
        Infer(FuncSig<C>, SetAsts<C>),
    }

    impl<C: ConfigTypes> Clone for Field<C> {
        fn clone(&self) -> Self {
            match self {
                Field::Annot(t) => Field::Annot(t.clone()),
                Field::Infer(f, s) => Field::Infer(f.clone(), s.clone()),
            }
        }
    }

    pub type FieldPrime<C> = (Option<ALoc>, Polarity, Field<C>);

    pub type TypeApp = (ALoc, Type, Option<Vec<Type>>);

    #[derive(Debug, Clone)]
    pub enum Extends {
        Explicit(TypeApp),
        Implicit { null: bool },
    }

    #[derive(Debug, Clone)]
    pub struct ClassSuper {
        pub extends: Extends,
        // declare class only
        pub mixins: Vec<TypeApp>,
        pub implements: Vec<TypeApp>,
        /// Parallel to [implements]: see comment on [ClassLikeBindingKind].
        pub implements_binding_kinds: Vec<ClassLikeBindingKind>,
        pub this_tparam: TypeParam,
        pub this_t: Type,
    }

    #[derive(Debug, Clone)]
    pub struct InterfaceSuper {
        pub inline: bool,
        // Anonymous interface, can appear anywhere inside a type
        pub extends: Vec<TypeApp>,
        /// Parallel to [extends]: see comment on [ClassLikeBindingKind].
        pub extends_binding_kinds: Vec<ClassLikeBindingKind>,
        /// The interface has a call property or a construct signature, so its
        /// values are functions at runtime and `Function.prototype` belongs on
        /// the supertype list.
        pub function_like: bool,
        /// Polymorphic [this] for the interface. None for inline interfaces.
        pub this_tparam: Option<TypeParam>,
        pub this_t: Option<Type>,
    }

    #[derive(Debug, Clone)]
    pub enum Super {
        Interface(InterfaceSuper),
        Class(ClassSuper),
    }

    pub struct FuncInfo<C: ConfigTypes> {
        pub id_loc: Option<ALoc>,
        pub this_write_loc: Option<ALoc>,
        pub uses_this: bool,
        pub func_sig: FuncSig<C>,
        pub set_asts: SetAsts<C>,
        pub set_type: SetType,
    }

    impl<C: ConfigTypes> Clone for FuncInfo<C> {
        fn clone(&self) -> Self {
            FuncInfo {
                id_loc: self.id_loc.clone(),
                this_write_loc: self.this_write_loc.clone(),
                uses_this: self.uses_this,
                func_sig: self.func_sig.clone(),
                set_asts: self.set_asts.clone(),
                set_type: self.set_type.clone(),
            }
        }
    }

    pub struct Signature<C: ConfigTypes> {
        pub reason: Reason,
        // Public members are keyed by `Name` so a `unique symbol` computed key
        // keeps its nominal identity instead of collapsing onto a shared string
        // placeholder. Private members (`#name`) are always identifier-spelled,
        // so they stay `FlowSmolStr`-keyed.
        pub fields: BTreeMap<Name, FieldPrime<C>>,
        pub private_fields: BTreeMap<FlowSmolStr, FieldPrime<C>>,
        pub proto_fields: BTreeMap<Name, FieldPrime<C>>,
        // Multiple function signatures indicates an overloaded method.
        pub methods: BTreeMap<Name, Vec1<FuncInfo<C>>>,
        pub private_methods: BTreeMap<FlowSmolStr, FuncInfo<C>>,
        pub getters: BTreeMap<Name, FuncInfo<C>>,
        pub setters: BTreeMap<Name, FuncInfo<C>>,
        pub calls: Vec<Type>,
        pub constructs: Vec<Type>,
        pub dict: ObjectDict,
        /// Names of members in this signature (own/proto/getter/setter) that
        /// were declared with the `abstract` modifier. Source of truth for
        /// which names contribute to `insttype.inst_abstract_props`. Keyed by
        /// `Name` so a `unique symbol` member keeps its nominal identity.
        pub abstract_members: std::collections::BTreeSet<Name>,
        /// Names of members in this signature declared with the [override]
        /// modifier, mapped to the member's def loc. Tracked per-side
        /// (instance/static) — override checks fire independently per side.
        /// The loc is used as the primary loc for the
        /// [ImplicitOverrideMissingModifier] error (which fires at the
        /// member that should have had the modifier). Keyed by `Name`; private
        /// members are stored as `Name::Str("#name")`.
        pub override_members: BTreeMap<Name, ALoc>,
    }

    impl<C: ConfigTypes> Clone for Signature<C> {
        fn clone(&self) -> Self {
            Signature {
                reason: self.reason.clone(),
                fields: self.fields.clone(),
                private_fields: self.private_fields.clone(),
                proto_fields: self.proto_fields.clone(),
                methods: self.methods.clone(),
                private_methods: self.private_methods.clone(),
                getters: self.getters.clone(),
                setters: self.setters.clone(),
                calls: self.calls.clone(),
                constructs: self.constructs.clone(),
                dict: self.dict.clone(),
                abstract_members: self.abstract_members.clone(),
                override_members: self.override_members.clone(),
            }
        }
    }

    pub struct Class<C: ConfigTypes> {
        pub id: ALocId,
        pub class_name: Option<FlowSmolStr>,
        pub class_loc: ALoc,
        pub tparams: TypeParams,
        pub tparams_map: BTreeMap<SubstName, Type>,
        pub super_: Super,
        // Multiple function signatures indicates an overloaded constructor.
        pub constructor: Vec<FuncInfo<C>>,
        pub static_: Signature<C>,
        pub instance: Signature<C>,
        /// Whether the class declaration carried the `abstract` class modifier.
        /// Propagated to `insttype.inst_abstract`.
        pub abstract_: bool,
        /// Whether the sig was built from a [declare class] form. Used to skip
        /// the implicit-override (no_implicit_override) check, since ambient
        /// declarations don't have implementation bodies and shouldn't be
        /// required to carry the [override] modifier.
        pub is_declare: bool,
    }

    impl<C: ConfigTypes> Clone for Class<C> {
        fn clone(&self) -> Self {
            Class {
                id: self.id.clone(),
                class_name: self.class_name.clone(),
                class_loc: self.class_loc.clone(),
                tparams: self.tparams.clone(),
                tparams_map: self.tparams_map.clone(),
                super_: self.super_.clone(),
                constructor: self.constructor.clone(),
                static_: self.static_.clone(),
                instance: self.instance.clone(),
                abstract_: self.abstract_,
                is_declare: self.is_declare,
            }
        }
    }
}
