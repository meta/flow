/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::sync::Arc;

use dupe::Dupe;
use dupe::OptionDupedExt;
use flow_data_structure_wrapper::smol_str::FlowSmolStr;
use flow_parser::ast;
use flow_parser::ast::types::TypeInner;
use flow_parser::loc::LOC_NONE;
use flow_parser::loc::Loc;

use crate::ty::ArrT;
use crate::ty::BuiltinOrSymbol;
use crate::ty::ComponentProps;
use crate::ty::Dict;
use crate::ty::FlattenedComponentProp;
use crate::ty::FunEffect;
use crate::ty::FunParam;
use crate::ty::FunT;
use crate::ty::InterfaceT;
use crate::ty::MappedTypeHomomorphicFlag;
use crate::ty::MappedTypeOptionalFlag;
use crate::ty::MappedTypeVariance;
use crate::ty::NamedProp;
use crate::ty::ObjKind;
use crate::ty::ObjT;
use crate::ty::Polarity;
use crate::ty::Prop;
use crate::ty::RendersKind;
use crate::ty::ReturnT;
use crate::ty::TupleElement;
use crate::ty::Ty;
use crate::ty::TypeParam;
use crate::ty::bk_union;
use crate::ty::mk_union;
use crate::ty::string_of_utility_ctor;
use crate::ty::types_of_utility;
use crate::ty_printer::better_quote;
use crate::ty_printer::property_key_quotes_needed;
use crate::ty_printer::utf8_escape;
use crate::ty_symbol::Symbol;

pub type AstType = ast::types::Type<Loc, Loc>;

pub type AstTypeArgs = ast::types::TypeArgs<Loc, Loc>;

pub type AstTypeParams = ast::types::TypeParams<Loc, Loc>;

pub type AstFunction = ast::types::Function<Loc, Loc>;

pub type AstObject = ast::types::Object<Loc, Loc>;

pub type AstGeneric = ast::types::Generic<Loc, Loc>;

pub type AstIdentifier = ast::Identifier<Loc, Loc>;

fn id_from_string(x: &str) -> AstIdentifier {
    ast::Identifier::new(ast::IdentifierInner {
        loc: LOC_NONE,
        name: FlowSmolStr::new(x),
        comments: None,
    })
}

fn id_from_symbol<L>(sym: &Symbol<L>) -> AstIdentifier {
    let name = if sym.sym_anonymous {
        FlowSmolStr::new("__Anonymous__")
    } else {
        sym.sym_name.dupe()
    };
    id_from_string(&name)
}

fn mk_generic(id: AstIdentifier, targs: Option<AstTypeArgs>) -> AstGeneric {
    ast::types::Generic {
        id: ast::types::generic::Identifier::Unqualified(id),
        targs,
        comments: None,
    }
}

fn mk_targs(arguments: Vec<AstType>) -> AstTypeArgs {
    ast::types::TypeArgs {
        loc: LOC_NONE,
        arguments: arguments.into(),
        comments: None,
    }
}

fn mk_generic_type(id: AstIdentifier, targs: Option<AstTypeArgs>) -> AstType {
    ast::types::Type::new(TypeInner::Generic {
        loc: LOC_NONE,
        inner: Arc::new(mk_generic(id, targs)),
    })
}

fn mk_typeof_expr(id: AstIdentifier) -> ast::types::typeof_::Target<Loc, Loc> {
    ast::types::typeof_::Target::Unqualified(id)
}

fn builtin_from_string(name: &str, targs: Option<AstTypeArgs>) -> AstType {
    mk_generic_type(id_from_string(name), targs)
}

fn property_variance(p: Polarity) -> Option<ast::Variance<Loc>> {
    match p {
        Polarity::Neutral => None,
        Polarity::Positive => Some(ast::Variance {
            loc: LOC_NONE,
            kind: ast::VarianceKind::Readonly,
            comments: None,
        }),
        Polarity::Negative => Some(ast::Variance {
            loc: LOC_NONE,
            kind: ast::VarianceKind::Writeonly,
            comments: None,
        }),
    }
}

fn type_param_variance(p: Polarity) -> Option<ast::Variance<Loc>> {
    match p {
        Polarity::Neutral => None,
        Polarity::Positive => Some(ast::Variance {
            loc: LOC_NONE,
            kind: ast::VarianceKind::Out,
            comments: None,
        }),
        Polarity::Negative => Some(ast::Variance {
            loc: LOC_NONE,
            kind: ast::VarianceKind::In,
            comments: None,
        }),
    }
}

fn qualified2(x1: &str, x2: &str) -> ast::types::Type<Loc, Loc> {
    let q = ast::types::typeof_::Target::Unqualified(id_from_string(x1));
    let q = ast::types::typeof_::Target::Qualified(Arc::new(ast::types::typeof_::Qualified {
        loc: LOC_NONE,
        qualification: q,
        id: id_from_string(x2),
    }));
    ast::types::Type::new(TypeInner::Typeof {
        loc: LOC_NONE,
        inner: Arc::new(ast::types::Typeof {
            argument: q,
            targs: None,
            comments: None,
        }),
    })
}

pub struct Serializer;

impl Serializer {
    pub fn new() -> Self {
        Serializer
    }

    pub fn type_<L: Dupe>(&self, t: &Arc<Ty<L>>) -> AstType {
        match t.as_ref() {
            Ty::Truncated => ast::types::Type::new(TypeInner::Exists {
                loc: LOC_NONE,
                comments: None,
            }),
            Ty::Bound(data) => {
                let (_, name) = data.as_ref();
                builtin_from_string(name, None)
            }
            Ty::Generic(g) => {
                let (x, _, ts) = g.as_ref();
                self.generic_type(x, ts.as_deref())
            }
            Ty::Any(_) => ast::types::Type::new(TypeInner::Any {
                loc: LOC_NONE,
                comments: None,
            }),
            Ty::Top => ast::types::Type::new(TypeInner::Unknown {
                loc: LOC_NONE,
                comments: None,
            }),
            Ty::Bot(_) => ast::types::Type::new(TypeInner::Empty {
                loc: LOC_NONE,
                comments: None,
            }),
            Ty::Void | Ty::TsVoid => ast::types::Type::new(TypeInner::Void {
                loc: LOC_NONE,
                comments: None,
            }),
            Ty::Undefined => ast::types::Type::new(TypeInner::Undefined {
                loc: LOC_NONE,
                comments: None,
            }),
            Ty::Null => ast::types::Type::new(TypeInner::Null {
                loc: LOC_NONE,
                comments: None,
            }),
            Ty::Symbol => ast::types::Type::new(TypeInner::Symbol {
                loc: LOC_NONE,
                comments: None,
            }),
            // A `unique symbol` annotation mints a new symbol, so the only spelling
            // that an existing unique symbol satisfies is its widening.
            Ty::UniqueSymbol => ast::types::Type::new(TypeInner::Symbol {
                loc: LOC_NONE,
                comments: None,
            }),
            Ty::Num => ast::types::Type::new(TypeInner::Number {
                loc: LOC_NONE,
                comments: None,
            }),
            Ty::Str => ast::types::Type::new(TypeInner::String {
                loc: LOC_NONE,
                comments: None,
            }),
            Ty::Bool => ast::types::Type::new(TypeInner::Boolean {
                loc: LOC_NONE,
                raw: ast::types::BooleanRaw::Boolean,
                comments: None,
            }),
            Ty::BigInt => ast::types::Type::new(TypeInner::BigInt {
                loc: LOC_NONE,
                comments: None,
            }),
            Ty::NumLit(lit) => ast::types::Type::new(TypeInner::NumberLiteral {
                loc: LOC_NONE,
                literal: self.num_lit(lit),
            }),
            Ty::StrLit(lit) => ast::types::Type::new(TypeInner::StringLiteral {
                loc: LOC_NONE,
                literal: self.str_lit(lit),
            }),
            Ty::BoolLit(lit) => ast::types::Type::new(TypeInner::BooleanLiteral {
                loc: LOC_NONE,
                literal: self.bool_lit(*lit),
            }),
            Ty::BigIntLit(lit) => ast::types::Type::new(TypeInner::BigIntLiteral {
                loc: LOC_NONE,
                literal: self.bigint_lit(lit),
            }),
            Ty::Fun(f) => {
                let f = self.function_(f);
                ast::types::Type::new(TypeInner::Function {
                    loc: LOC_NONE,
                    inner: Arc::new(f),
                })
            }
            Ty::Obj(o) => self.obj_(o),
            Ty::Arr(a) => self.arr(a),
            Ty::Tup { elements, inexact } => self.tuple(elements, *inexact),
            Ty::Union(from_bounds, _, _, _) => self.union(t, *from_bounds),
            Ty::Inter(t0, t1, rest) => self.intersection(t0, t1, rest),
            Ty::Utility(crate::ty::Utility::Keys(t)) => {
                let argument = self.type_(t);
                ast::types::Type::new(TypeInner::Keyof {
                    loc: LOC_NONE,
                    inner: Arc::new(ast::types::Keyof {
                        argument,
                        comments: None,
                    }),
                })
            }
            Ty::Utility(crate::ty::Utility::ElementType(object, index)) => {
                ast::types::Type::new(TypeInner::IndexedAccess {
                    loc: LOC_NONE,
                    inner: Arc::new(ast::types::IndexedAccess {
                        object: self.type_(object),
                        index: self.type_(index),
                        comments: None,
                    }),
                })
            }
            Ty::Utility(u) => self.utility(u),
            Ty::IndexedAccess {
                _object,
                index,
                optional,
            } => {
                let object_t = self.type_(_object);
                let index_t = self.type_(index);
                let indexed_access = ast::types::IndexedAccess {
                    object: object_t,
                    index: index_t,
                    comments: None,
                };
                if *optional {
                    ast::types::Type::new(TypeInner::OptionalIndexedAccess {
                        loc: LOC_NONE,
                        inner: Arc::new(ast::types::OptionalIndexedAccess {
                            indexed_access,
                            optional: true,
                        }),
                    })
                } else {
                    ast::types::Type::new(TypeInner::IndexedAccess {
                        loc: LOC_NONE,
                        inner: Arc::new(indexed_access),
                    })
                }
            }
            Ty::InlineInterface(i) => self.inline_interface(i),
            Ty::Conditional {
                check_type,
                extends_type,
                true_type,
                false_type,
            } => {
                let check_type = self.type_(check_type);
                let extends_type = self.type_(extends_type);
                let true_type = self.type_(true_type);
                let false_type = self.type_(false_type);
                ast::types::Type::new(TypeInner::Conditional {
                    loc: LOC_NONE,
                    inner: Arc::new(ast::types::Conditional {
                        check_type,
                        extends_type,
                        true_type,
                        false_type,
                        comments: None,
                    }),
                })
            }
            Ty::Infer(data) => {
                let (s, b) = data.as_ref();
                let id = id_from_symbol(s);
                let bound = match b {
                    None => ast::types::AnnotationOrHint::Missing(LOC_NONE),
                    Some(b) => {
                        let bound_t = self.type_(b);
                        ast::types::AnnotationOrHint::Available(ast::types::Annotation {
                            loc: LOC_NONE,
                            annotation: bound_t,
                        })
                    }
                };
                let tparam = ast::types::TypeParam {
                    loc: LOC_NONE,
                    name: id,
                    bound,
                    bound_kind: ast::types::type_param::BoundKind::Extends,
                    variance: None,
                    default: None,
                    const_: None,
                };
                ast::types::Type::new(TypeInner::Infer {
                    loc: LOC_NONE,
                    inner: Arc::new(ast::types::Infer {
                        tparam,
                        comments: None,
                    }),
                })
            }
            Ty::TypeOf(data) => {
                let (builtin_or_sym, targs) = data.as_ref();
                match builtin_or_sym {
                    BuiltinOrSymbol::TSymbol(name) => {
                        let id = id_from_symbol(name);
                        let targs = targs.as_ref().map(|ts| self.type_arguments(ts));
                        ast::types::Type::new(TypeInner::Typeof {
                            loc: LOC_NONE,
                            inner: Arc::new(ast::types::Typeof {
                                argument: mk_typeof_expr(id),
                                targs,
                                comments: None,
                            }),
                        })
                    }
                    BuiltinOrSymbol::FunProto => qualified2("Object", "prototype"),
                    BuiltinOrSymbol::ObjProto => qualified2("Function", "prototype"),
                }
            }
            Ty::Component {
                regular_props,
                renders,
            } => self.component(regular_props, renders.as_ref()),
            Ty::Renders(t, kind) => ast::types::Type::new(TypeInner::Renders {
                loc: LOC_NONE,
                inner: Arc::new(self.renders(t, *kind)),
            }),
            Ty::TemplateLiteral { quasis, types } => {
                let last = quasis.len().saturating_sub(1);
                let quasis_ast: Arc<[_]> = quasis
                    .iter()
                    .enumerate()
                    .map(|(i, q)| ast::types::type_template_literal::Element {
                        loc: LOC_NONE,
                        value: ast::types::type_template_literal::Value {
                            raw: q.dupe(),
                            cooked: q.dupe(),
                        },
                        tail: i == last,
                    })
                    .collect();
                let types_ast: Arc<[_]> = types.iter().map(|t| self.type_(t)).collect();
                ast::types::Type::new(TypeInner::TemplateLiteral {
                    loc: LOC_NONE,
                    inner: Arc::new(ast::types::TypeTemplateLiteral {
                        quasis: quasis_ast,
                        types: types_ast,
                        comments: None,
                    }),
                })
            }
        }
    }

    fn renders<L: Dupe>(&self, t: &Arc<Ty<L>>, kind: RendersKind) -> ast::types::Renders<Loc, Loc> {
        let argument = self.type_(t);
        let variant = match kind {
            RendersKind::RendersNormal => ast::types::RendersVariant::Normal,
            RendersKind::RendersMaybe => ast::types::RendersVariant::Maybe,
            RendersKind::RendersStar => ast::types::RendersVariant::Star,
        };
        ast::types::Renders {
            operator_loc: LOC_NONE,
            argument,
            comments: None,
            variant,
        }
    }

    fn generic_type<L: Dupe>(&self, x: &Symbol<L>, targs: Option<&[Arc<Ty<L>>]>) -> AstType {
        let id = id_from_symbol(x);
        let targs = targs.map(|ts| self.type_arguments(ts));
        mk_generic_type(id, targs)
    }

    fn union<L: Dupe>(&self, t: &Arc<Ty<L>>, from_bounds: bool) -> AstType {
        let ts = bk_union(t);
        let has_null = ts.iter().any(|t| matches!(t.as_ref(), Ty::Null));
        let void = ts
            .iter()
            .find(|t| matches!(t.as_ref(), Ty::Void | Ty::Undefined))
            .duped();

        if has_null && let Some(void) = void {
            let filtered: Vec<Arc<Ty<L>>> = ts
                .into_iter()
                .filter(|t| !matches!(t.as_ref(), Ty::Null | Ty::Void | Ty::Undefined))
                .collect();
            match filtered.len() {
                0 => {
                    let t0 = ast::types::Type::new(TypeInner::Null {
                        loc: LOC_NONE,
                        comments: None,
                    });
                    let t1 = self.type_(&void);
                    ast::types::Type::new(TypeInner::Union {
                        loc: LOC_NONE,
                        inner: Arc::new(ast::types::Union {
                            types: (t0, t1, vec![]),
                            comments: None,
                        }),
                    })
                }
                _ => {
                    let inner_t = mk_union(from_bounds, filtered);
                    let inner_ast = match inner_t {
                        Some(t) => self.type_(&t),
                        None => ast::types::Type::new(TypeInner::Empty {
                            loc: LOC_NONE,
                            comments: None,
                        }),
                    };
                    ast::types::Type::new(TypeInner::Nullable {
                        loc: LOC_NONE,
                        inner: Arc::new(ast::types::Nullable {
                            argument: inner_ast,
                            comments: None,
                        }),
                    })
                }
            }
        } else {
            let mut ts_iter = ts.into_iter();
            let ast_t0 = self.type_(&ts_iter.next().expect("union must have at least 2 elements"));
            let ast_t1 = self.type_(&ts_iter.next().expect("union must have at least 2 elements"));
            let ast_rest: Vec<AstType> = ts_iter.map(|t| self.type_(&t)).collect();
            ast::types::Type::new(TypeInner::Union {
                loc: LOC_NONE,
                inner: Arc::new(ast::types::Union {
                    types: (ast_t0, ast_t1, ast_rest),
                    comments: None,
                }),
            })
        }
    }

    fn intersection<L: Dupe>(
        &self,
        t0: &Arc<Ty<L>>,
        t1: &Arc<Ty<L>>,
        rest: &[Arc<Ty<L>>],
    ) -> AstType {
        let ast_t0 = self.type_(t0);
        let ast_t1 = self.type_(t1);
        let ast_rest: Vec<AstType> = rest.iter().map(|t| self.type_(t)).collect();
        ast::types::Type::new(TypeInner::Intersection {
            loc: LOC_NONE,
            inner: Arc::new(ast::types::Intersection {
                types: (ast_t0, ast_t1, ast_rest),
                comments: None,
            }),
        })
    }

    fn function_<L: Dupe>(&self, f: &FunT<L>) -> AstFunction {
        let return_ = self.fun_return_t(&f.fun_return);
        let params = self.fun_params(
            f.fun_this_param.as_ref(),
            &f.fun_params,
            f.fun_rest_param.as_ref(),
        );
        let tparams = f.fun_type_params.as_ref().map(|tps| self.type_params(tps));
        let effect = match f.fun_effect {
            FunEffect::Arbitrary => ast::function::Effect::Arbitrary,
            FunEffect::Hook => ast::function::Effect::Hook,
        };
        ast::types::Function {
            tparams,
            params,
            return_,
            comments: None,
            effect,
        }
    }

    fn fun_params<L: Dupe>(
        &self,
        this_param: Option<&Arc<Ty<L>>>,
        params: &[(Option<FlowSmolStr>, Arc<Ty<L>>, FunParam)],
        rest_param: Option<&(Option<FlowSmolStr>, Arc<Ty<L>>)>,
    ) -> ast::types::function::Params<Loc, Loc> {
        let this = this_param.map(|t| ast::types::function::ThisParam {
            loc: LOC_NONE,
            annot: ast::types::Annotation {
                loc: LOC_NONE,
                annotation: self.type_(t),
            },
            comments: None,
        });
        let params: Vec<ast::types::function::Param<Loc, Loc>> = params
            .iter()
            .map(|(name, t, prm)| self.fun_param(name.as_ref(), t, prm))
            .collect();
        let rest = rest_param.map(|(name, t)| self.fun_rest_param(name.as_ref(), t));
        ast::types::function::Params {
            loc: LOC_NONE,
            this,
            params: params.into(),
            rest,
            comments: None,
        }
    }

    fn fun_param<L: Dupe>(
        &self,
        name: Option<&FlowSmolStr>,
        t: &Arc<Ty<L>>,
        prm: &FunParam,
    ) -> ast::types::function::Param<Loc, Loc> {
        let annot = self.type_(t);
        let param = match name {
            Some(n) => {
                let name = id_from_string(n.as_str());
                ast::types::function::ParamKind::Labeled {
                    name,
                    annot,
                    optional: prm.prm_optional,
                }
            }
            None => ast::types::function::ParamKind::Anonymous(annot),
        };
        ast::types::function::Param {
            loc: LOC_NONE,
            param,
        }
    }

    fn fun_rest_param<L: Dupe>(
        &self,
        name: Option<&FlowSmolStr>,
        t: &Arc<Ty<L>>,
    ) -> ast::types::function::RestParam<Loc, Loc> {
        let argument = self.fun_param(
            name,
            t,
            &FunParam {
                prm_optional: false,
            },
        );
        ast::types::function::RestParam {
            loc: LOC_NONE,
            argument,
            comments: None,
        }
    }

    fn fun_return_t<L: Dupe>(
        &self,
        ret: &ReturnT<L>,
    ) -> ast::types::function::ReturnAnnotation<Loc, Loc> {
        match ret {
            ReturnT::ReturnType(t) => {
                let t = self.type_(t);
                ast::types::function::ReturnAnnotation::Available(ast::types::Annotation {
                    loc: LOC_NONE,
                    annotation: t,
                })
            }
            ReturnT::TypeGuard(kind, x, t) => {
                let kind = *kind;
                let t = t.as_ref().map(|t| self.type_(t));
                ast::types::function::ReturnAnnotation::TypeGuard(ast::types::TypeGuard {
                    loc: LOC_NONE,
                    kind,
                    guard: (id_from_string(x), t),
                    comments: None,
                })
            }
        }
    }

    fn obj_<L: Dupe>(&self, o: &ObjT<L>) -> AstType {
        let properties: Vec<ast::types::object::Property<Loc, Loc>> = o
            .obj_props
            .iter()
            .filter_map(|p| self.obj_prop(p))
            .collect();

        let (exact, inexact, properties) = match &o.obj_kind {
            ObjKind::ExactObj => (false, false, properties),
            ObjKind::InexactObj => (false, true, properties),
            ObjKind::IndexedObj(d) => {
                let indexer = self.obj_index_prop(d);
                let mut props = vec![ast::types::object::Property::Indexer(indexer)];
                props.extend(properties);
                (false, false, props)
            }
            ObjKind::MappedTypeObj => (false, false, properties),
        };

        ast::types::Type::new(TypeInner::Object {
            loc: LOC_NONE,
            inner: Arc::new(ast::types::Object {
                exact,
                inexact,
                properties: properties.into(),
                comments: None,
            }),
        })
    }

    fn obj_prop<L: Dupe>(&self, prop: &Prop<L>) -> Option<ast::types::object::Property<Loc, Loc>> {
        match prop {
            Prop::NamedProp { name, prop, .. } => {
                // A `unique symbol` key has no string spelling: serializing it via
                // `display_smol_str` would emit `[a]`/`[symbol]` as an ordinary
                // string-keyed property, which describes a *different* property.
                // Since there is no faithful AST spelling for it here, omit it.
                // Callers that must not silently change the type reject it before
                // reaching here (see `ValidationError::SymbolKey`).
                let name = name.as_smol_str_opt()?;
                let p = self.obj_named_prop(name, prop);
                Some(ast::types::object::Property::NormalProperty(p))
            }
            Prop::CallProp(f) => {
                let p = self.obj_call_prop(f);
                Some(ast::types::object::Property::CallProperty(p))
            }
            Prop::SpreadProp(t) => {
                let p = self.obj_spread_prop(t);
                Some(ast::types::object::Property::SpreadProperty(p))
            }
            Prop::MappedTypeProp {
                key_tparam,
                source,
                prop,
                name_type,
                flags,
                homomorphic,
            } => {
                let p = self.obj_mapped_type_prop(
                    key_tparam,
                    source,
                    prop,
                    name_type.as_ref(),
                    flags,
                    homomorphic,
                );
                Some(ast::types::object::Property::MappedType(p))
            }
        }
    }

    fn obj_named_prop<L: Dupe>(
        &self,
        name: &str,
        prop: &NamedProp<L>,
    ) -> ast::types::object::NormalProperty<Loc, Loc> {
        // TODO consider making it an error to try to serialize an internal name
        let to_key = |x: &str| -> ast::expression::object::Key<Loc, Loc> {
            if property_key_quotes_needed(x) {
                let quote = better_quote(false, x);
                let raw = format!("{}{}{}", quote, utf8_escape(&quote, x), quote);
                ast::expression::object::Key::StringLiteral((
                    LOC_NONE,
                    ast::StringLiteral {
                        value: FlowSmolStr::new(x),
                        raw: FlowSmolStr::new(&raw),
                        comments: None,
                    },
                ))
            } else {
                ast::expression::object::Key::Identifier(id_from_string(x))
            }
        };
        let key = to_key(name);
        match prop {
            NamedProp::Field {
                t,
                polarity,
                optional,
            } => {
                let t = self.type_(t);
                ast::types::object::NormalProperty {
                    loc: LOC_NONE,
                    key,
                    value: ast::types::object::PropertyValue::Init(Some(t)),
                    optional: *optional,
                    static_: false,
                    proto: false,
                    method: false,
                    abstract_: false,
                    override_: false,
                    variance: property_variance(*polarity),
                    ts_accessibility: None,
                    init: None,
                    comments: None,
                }
            }
            NamedProp::Method { fun: f, optional } => {
                let fun_t = self.function_(f);
                ast::types::object::NormalProperty {
                    loc: LOC_NONE,
                    key,
                    value: ast::types::object::PropertyValue::Init(Some(ast::types::Type::new(
                        TypeInner::Function {
                            loc: LOC_NONE,
                            inner: Arc::new(fun_t),
                        },
                    ))),
                    optional: *optional,
                    static_: false,
                    proto: false,
                    method: true,
                    abstract_: false,
                    override_: false,
                    variance: None,
                    ts_accessibility: None,
                    init: None,
                    comments: None,
                }
            }
            NamedProp::Get(t) => {
                let getter_f = self.getter(t);
                ast::types::object::NormalProperty {
                    loc: LOC_NONE,
                    key,
                    value: ast::types::object::PropertyValue::Get(LOC_NONE, getter_f),
                    optional: false,
                    static_: false,
                    proto: false,
                    method: false,
                    abstract_: false,
                    override_: false,
                    variance: None,
                    ts_accessibility: None,
                    init: None,
                    comments: None,
                }
            }
            NamedProp::Set(t) => {
                let setter_f = self.setter(t);
                ast::types::object::NormalProperty {
                    loc: LOC_NONE,
                    key,
                    value: ast::types::object::PropertyValue::Set(LOC_NONE, setter_f),
                    optional: false,
                    static_: false,
                    proto: false,
                    method: false,
                    abstract_: false,
                    override_: false,
                    variance: None,
                    ts_accessibility: None,
                    init: None,
                    comments: None,
                }
            }
        }
    }

    fn obj_index_prop<L: Dupe>(&self, d: &Dict<L>) -> ast::types::object::Indexer<Loc, Loc> {
        let id = d.dict_name.as_ref().map(|n| id_from_string(n));
        let key = self.type_(&d.dict_key);
        let value = self.type_(&d.dict_value);
        ast::types::object::Indexer {
            loc: LOC_NONE,
            id,
            key,
            value,
            static_: false,
            variance: property_variance(d.dict_polarity),
            optional: false,
            comments: None,
        }
    }

    fn obj_call_prop<L: Dupe>(&self, f: &FunT<L>) -> ast::types::object::CallProperty<Loc, Loc> {
        let value = self.function_(f);
        ast::types::object::CallProperty {
            loc: LOC_NONE,
            value: (LOC_NONE, value),
            static_: false,
            comments: None,
        }
    }

    fn obj_spread_prop<L: Dupe>(
        &self,
        t: &Arc<Ty<L>>,
    ) -> ast::types::object::SpreadProperty<Loc, Loc> {
        let t = self.type_(t);
        ast::types::object::SpreadProperty {
            loc: LOC_NONE,
            argument: t,
            comments: None,
        }
    }

    fn obj_mapped_type_prop<L: Dupe>(
        &self,
        key_tparam: &TypeParam<L>,
        source: &Arc<Ty<L>>,
        prop: &Arc<Ty<L>>,
        name_type: Option<&Arc<Ty<L>>>,
        flags: &crate::ty::MappedTypeFlags,
        homomorphic: &MappedTypeHomomorphicFlag<L>,
    ) -> ast::types::object::MappedType<Loc, Loc> {
        let source_type = self.type_(source);
        let source_type = match homomorphic {
            MappedTypeHomomorphicFlag::Homomorphic => ast::types::Type::new(TypeInner::Keyof {
                loc: LOC_NONE,
                inner: Arc::new(ast::types::Keyof {
                    argument: source_type,
                    comments: None,
                }),
            }),
            MappedTypeHomomorphicFlag::SemiHomomorphic(selected_keys) => self.type_(selected_keys),
            MappedTypeHomomorphicFlag::Unspecialized => source_type,
        };
        let prop_type = self.type_(prop);
        let name_type = name_type.map(|nt| self.type_(nt));
        let key_tparam = self.type_param(key_tparam);
        let optional = match flags.optional {
            MappedTypeOptionalFlag::KeepOptionality => {
                ast::types::object::MappedTypeOptionalFlag::NoOptionalFlag
            }
            MappedTypeOptionalFlag::RemoveOptional => {
                ast::types::object::MappedTypeOptionalFlag::MinusOptional
            }
            MappedTypeOptionalFlag::MakeOptional => {
                ast::types::object::MappedTypeOptionalFlag::Optional
            }
        };
        let (variance, variance_op) = match flags.variance {
            MappedTypeVariance::OverrideVariance(pol) => (property_variance(pol), None),
            MappedTypeVariance::RemoveVariance(pol) => (
                property_variance(pol),
                Some(ast::types::object::MappedTypeVarianceOp::Remove),
            ),
            MappedTypeVariance::KeepVariance => (None, None),
        };
        ast::types::object::MappedType {
            loc: LOC_NONE,
            key_tparam,
            prop_type,
            source_type,
            name_type,
            variance,
            variance_op,
            optional,
            comments: None,
        }
    }

    fn arr<L: Dupe>(&self, a: &ArrT<L>) -> AstType {
        let t = self.type_(&a.arr_elt_t);
        if a.arr_readonly {
            builtin_from_string("ReadonlyArray", Some(mk_targs(vec![t])))
        } else {
            builtin_from_string("Array", Some(mk_targs(vec![t])))
        }
    }

    fn tuple<L: Dupe>(&self, elements: &[TupleElement<L>], inexact: bool) -> AstType {
        let els: Vec<ast::types::tuple::Element<Loc, Loc>> = elements
            .iter()
            .enumerate()
            .map(|(i, el)| self.tuple_element(i, el))
            .collect();
        ast::types::Type::new(TypeInner::Tuple {
            loc: LOC_NONE,
            inner: Arc::new(ast::types::Tuple {
                elements: els.into(),
                inexact,
                comments: None,
            }),
        })
    }

    fn tuple_element<L: Dupe>(
        &self,
        i: usize,
        el: &TupleElement<L>,
    ) -> ast::types::tuple::Element<Loc, Loc> {
        match el {
            TupleElement::TupleElement {
                name,
                t,
                polarity,
                optional,
            } => {
                let annot = self.type_(t);
                match (name, polarity) {
                    (Some(n), _) => ast::types::tuple::Element::LabeledElement {
                        loc: LOC_NONE,
                        element: ast::types::tuple::LabeledElement {
                            name: id_from_string(n),
                            annot,
                            variance: property_variance(*polarity),
                            optional: *optional,
                        },
                    },
                    (None, Polarity::Neutral) => ast::types::tuple::Element::UnlabeledElement {
                        loc: LOC_NONE,
                        annot,
                        optional: *optional,
                    },
                    _ => {
                        // No label, but has polarity - e.g. `Readonly<[string, number]>`
                        // We must make up a name.
                        let name = format!("element_{}", i);
                        ast::types::tuple::Element::LabeledElement {
                            loc: LOC_NONE,
                            element: ast::types::tuple::LabeledElement {
                                name: id_from_string(&name),
                                annot,
                                variance: property_variance(*polarity),
                                optional: *optional,
                            },
                        }
                    }
                }
            }
            TupleElement::TupleSpread { name, t } => {
                let annot = self.type_(t);
                let name = name.as_ref().map(|n| id_from_string(n));
                ast::types::tuple::Element::SpreadElement {
                    loc: LOC_NONE,
                    element: ast::types::tuple::SpreadElement { name, annot },
                }
            }
        }
    }

    fn type_params<L: Dupe>(&self, tps: &[TypeParam<L>]) -> AstTypeParams {
        let params: Vec<ast::types::TypeParam<Loc, Loc>> =
            tps.iter().map(|tp| self.type_param(tp)).collect();
        ast::types::TypeParams {
            loc: LOC_NONE,
            params: params.into(),
            comments: None,
        }
    }

    fn type_param<L: Dupe>(&self, tp: &TypeParam<L>) -> ast::types::TypeParam<Loc, Loc> {
        let bound = tp.tp_bound.as_ref().map(|b| ast::types::Annotation {
            loc: LOC_NONE,
            annotation: self.type_(b),
        });
        let default = tp.tp_default.as_ref().map(|d| self.type_(d));
        ast::types::TypeParam {
            loc: LOC_NONE,
            name: id_from_string(&tp.tp_name),
            bound: match bound {
                Some(b) => ast::types::AnnotationOrHint::Available(b),
                None => ast::types::AnnotationOrHint::Missing(LOC_NONE),
            },
            bound_kind: ast::types::type_param::BoundKind::Extends,
            variance: type_param_variance(tp.tp_polarity),
            default,
            const_: None,
        }
    }

    fn type_arguments<L: Dupe>(&self, ts: &[Arc<Ty<L>>]) -> AstTypeArgs {
        let ts: Vec<AstType> = ts.iter().map(|t| self.type_(t)).collect();
        mk_targs(ts)
    }

    fn str_lit(&self, lit: &str) -> ast::StringLiteral<Loc> {
        let quote = better_quote(false, lit);
        let raw_lit = utf8_escape(&quote, lit);
        let raw = format!("{}{}{}", quote, raw_lit, quote);
        ast::StringLiteral {
            value: FlowSmolStr::new(lit),
            raw: FlowSmolStr::new(&raw),
            comments: None,
        }
    }

    fn num_lit(&self, lit: &str) -> ast::NumberLiteral<Loc> {
        let value = lit.parse::<f64>().unwrap_or(0.0);
        ast::NumberLiteral {
            value,
            raw: FlowSmolStr::new(lit),
            comments: None,
        }
    }

    fn bool_lit(&self, lit: bool) -> ast::BooleanLiteral<Loc> {
        ast::BooleanLiteral {
            value: lit,
            comments: None,
        }
    }

    fn bigint_lit(&self, lit: &str) -> ast::BigIntLiteral<Loc> {
        let value = lit.parse::<i64>().ok();
        ast::BigIntLiteral {
            value,
            raw: FlowSmolStr::new(lit),
            comments: None,
        }
    }

    fn getter<L: Dupe>(&self, t: &Arc<Ty<L>>) -> AstFunction {
        ast::types::Function {
            tparams: None,
            params: ast::types::function::Params {
                loc: LOC_NONE,
                this: None,
                params: Vec::new().into(),
                rest: None,
                comments: None,
            },
            return_: ast::types::function::ReturnAnnotation::Available(ast::types::Annotation {
                loc: LOC_NONE,
                annotation: self.type_(t),
            }),
            comments: None,
            effect: ast::function::Effect::Arbitrary,
        }
    }

    fn setter<L: Dupe>(&self, t: &Arc<Ty<L>>) -> AstFunction {
        let param = ast::types::function::Param {
            loc: LOC_NONE,
            param: ast::types::function::ParamKind::Anonymous(self.type_(t)),
        };
        ast::types::Function {
            tparams: None,
            params: ast::types::function::Params {
                loc: LOC_NONE,
                this: None,
                params: vec![param].into(),
                rest: None,
                comments: None,
            },
            return_: ast::types::function::ReturnAnnotation::Available(ast::types::Annotation {
                loc: LOC_NONE,
                annotation: ast::types::Type::new(TypeInner::Void {
                    loc: LOC_NONE,
                    comments: None,
                }),
            }),
            comments: None,
            effect: ast::function::Effect::Arbitrary,
        }
    }

    fn inline_interface<L: Dupe>(&self, i: &InterfaceT<L>) -> AstType {
        let extends: Vec<(Loc, AstGeneric)> = i
            .if_extends
            .iter()
            .map(|e| self.interface_extends(e))
            .collect();
        let properties: Vec<ast::types::object::Property<Loc, Loc>> =
            i.if_props.iter().filter_map(|p| self.obj_prop(p)).collect();
        let properties = match &i.if_dict {
            Some(d) => {
                let indexer = self.obj_index_prop(d);
                let mut props = vec![ast::types::object::Property::Indexer(indexer)];
                props.extend(properties);
                props
            }
            None => properties,
        };
        let body = ast::types::Object {
            exact: false,
            inexact: false,
            properties: properties.into(),
            comments: None,
        };
        ast::types::Type::new(TypeInner::Interface {
            loc: LOC_NONE,
            inner: Arc::new(ast::types::Interface {
                body: (LOC_NONE, body),
                extends: extends.into(),
                comments: None,
            }),
        })
    }

    fn interface_extends<L: Dupe>(&self, e: &crate::ty::GenericT<L>) -> (Loc, AstGeneric) {
        let (x, _, ts) = e;
        let id = id_from_symbol(x);
        let targs = ts.as_ref().map(|ts| self.type_arguments(ts));
        (LOC_NONE, mk_generic(id, targs))
    }

    fn utility<L: Dupe>(&self, u: &crate::ty::Utility<L>) -> AstType {
        let ctor = string_of_utility_ctor(u);
        let ts = types_of_utility(u);
        let id = id_from_string(ctor);
        let targs = ts.map(|ts| {
            let args: Vec<AstType> = ts.iter().map(|t| self.type_(t)).collect();
            mk_targs(args)
        });
        mk_generic_type(id, targs)
    }

    fn component<L: Dupe>(
        &self,
        regular_props: &ComponentProps<L>,
        renders: Option<&Arc<Ty<L>>>,
    ) -> AstType {
        let params = match regular_props {
            ComponentProps::UnflattenedComponentProps(t) => {
                let rest_param = ast::types::component_params::RestParam {
                    loc: LOC_NONE,
                    argument: None,
                    annot: self.type_(t),
                    optional: false,
                    comments: None,
                };
                ast::types::component_params::Params {
                    loc: LOC_NONE,
                    params: Vec::new().into(),
                    rest: Some(rest_param),
                    comments: None,
                }
            }
            ComponentProps::FlattenedComponentProps {
                props,
                inexact,
                dict,
            } => {
                let params: Vec<ast::types::component_params::Param<Loc, Loc>> = props
                    .iter()
                    .filter_map(|p| self.component_param(p))
                    .collect();
                // The AST has no indexer spelling, so a dict tail degrades to
                // the same openness marker as inexactness.
                let rest = if *inexact || dict.is_some() {
                    let empty_obj = ast::types::Type::new(TypeInner::Object {
                        loc: LOC_NONE,
                        inner: Arc::new(ast::types::Object {
                            exact: false,
                            inexact: true,
                            properties: Vec::new().into(),
                            comments: None,
                        }),
                    });
                    Some(ast::types::component_params::RestParam {
                        loc: LOC_NONE,
                        argument: None,
                        annot: empty_obj,
                        optional: false,
                        comments: None,
                    })
                } else {
                    None
                };
                ast::types::component_params::Params {
                    loc: LOC_NONE,
                    params: params.into(),
                    rest,
                    comments: None,
                }
            }
        };

        let renders_annot = match renders {
            None => ast::types::ComponentRendersAnnotation::MissingRenders(LOC_NONE),
            Some(t) => match t.as_ref() {
                Ty::Renders(inner_t, kind) => {
                    ast::types::ComponentRendersAnnotation::AvailableRenders(
                        LOC_NONE,
                        self.renders(inner_t, *kind),
                    )
                }
                _ => ast::types::ComponentRendersAnnotation::AvailableRenders(
                    LOC_NONE,
                    self.renders(t, RendersKind::RendersNormal),
                ),
            },
        };

        ast::types::Type::new(TypeInner::Component {
            loc: LOC_NONE,
            inner: Arc::new(ast::types::Component {
                tparams: None,
                params,
                renders: renders_annot,
                comments: None,
            }),
        })
    }

    fn component_param<L: Dupe>(
        &self,
        prop: &FlattenedComponentProp<L>,
    ) -> Option<ast::types::component_params::Param<Loc, Loc>> {
        match prop {
            FlattenedComponentProp::FlattenedComponentProp {
                name, optional, t, ..
            } => {
                // A `unique symbol` key has no component-param spelling, so it is
                // dropped for the same reason as a symbol-keyed object property
                // (see `obj_prop`).
                let x = name.as_smol_str_opt()?.as_str();
                let param_name = if property_key_quotes_needed(x) {
                    let quote = better_quote(false, x);
                    let raw = format!("{}{}{}", quote, utf8_escape(&quote, x), quote);
                    ast::statement::component_params::ParamName::StringLiteral((
                        LOC_NONE,
                        ast::StringLiteral {
                            value: FlowSmolStr::new(x),
                            raw: FlowSmolStr::new(&raw),
                            comments: None,
                        },
                    ))
                } else {
                    ast::statement::component_params::ParamName::Identifier(id_from_string(x))
                };
                let annot = ast::types::Annotation {
                    loc: LOC_NONE,
                    annotation: self.type_(t),
                };
                Some(ast::types::component_params::Param {
                    loc: LOC_NONE,
                    name: param_name,
                    annot,
                    optional: *optional,
                })
            }
        }
    }
}

pub fn type_<L: Dupe>(t: &Arc<Ty<L>>) -> AstType {
    let serializer = Serializer::new();
    serializer.type_(t)
}
