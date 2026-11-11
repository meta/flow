/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::sync::Arc;

use dupe::Dupe;
use flow_data_structure_wrapper::smol_str::FlowSmolStr;
use flow_parser::ast;
use flow_parser::ast::expression;
use flow_parser::ast::expression::ExpressionInner;
use flow_parser::ast::function;
use flow_parser::ast::pattern;
use flow_parser::ast::statement;
use flow_parser::ast::statement::StatementInner;
use flow_parser::ast_utils;
use flow_parser::loc::Loc;
use flow_parser::loc_sig::LocSig;

pub mod identifiers {
    use super::*;

    pub fn identifier(loc: Option<Loc>, name: &str) -> ast::Identifier<Loc, Loc> {
        let loc = loc.unwrap_or_else(Loc::none);
        ast_utils::ident_of_source(None, loc, FlowSmolStr::from(name))
    }
}

pub mod types {
    use ast::types as at;

    use super::*;

    pub mod functions {
        use super::*;

        pub fn param(
            loc: Option<Loc>,
            optional: Option<bool>,
            name: Option<ast::Identifier<Loc, Loc>>,
            annot: at::Type<Loc, Loc>,
        ) -> at::function::Param<Loc, Loc> {
            let loc = loc.unwrap_or_else(Loc::none);
            let optional = optional.unwrap_or(false);
            let param = match name {
                Some(name) => at::function::ParamKind::Labeled {
                    name,
                    annot,
                    optional,
                },
                None => at::function::ParamKind::Anonymous(annot),
            };
            at::function::Param { loc, param }
        }

        pub fn params(
            loc: Option<Loc>,
            rest: Option<at::function::RestParam<Loc, Loc>>,
            this: Option<at::function::ThisParam<Loc, Loc>>,
            comments: Option<ast::Syntax<Loc, Arc<[ast::Comment<Loc>]>>>,
            params: Vec<at::function::Param<Loc, Loc>>,
        ) -> at::function::Params<Loc, Loc> {
            let loc = loc.unwrap_or_else(Loc::none);
            at::function::Params {
                loc,
                this,
                params: params.into(),
                rest,
                comments,
            }
        }

        pub fn make(
            tparams: Option<at::TypeParams<Loc, Loc>>,
            comments: Option<ast::Syntax<Loc, ()>>,
            effect_: Option<function::Effect>,
            params: at::function::Params<Loc, Loc>,
            return_: at::function::ReturnAnnotation<Loc, Loc>,
        ) -> at::Function<Loc, Loc> {
            let effect = effect_.unwrap_or(function::Effect::Arbitrary);
            at::Function {
                tparams,
                params,
                return_,
                effect,
                comments,
            }
        }
    }

    pub mod objects {
        use super::*;

        pub fn make(
            exact: Option<bool>,
            inexact: Option<bool>,
            comments: Option<ast::Syntax<Loc, Arc<[ast::Comment<Loc>]>>>,
            properties: Vec<at::object::Property<Loc, Loc>>,
        ) -> at::Object<Loc, Loc> {
            let exact = exact.unwrap_or(false);
            let inexact = inexact.unwrap_or(false);
            at::Object {
                exact,
                inexact,
                properties: properties.into(),
                comments,
            }
        }

        #[allow(clippy::too_many_arguments)]
        pub fn property(
            loc: Option<Loc>,
            optional: Option<bool>,
            static_: Option<bool>,
            proto: Option<bool>,
            method: Option<bool>,
            abstract_: Option<bool>,
            variance: Option<ast::Variance<Loc>>,
            ts_accessibility: Option<ast::class::ts_accessibility::TSAccessibility<Loc>>,
            comments: Option<ast::Syntax<Loc, ()>>,
            key: expression::object::Key<Loc, Loc>,
            value: at::object::PropertyValue<Loc, Loc>,
            init: Option<expression::Expression<Loc, Loc>>,
        ) -> at::object::NormalProperty<Loc, Loc> {
            let loc = loc.unwrap_or_else(Loc::none);
            let optional = optional.unwrap_or(false);
            let static_ = static_.unwrap_or(false);
            let proto = proto.unwrap_or(false);
            let method = method.unwrap_or(false);
            let abstract_ = abstract_.unwrap_or(false);
            at::object::NormalProperty {
                loc,
                key,
                value,
                optional,
                static_,
                proto,
                method,
                abstract_,
                override_: false,
                variance,
                ts_accessibility,
                init,
                comments,
            }
        }

        pub fn getter(
            loc: Option<Loc>,
            optional: Option<bool>,
            static_: Option<bool>,
            proto: Option<bool>,
            method: Option<bool>,
            variance: Option<ast::Variance<Loc>>,
            key: expression::object::Key<Loc, Loc>,
            func: at::Function<Loc, Loc>,
        ) -> at::object::Property<Loc, Loc> {
            let loc = loc.unwrap_or_else(Loc::none);
            let value = at::object::PropertyValue::Get(loc.dupe(), func);
            let prop = property(
                Some(loc),
                optional,
                static_,
                proto,
                method,
                None,
                variance,
                None,
                None,
                key,
                value,
                None,
            );
            at::object::Property::NormalProperty(prop)
        }

        pub fn setter(
            loc: Option<Loc>,
            optional: Option<bool>,
            static_: Option<bool>,
            proto: Option<bool>,
            method: Option<bool>,
            variance: Option<ast::Variance<Loc>>,
            key: expression::object::Key<Loc, Loc>,
            func: at::Function<Loc, Loc>,
        ) -> at::object::Property<Loc, Loc> {
            let loc = loc.unwrap_or_else(Loc::none);
            let value = at::object::PropertyValue::Set(loc.dupe(), func);
            let prop = property(
                Some(loc),
                optional,
                static_,
                proto,
                method,
                None,
                variance,
                None,
                None,
                key,
                value,
                None,
            );
            at::object::Property::NormalProperty(prop)
        }
    }

    pub fn mixed(loc: Option<Loc>, comments: Option<ast::Syntax<Loc, ()>>) -> at::Type<Loc, Loc> {
        let loc = loc.unwrap_or_else(Loc::none);
        at::Type::new(at::TypeInner::Mixed { loc, comments })
    }

    pub fn number(loc: Option<Loc>, comments: Option<ast::Syntax<Loc, ()>>) -> at::Type<Loc, Loc> {
        let loc = loc.unwrap_or_else(Loc::none);
        at::Type::new(at::TypeInner::Number { loc, comments })
    }

    pub fn empty(loc: Option<Loc>, comments: Option<ast::Syntax<Loc, ()>>) -> at::Type<Loc, Loc> {
        let loc = loc.unwrap_or_else(Loc::none);
        at::Type::new(at::TypeInner::Empty { loc, comments })
    }

    pub fn void(loc: Option<Loc>, comments: Option<ast::Syntax<Loc, ()>>) -> at::Type<Loc, Loc> {
        let loc = loc.unwrap_or_else(Loc::none);
        at::Type::new(at::TypeInner::Void { loc, comments })
    }

    pub fn annotation(t: at::Type<Loc, Loc>) -> at::Annotation<Loc, Loc> {
        at::Annotation {
            loc: Loc::none(),
            annotation: t,
        }
    }

    pub fn object_(
        loc: Option<Loc>,
        exact: Option<bool>,
        inexact: Option<bool>,
        properties: Vec<at::object::Property<Loc, Loc>>,
    ) -> at::Type<Loc, Loc> {
        let loc = loc.unwrap_or_else(Loc::none);
        at::Type::new(at::TypeInner::Object {
            loc,
            inner: Arc::new(objects::make(exact, inexact, None, properties)),
        })
    }

    pub fn type_param(
        loc: Option<Loc>,
        bound: Option<at::AnnotationOrHint<Loc, Loc>>,
        bound_kind: Option<at::type_param::BoundKind>,
        variance: Option<ast::Variance<Loc>>,
        default: Option<at::Type<Loc, Loc>>,
        const_: Option<at::type_param::ConstModifier<Loc>>,
        name: &str,
    ) -> at::TypeParam<Loc, Loc> {
        let loc = loc.unwrap_or_else(Loc::none);
        let bound = bound.unwrap_or(at::AnnotationOrHint::Missing(Loc::none()));
        let bound_kind = bound_kind.unwrap_or(at::type_param::BoundKind::Extends);
        at::TypeParam {
            loc,
            name: identifiers::identifier(None, name),
            bound,
            bound_kind,
            variance,
            default,
            const_,
        }
    }

    pub fn return_type_annotation(
        t: at::Annotation<Loc, Loc>,
    ) -> at::function::ReturnAnnotation<Loc, Loc> {
        at::function::ReturnAnnotation::Available(t)
    }

    pub fn component_renders_annotation(
        loc: Option<Loc>,
        variant: at::RendersVariant,
        argument: at::Type<Loc, Loc>,
    ) -> at::ComponentRendersAnnotation<Loc, Loc> {
        let loc = loc.unwrap_or_else(Loc::none);
        at::ComponentRendersAnnotation::AvailableRenders(
            loc,
            at::Renders {
                operator_loc: Loc::none(),
                comments: None,
                variant,
                argument,
            },
        )
    }

    pub fn component_type(
        loc: Option<Loc>,
        tparams: Option<at::TypeParams<Loc, Loc>>,
        renders: Option<at::ComponentRendersAnnotation<Loc, Loc>>,
        params: at::component_params::Params<Loc, Loc>,
    ) -> at::Type<Loc, Loc> {
        let loc = loc.unwrap_or_else(Loc::none);
        let renders =
            renders.unwrap_or(at::ComponentRendersAnnotation::MissingRenders(Loc::none()));
        at::Type::new(at::TypeInner::Component {
            loc: loc.dupe(),
            inner: Arc::new(at::Component {
                tparams,
                params: at::component_params::Params { loc, ..params },
                renders,
                comments: None,
            }),
        })
    }

    pub fn return_type_guard_annotation(
        loc: Option<Loc>,
        comments: Option<ast::Syntax<Loc, Arc<[ast::Comment<Loc>]>>>,
        x: ast::Identifier<Loc, Loc>,
        t: at::Type<Loc, Loc>,
    ) -> at::function::ReturnAnnotation<Loc, Loc> {
        let loc = loc.unwrap_or_else(Loc::none);
        at::function::ReturnAnnotation::TypeGuard(at::TypeGuard {
            loc,
            kind: at::TypeGuardKind::Default,
            guard: (x, Some(t)),
            comments,
        })
    }

    pub fn type_params(
        comments: Option<ast::Syntax<Loc, Arc<[ast::Comment<Loc>]>>>,
        loc: Option<Loc>,
        params: Vec<at::TypeParam<Loc, Loc>>,
    ) -> at::TypeParams<Loc, Loc> {
        let loc = loc.unwrap_or_else(Loc::none);
        at::TypeParams {
            loc,
            params: params.into(),
            comments,
        }
    }

    pub fn type_args(
        comments: Option<ast::Syntax<Loc, Arc<[ast::Comment<Loc>]>>>,
        loc: Option<Loc>,
        arguments: Vec<at::Type<Loc, Loc>>,
    ) -> at::TypeArgs<Loc, Loc> {
        let loc = loc.unwrap_or_else(Loc::none);
        at::TypeArgs {
            loc,
            arguments: arguments.into(),
            comments,
        }
    }

    pub fn unqualified_generic(
        comments: Option<ast::Syntax<Loc, ()>>,
        loc: Option<Loc>,
        targs: Option<at::TypeArgs<Loc, Loc>>,
        name: &str,
    ) -> at::Type<Loc, Loc> {
        let loc = loc.unwrap_or_else(Loc::none);
        at::Type::new(at::TypeInner::Generic {
            loc,
            inner: Arc::new(at::Generic {
                id: at::generic::Identifier::Unqualified(identifiers::identifier(None, name)),
                targs,
                comments,
            }),
        })
    }
}

pub fn string_literal(
    comments: Option<ast::Syntax<Loc, ()>>,
    value: &str,
) -> ast::StringLiteral<Loc> {
    ast::StringLiteral {
        value: FlowSmolStr::from(value),
        raw: FlowSmolStr::from(format!("{:?}", value)),
        comments,
    }
}

pub fn number_literal(
    comments: Option<ast::Syntax<Loc, ()>>,
    value: f64,
    raw: &str,
) -> ast::NumberLiteral<Loc> {
    ast::NumberLiteral {
        value,
        raw: FlowSmolStr::from(raw),
        comments,
    }
}

pub fn int_literal(comments: Option<ast::Syntax<Loc, ()>>, value: i32) -> ast::NumberLiteral<Loc> {
    number_literal(comments, value as f64, &format!("{}", value))
}

pub fn boolean_literal(
    comments: Option<ast::Syntax<Loc, ()>>,
    value: bool,
) -> ast::BooleanLiteral<Loc> {
    ast::BooleanLiteral { value, comments }
}

pub mod literals {
    use super::*;

    pub fn string(comments: Option<ast::Syntax<Loc, ()>>, value: &str) -> ast::StringLiteral<Loc> {
        string_literal(comments, value)
    }

    pub fn number(
        comments: Option<ast::Syntax<Loc, ()>>,
        value: f64,
        raw: &str,
    ) -> ast::NumberLiteral<Loc> {
        number_literal(comments, value, raw)
    }

    pub fn int(comments: Option<ast::Syntax<Loc, ()>>, value: i32) -> ast::NumberLiteral<Loc> {
        int_literal(comments, value)
    }

    pub fn boolean(
        comments: Option<ast::Syntax<Loc, ()>>,
        value: bool,
    ) -> ast::BooleanLiteral<Loc> {
        boolean_literal(comments, value)
    }
}

pub fn string_literal_expression(
    loc: Option<Loc>,
    comments: Option<ast::Syntax<Loc, ()>>,
    value: &str,
) -> expression::Expression<Loc, Loc> {
    let loc = loc.unwrap_or_else(Loc::none);
    expression::Expression::new(ExpressionInner::StringLiteral {
        loc,
        inner: Arc::new(string_literal(comments, value)),
    })
}

pub fn number_literal_expression(
    loc: Option<Loc>,
    comments: Option<ast::Syntax<Loc, ()>>,
    value: f64,
    raw: &str,
) -> expression::Expression<Loc, Loc> {
    let loc = loc.unwrap_or_else(Loc::none);
    expression::Expression::new(ExpressionInner::NumberLiteral {
        loc,
        inner: Arc::new(number_literal(comments, value, raw)),
    })
}

pub fn int_literal_expression(
    loc: Option<Loc>,
    comments: Option<ast::Syntax<Loc, ()>>,
    value: i32,
) -> expression::Expression<Loc, Loc> {
    let loc = loc.unwrap_or_else(Loc::none);
    expression::Expression::new(ExpressionInner::NumberLiteral {
        loc,
        inner: Arc::new(int_literal(comments, value)),
    })
}

pub fn boolean_literal_expression(
    loc: Option<Loc>,
    comments: Option<ast::Syntax<Loc, ()>>,
    value: bool,
) -> expression::Expression<Loc, Loc> {
    let loc = loc.unwrap_or_else(Loc::none);
    expression::Expression::new(ExpressionInner::BooleanLiteral {
        loc,
        inner: Arc::new(boolean_literal(comments, value)),
    })
}

pub mod patterns {
    use ast::pattern as ap;
    use ast::types as at;

    use super::*;

    pub fn identifier(
        loc: Option<Loc>,
        annot: Option<at::AnnotationOrHint<Loc, Loc>>,
        str: &str,
    ) -> ap::Pattern<Loc, Loc> {
        let loc = loc.unwrap_or_else(Loc::none);
        let annot = annot.unwrap_or(at::AnnotationOrHint::Missing(loc.dupe()));
        ap::Pattern::Identifier {
            loc: loc.dupe(),
            inner: Arc::new(ap::Identifier {
                name: ast_utils::ident_of_source(None, loc, FlowSmolStr::from(str)),
                annot,
                optional: false,
            }),
        }
    }

    pub fn array(elements: Vec<Option<ap::Pattern<Loc, Loc>>>) -> ap::Pattern<Loc, Loc> {
        let elements: Vec<ap::array::Element<Loc, Loc>> = elements
            .into_iter()
            .map(|e| match e {
                Some(i) => ap::array::Element::NormalElement(ap::array::NormalElement {
                    loc: Loc::none(),
                    argument: i,
                    default: None,
                }),
                None => ap::array::Element::Hole(Loc::none()),
            })
            .collect();
        ap::Pattern::Array {
            loc: Loc::none(),
            inner: Arc::new(ap::Array {
                elements: elements.into(),
                annot: at::AnnotationOrHint::Missing(Loc::none()),
                optional: false,
                comments: None,
            }),
        }
    }

    pub fn object_(str: &str) -> ap::Pattern<Loc, Loc> {
        ap::Pattern::Object {
            loc: Loc::none(),
            inner: Arc::new(ap::Object {
                properties: vec![ap::object::Property::NormalProperty(
                    ap::object::NormalProperty {
                        loc: Loc::none(),
                        key: ap::object::Key::Identifier(ast_utils::ident_of_source(
                            None,
                            Loc::none(),
                            FlowSmolStr::from(str),
                        )),
                        pattern: identifier(None, None, str),
                        default: None,
                        shorthand: true,
                    },
                )]
                .into(),
                annot: at::AnnotationOrHint::Missing(Loc::none()),
                optional: false,
                comments: None,
            }),
        }
    }
}

pub mod functions {
    use ast::types as at;

    use super::*;

    pub fn params(
        loc: Option<Loc>,
        rest: Option<function::RestParam<Loc, Loc>>,
        this_: Option<function::ThisParam<Loc, Loc>>,
        comments: Option<ast::Syntax<Loc, Arc<[ast::Comment<Loc>]>>>,
        ps: Vec<function::Param<Loc, Loc>>,
    ) -> function::Params<Loc, Loc> {
        let loc = loc.unwrap_or_else(Loc::none);
        function::Params {
            loc,
            this_,
            params: ps.into(),
            rest,
            comments,
        }
    }

    pub fn param(
        loc: Option<Loc>,
        default: Option<expression::Expression<Loc, Loc>>,
        argument: pattern::Pattern<Loc, Loc>,
    ) -> function::Param<Loc, Loc> {
        let loc = loc.unwrap_or_else(Loc::none);
        function::Param::RegularParam {
            loc,
            argument,
            default,
        }
    }

    pub fn rest_param(
        loc: Option<Loc>,
        comments: Option<ast::Syntax<Loc, ()>>,
        argument: pattern::Pattern<Loc, Loc>,
    ) -> function::RestParam<Loc, Loc> {
        let loc = loc.unwrap_or_else(Loc::none);
        function::RestParam {
            loc,
            argument,
            comments,
        }
    }

    pub fn body(
        loc: Option<Loc>,
        comments: Option<ast::Syntax<Loc, Arc<[ast::Comment<Loc>]>>>,
        stmts: Vec<statement::Statement<Loc, Loc>>,
    ) -> function::Body<Loc, Loc> {
        let loc = loc.unwrap_or_else(Loc::none);
        function::Body::BodyBlock((
            loc,
            statement::Block {
                body: stmts.into(),
                comments,
            },
        ))
    }

    pub fn body_expression(expr: expression::Expression<Loc, Loc>) -> function::Body<Loc, Loc> {
        function::Body::BodyExpression(expr)
    }

    pub fn make(
        id: Option<ast::Identifier<Loc, Loc>>,
        params_: Option<function::Params<Loc, Loc>>,
        tparams: Option<at::TypeParams<Loc, Loc>>,
        return_: Option<function::ReturnAnnot<Loc, Loc>>,
        generator: Option<bool>,
        effect_: Option<function::Effect>,
        async_: Option<bool>,
        body_: Option<function::Body<Loc, Loc>>,
    ) -> function::Function<Loc, Loc> {
        let return_ = return_.unwrap_or(function::ReturnAnnot::Missing(Loc::none()));
        let generator = generator.unwrap_or(false);
        let effect_ = effect_.unwrap_or(function::Effect::Arbitrary);
        let async_ = async_.unwrap_or(false);
        let params = params_.unwrap_or_else(|| params(None, None, None, None, vec![]));
        let body = body_.unwrap_or_else(|| body(None, None, vec![]));
        function::Function {
            id,
            params,
            body,
            async_,
            generator,
            effect_,
            predicate: None,
            return_,
            tparams,
            sig_loc: Loc::none(),
            comments: None,
        }
    }

    pub fn pattern_of_param(
        param: &at::function::Param<Loc, Loc>,
    ) -> Option<pattern::Pattern<Loc, Loc>> {
        match &param.param {
            at::function::ParamKind::Anonymous(_) => None,
            at::function::ParamKind::Labeled {
                name,
                annot,
                optional,
            } => Some(pattern::Pattern::Identifier {
                loc: Loc::none(),
                inner: Arc::new(pattern::Identifier {
                    name: name.dupe(),
                    annot: at::AnnotationOrHint::Available(at::Annotation {
                        loc: Loc::none(),
                        annotation: annot.dupe(),
                    }),
                    optional: *optional,
                }),
            }),
            at::function::ParamKind::Destructuring(patt) => Some(patt.clone()),
        }
    }

    pub fn param_of_type(
        param: &at::function::Param<Loc, Loc>,
    ) -> Option<function::Param<Loc, Loc>> {
        let argument = pattern_of_param(param);
        argument.map(|argument| function::Param::RegularParam {
            loc: Loc::none(),
            argument,
            default: None,
        })
    }

    pub fn rest_param_of_type(
        rest: &at::function::RestParam<Loc, Loc>,
    ) -> Option<function::RestParam<Loc, Loc>> {
        let at::function::RestParam {
            argument, comments, ..
        } = rest;
        let argument = pattern_of_param(argument);
        argument.map(|argument| function::RestParam {
            loc: Loc::none(),
            argument,
            comments: comments.dupe(),
        })
    }

    pub fn this_param_of_type(
        this_param: &at::function::ThisParam<Loc, Loc>,
    ) -> function::ThisParam<Loc, Loc> {
        let at::function::ThisParam {
            loc,
            annot,
            comments,
        } = this_param;
        function::ThisParam {
            loc: loc.dupe(),
            annot: annot.clone(),
            comments: comments.clone(),
        }
    }

    pub fn params_of_type(
        type_params: &at::function::Params<Loc, Loc>,
    ) -> Option<function::Params<Loc, Loc>> {
        let at::function::Params {
            loc,
            this: this_,
            params,
            rest,
            comments,
        } = type_params;
        let params: Option<Vec<_>> = params.iter().map(param_of_type).collect();
        params.map(|params| function::Params {
            loc: loc.dupe(),
            this_: this_.as_ref().map(this_param_of_type),
            params: params.into(),
            rest: rest.as_ref().and_then(rest_param_of_type),
            comments: comments.dupe(),
        })
    }

    pub fn of_type(
        id: Option<ast::Identifier<Loc, Loc>>,
        generator: Option<bool>,
        async_: Option<bool>,
        effect_: Option<function::Effect>,
        body_: Option<function::Body<Loc, Loc>>,
        type_fn: &at::Function<Loc, Loc>,
    ) -> Option<function::Function<Loc, Loc>> {
        let at::Function {
            tparams,
            params: type_params,
            return_: return_annot,
            ..
        } = type_fn;
        let generator = generator.unwrap_or(false);
        let async_ = async_.unwrap_or(false);
        let effect_ = effect_.unwrap_or(function::Effect::Arbitrary);
        let params = params_of_type(type_params);
        let return_ = match return_annot {
            at::function::ReturnAnnotation::Available(annot) => {
                function::ReturnAnnot::Available(at::Annotation {
                    loc: Loc::none(),
                    annotation: annot.annotation.clone(),
                })
            }
            at::function::ReturnAnnotation::TypeGuard(g) => {
                function::ReturnAnnot::TypeGuard(at::TypeGuardAnnotation {
                    loc: Loc::none(),
                    guard: g.clone(),
                })
            }
            at::function::ReturnAnnotation::Missing(loc) => {
                function::ReturnAnnot::Missing(loc.dupe())
            }
        };
        params.map(|params| {
            let body = body_.unwrap_or_else(|| body(None, None, vec![]));
            function::Function {
                id,
                params,
                body,
                async_,
                generator,
                effect_,
                predicate: None,
                return_,
                tparams: tparams.clone(),
                sig_loc: Loc::none(),
                comments: None,
            }
        })
    }
}

pub mod classes {
    use ast::class;
    use ast::types as at;

    use super::*;

    pub mod methods {
        use super::*;

        pub fn make(
            comments: Option<ast::Syntax<Loc, ()>>,
            decorators: Option<Vec<class::Decorator<Loc, Loc>>>,
            static_: Option<bool>,
            id: &str,
            function_: function::Function<Loc, Loc>,
        ) -> class::Method<Loc, Loc> {
            let decorators = decorators.unwrap_or_default();
            let static_ = static_.unwrap_or(false);
            class::Method {
                loc: Loc::none(),
                kind: class::MethodKind::Method,
                key: expression::object::Key::Identifier(identifiers::identifier(None, id)),
                value: (Loc::none(), function_),
                static_,
                override_: false,
                ts_accessibility: None,
                decorators: decorators.into(),
                comments,
            }
        }

        pub fn with_body(
            mut method_: class::Method<Loc, Loc>,
            body: function::Body<Loc, Loc>,
        ) -> class::Method<Loc, Loc> {
            let (value_loc, mut fun_) = method_.value;
            fun_.body = body;
            method_.value = (value_loc, fun_);
            method_
        }

        pub fn with_docs(
            mut method_: class::Method<Loc, Loc>,
            docs: Option<ast::Syntax<Loc, ()>>,
        ) -> class::Method<Loc, Loc> {
            method_.key = match method_.key {
                expression::object::Key::StringLiteral((t, mut lit)) => {
                    lit.comments = docs;
                    expression::object::Key::StringLiteral((t, lit))
                }
                expression::object::Key::NumberLiteral((t, mut lit)) => {
                    lit.comments = docs;
                    expression::object::Key::NumberLiteral((t, lit))
                }
                expression::object::Key::BigIntLiteral((t, mut lit)) => {
                    lit.comments = docs;
                    expression::object::Key::BigIntLiteral((t, lit))
                }
                expression::object::Key::Identifier(id) => {
                    let mut inner = (*id.0).clone();
                    inner.comments = docs;
                    expression::object::Key::Identifier(ast::Identifier::new(inner))
                }
                expression::object::Key::PrivateName(mut pn) => {
                    pn.comments = docs;
                    expression::object::Key::PrivateName(pn)
                }
                expression::object::Key::Computed(mut ck) => {
                    ck.comments = docs;
                    expression::object::Key::Computed(ck)
                }
            };
            method_
        }
    }

    pub fn implements(
        targs: Option<at::TypeArgs<Loc, Loc>>,
        id: ast::Identifier<Loc, Loc>,
    ) -> class::implements::Interface<Loc, Loc> {
        class::implements::Interface {
            loc: Loc::none(),
            id: at::generic::Identifier::Unqualified(id),
            targs,
        }
    }

    pub fn property(
        comments: Option<ast::Syntax<Loc, ()>>,
        annot: Option<at::AnnotationOrHint<Loc, Loc>>,
        static_: Option<bool>,
        variance: Option<ast::Variance<Loc>>,
        decorators: Option<Vec<class::Decorator<Loc, Loc>>>,
        id: &str,
        value: expression::Expression<Loc, Loc>,
    ) -> class::BodyElement<Loc, Loc> {
        let annot = annot.unwrap_or(at::AnnotationOrHint::Missing(Loc::none()));
        let static_ = static_.unwrap_or(false);
        let decorators = decorators.unwrap_or_default();
        class::BodyElement::Property(class::Property {
            loc: Loc::none(),
            key: expression::object::Key::Identifier(identifiers::identifier(None, id)),
            value: class::property::Value::Initialized(value),
            annot,
            static_,
            override_: false,
            optional: false,
            definite: false,
            variance,
            ts_accessibility: None,
            decorators: decorators.into(),
            comments,
        })
    }

    pub fn method_(
        comments: Option<ast::Syntax<Loc, ()>>,
        decorators: Option<Vec<class::Decorator<Loc, Loc>>>,
        static_: Option<bool>,
        id: &str,
        function_: function::Function<Loc, Loc>,
    ) -> class::BodyElement<Loc, Loc> {
        class::BodyElement::Method(methods::make(comments, decorators, static_, id, function_))
    }

    pub fn make(
        comments: Option<ast::Syntax<Loc, ()>>,
        super_: Option<expression::Expression<Loc, Loc>>,
        implements_: Option<Vec<class::implements::Interface<Loc, Loc>>>,
        abstract_: Option<bool>,
        id: Option<ast::Identifier<Loc, Loc>>,
        elements: Vec<class::BodyElement<Loc, Loc>>,
    ) -> class::Class<Loc, Loc> {
        let implements_ = implements_.unwrap_or_default();
        let abstract_ = abstract_.unwrap_or(false);
        let extends = super_.map(|expr| class::Extends {
            loc: Loc::none(),
            expr,
            targs: None,
            comments: None,
        });
        let implements = if implements_.is_empty() {
            None
        } else {
            Some(class::Implements {
                loc: Loc::none(),
                interfaces: implements_.into(),
                comments: None,
            })
        };
        class::Class {
            id,
            body: class::Body {
                loc: Loc::none(),
                body: elements.into(),
                comments: None,
            },
            tparams: None,
            extends,
            implements,
            class_decorators: vec![].into(),
            comments,
            abstract_,
        }
    }
}

pub mod jsxs {
    use ast::jsx;

    use super::*;

    pub fn identifier(
        comments: Option<ast::Syntax<Loc, ()>>,
        name: FlowSmolStr,
    ) -> jsx::Name<Loc, Loc> {
        jsx::Name::Identifier(jsx::Identifier {
            loc: Loc::none(),
            name,
            comments,
        })
    }

    pub fn attr_identifier(
        loc: Option<Loc>,
        comments: Option<ast::Syntax<Loc, ()>>,
        name: FlowSmolStr,
    ) -> jsx::attribute::Name<Loc, Loc> {
        let loc = loc.unwrap_or_else(Loc::none);
        jsx::attribute::Name::Identifier(jsx::Identifier {
            loc,
            name,
            comments,
        })
    }

    pub fn attr_literal(lit: ast::StringLiteral<Loc>) -> jsx::attribute::Value<Loc, Loc> {
        jsx::attribute::Value::StringLiteral((Loc::none(), lit))
    }

    pub fn attr(
        loc: Option<Loc>,
        name: jsx::attribute::Name<Loc, Loc>,
        value: Option<jsx::attribute::Value<Loc, Loc>>,
    ) -> jsx::OpeningAttribute<Loc, Loc> {
        let loc = loc.unwrap_or_else(Loc::none);
        jsx::OpeningAttribute::Attribute(jsx::Attribute { loc, name, value })
    }

    pub fn element(
        self_closing: Option<bool>,
        targs: Option<expression::CallTypeArgs<Loc, Loc>>,
        attributes: Option<Vec<jsx::OpeningAttribute<Loc, Loc>>>,
        children: Option<Vec<jsx::Child<Loc, Loc>>>,
        comments: Option<ast::Syntax<Loc, ()>>,
        name: jsx::Name<Loc, Loc>,
    ) -> jsx::Element<Loc, Loc> {
        let self_closing = self_closing.unwrap_or(false);
        let attributes = attributes.unwrap_or_default();
        let children = children.unwrap_or_default();
        jsx::Element {
            opening_element: jsx::Opening {
                loc: Loc::none(),
                name: name.clone(),
                targs,
                self_closing,
                attributes: attributes.into(),
                props: Loc::none(),
            },
            closing_element: if self_closing {
                None
            } else {
                Some(jsx::Closing {
                    loc: Loc::none(),
                    name,
                })
            },
            children: (Loc::none(), children),
            comments,
        }
    }

    pub fn child_element(
        loc: Option<Loc>,
        selfclosing: Option<bool>,
        attrs: Option<Vec<jsx::OpeningAttribute<Loc, Loc>>>,
        children: Option<Vec<jsx::Child<Loc, Loc>>>,
        name: jsx::Name<Loc, Loc>,
    ) -> jsx::Child<Loc, Loc> {
        let loc = loc.unwrap_or_else(Loc::none);
        jsx::Child::Element {
            loc,
            inner: element(selfclosing, None, attrs, children, None, name),
        }
    }

    pub fn closing(loc: Option<Loc>, name: jsx::Name<Loc, Loc>) -> jsx::Closing<Loc, Loc> {
        let loc = loc.unwrap_or_else(Loc::none);
        jsx::Closing { loc, name }
    }
}

pub mod statements {
    use ast::types as at;

    use super::*;

    pub fn empty(comments: Option<ast::Syntax<Loc, ()>>) -> statement::Statement<Loc, Loc> {
        statement::Statement::new(StatementInner::Empty {
            loc: Loc::none(),
            inner: Arc::new(statement::Empty { comments }),
        })
    }

    pub fn block(
        comments: Option<ast::Syntax<Loc, Arc<[ast::Comment<Loc>]>>>,
        children: Vec<statement::Statement<Loc, Loc>>,
    ) -> statement::Statement<Loc, Loc> {
        statement::Statement::new(StatementInner::Block {
            loc: Loc::none(),
            inner: Arc::new(statement::Block {
                body: children.into(),
                comments,
            }),
        })
    }

    pub fn while_(
        test: expression::Expression<Loc, Loc>,
        comments: Option<ast::Syntax<Loc, ()>>,
        body: statement::Statement<Loc, Loc>,
    ) -> statement::Statement<Loc, Loc> {
        statement::Statement::new(StatementInner::While {
            loc: Loc::none(),
            inner: Arc::new(statement::While {
                test,
                body,
                comments,
            }),
        })
    }

    pub fn do_while(
        body: statement::Statement<Loc, Loc>,
        comments: Option<ast::Syntax<Loc, ()>>,
        test: expression::Expression<Loc, Loc>,
    ) -> statement::Statement<Loc, Loc> {
        statement::Statement::new(StatementInner::DoWhile {
            loc: Loc::none(),
            inner: Arc::new(statement::DoWhile {
                body,
                test,
                comments,
            }),
        })
    }

    pub fn for_raw(
        comments: Option<ast::Syntax<Loc, ()>>,
        init: Option<statement::for_::Init<Loc, Loc>>,
        test: Option<expression::Expression<Loc, Loc>>,
        update: Option<expression::Expression<Loc, Loc>>,
        body: statement::Statement<Loc, Loc>,
    ) -> statement::Statement<Loc, Loc> {
        statement::Statement::new(StatementInner::For {
            loc: Loc::none(),
            inner: Arc::new(statement::For {
                init,
                test,
                update,
                body,
                comments,
            }),
        })
    }

    pub fn for_(
        comments: Option<ast::Syntax<Loc, ()>>,
        init: expression::Expression<Loc, Loc>,
        test: Option<expression::Expression<Loc, Loc>>,
        update: Option<expression::Expression<Loc, Loc>>,
        body: statement::Statement<Loc, Loc>,
    ) -> statement::Statement<Loc, Loc> {
        statement::Statement::new(StatementInner::For {
            loc: Loc::none(),
            inner: Arc::new(statement::For {
                init: Some(statement::for_::Init::InitExpression(init)),
                test,
                update,
                body,
                comments,
            }),
        })
    }

    pub fn for_in(
        comments: Option<ast::Syntax<Loc, ()>>,
        left: statement::for_in::Left<Loc, Loc>,
        right: expression::Expression<Loc, Loc>,
        body: statement::Statement<Loc, Loc>,
    ) -> statement::Statement<Loc, Loc> {
        statement::Statement::new(StatementInner::ForIn {
            loc: Loc::none(),
            inner: Arc::new(statement::ForIn {
                left,
                right,
                body,
                comments,
            }),
        })
    }

    pub fn for_in_declarator(
        kind: Option<ast::VariableKind>,
        comments: Option<ast::Syntax<Loc, ()>>,
        declarations: Vec<statement::variable::Declarator<Loc, Loc>>,
    ) -> statement::for_in::Left<Loc, Loc> {
        let kind = kind.unwrap_or(ast::VariableKind::Var);
        statement::for_in::Left::LeftDeclaration((
            Loc::none(),
            statement::VariableDeclaration {
                declarations: declarations.into(),
                kind,
                comments,
            },
        ))
    }

    pub fn for_in_pattern(patt: pattern::Pattern<Loc, Loc>) -> statement::for_in::Left<Loc, Loc> {
        statement::for_in::Left::LeftPattern(patt)
    }

    pub fn for_of(
        await_: Option<bool>,
        comments: Option<ast::Syntax<Loc, ()>>,
        left: statement::for_of::Left<Loc, Loc>,
        right: expression::Expression<Loc, Loc>,
        body: statement::Statement<Loc, Loc>,
    ) -> statement::Statement<Loc, Loc> {
        let await_ = await_.unwrap_or(false);
        statement::Statement::new(StatementInner::ForOf {
            loc: Loc::none(),
            inner: Arc::new(statement::ForOf {
                left,
                right,
                body,
                await_,
                comments,
            }),
        })
    }

    pub fn for_of_declarator(
        kind: Option<ast::VariableKind>,
        comments: Option<ast::Syntax<Loc, ()>>,
        declarations: Vec<statement::variable::Declarator<Loc, Loc>>,
    ) -> statement::for_of::Left<Loc, Loc> {
        let kind = kind.unwrap_or(ast::VariableKind::Var);
        statement::for_of::Left::LeftDeclaration((
            Loc::none(),
            statement::VariableDeclaration {
                declarations: declarations.into(),
                kind,
                comments,
            },
        ))
    }

    pub fn for_of_pattern(patt: pattern::Pattern<Loc, Loc>) -> statement::for_of::Left<Loc, Loc> {
        statement::for_of::Left::LeftPattern(patt)
    }

    pub fn expression(
        loc: Option<Loc>,
        directive: Option<String>,
        comments: Option<ast::Syntax<Loc, ()>>,
        expr: expression::Expression<Loc, Loc>,
    ) -> statement::Statement<Loc, Loc> {
        let loc = loc.unwrap_or_else(Loc::none);
        statement::Statement::new(StatementInner::Expression {
            loc,
            inner: Arc::new(statement::Expression {
                expression: expr,
                directive,
                comments,
            }),
        })
    }

    pub fn labeled(
        comments: Option<ast::Syntax<Loc, ()>>,
        label: ast::Identifier<Loc, Loc>,
        body: statement::Statement<Loc, Loc>,
    ) -> statement::Statement<Loc, Loc> {
        statement::Statement::new(StatementInner::Labeled {
            loc: Loc::none(),
            inner: Arc::new(statement::Labeled {
                label,
                body,
                comments,
            }),
        })
    }

    pub fn variable_declarator_generic(
        loc: Option<Loc>,
        id: pattern::Pattern<Loc, Loc>,
        init: Option<expression::Expression<Loc, Loc>>,
    ) -> statement::variable::Declarator<Loc, Loc> {
        let loc = loc.unwrap_or_else(Loc::none);
        statement::variable::Declarator {
            loc,
            id,
            init,
            definite: false,
        }
    }

    pub fn variable_declarator(
        loc: Option<Loc>,
        init: Option<expression::Expression<Loc, Loc>>,
        annot: Option<at::AnnotationOrHint<Loc, Loc>>,
        str: &str,
    ) -> statement::variable::Declarator<Loc, Loc> {
        variable_declarator_generic(
            loc.dupe(),
            super::patterns::identifier(loc, annot, str),
            init,
        )
    }

    pub fn variable_declaration(
        kind: Option<ast::VariableKind>,
        loc: Option<Loc>,
        comments: Option<ast::Syntax<Loc, ()>>,
        declarations: Vec<statement::variable::Declarator<Loc, Loc>>,
    ) -> statement::Statement<Loc, Loc> {
        let kind = kind.unwrap_or(ast::VariableKind::Var);
        let loc = loc.unwrap_or_else(Loc::none);
        statement::Statement::new(StatementInner::VariableDeclaration {
            loc,
            inner: Arc::new(statement::VariableDeclaration {
                declarations: declarations.into(),
                kind,
                comments,
            }),
        })
    }

    pub fn let_declaration(
        loc: Option<Loc>,
        declarations: Vec<statement::variable::Declarator<Loc, Loc>>,
    ) -> statement::Statement<Loc, Loc> {
        variable_declaration(Some(ast::VariableKind::Let), loc, None, declarations)
    }

    pub fn const_declaration(
        loc: Option<Loc>,
        comments: Option<ast::Syntax<Loc, ()>>,
        declarations: Vec<statement::variable::Declarator<Loc, Loc>>,
    ) -> statement::Statement<Loc, Loc> {
        variable_declaration(Some(ast::VariableKind::Const), loc, comments, declarations)
    }

    pub fn function_declaration(
        loc: Option<Loc>,
        async_: Option<bool>,
        generator: Option<bool>,
        params: Option<function::Params<Loc, Loc>>,
        body: Option<function::Body<Loc, Loc>>,
        id: ast::Identifier<Loc, Loc>,
    ) -> statement::Statement<Loc, Loc> {
        let loc = loc.unwrap_or_else(Loc::none);
        let fn_ =
            super::functions::make(Some(id), params, None, None, generator, None, async_, body);
        statement::Statement::new(StatementInner::FunctionDeclaration {
            loc,
            inner: Arc::new(fn_),
        })
    }

    pub fn class_declaration(
        super_: Option<expression::Expression<Loc, Loc>>,
        implements: Option<Vec<ast::class::implements::Interface<Loc, Loc>>>,
        id: Option<ast::Identifier<Loc, Loc>>,
        elements: Vec<ast::class::BodyElement<Loc, Loc>>,
    ) -> statement::Statement<Loc, Loc> {
        statement::Statement::new(StatementInner::ClassDeclaration {
            loc: Loc::none(),
            inner: Arc::new(super::classes::make(
                None, super_, implements, None, id, elements,
            )),
        })
    }

    pub fn import_declaration(
        loc: Option<Loc>,
        comments: Option<ast::Syntax<Loc, ()>>,
        attributes: Option<(
            Loc,
            Vec<statement::import_declaration::ImportAttribute<Loc, Loc>>,
        )>,
        import_kind: statement::ImportKind,
        source: (Loc, ast::StringLiteral<Loc>),
        default: Option<statement::import_declaration::DefaultIdentifier<Loc, Loc>>,
        specifiers: Option<statement::import_declaration::Specifier<Loc, Loc>>,
    ) -> statement::Statement<Loc, Loc> {
        let loc = loc.unwrap_or_else(Loc::none);
        statement::Statement::new(StatementInner::ImportDeclaration {
            loc,
            inner: Arc::new(statement::ImportDeclaration {
                import_kind,
                // Synthesised imports are never phase-modified.
                phase: None,
                source,
                default,
                specifiers,
                attributes,
                comments,
            }),
        })
    }

    pub fn named_import_specifier(
        kind: Option<statement::ImportKind>,
        kind_loc: Option<Loc>,
        local: Option<ast::Identifier<Loc, Loc>>,
        remote: ast::Identifier<Loc, Loc>,
    ) -> statement::import_declaration::NamedSpecifier<Loc, Loc> {
        statement::import_declaration::NamedSpecifier {
            kind,
            kind_loc,
            local,
            remote,
            remote_name_def_loc: None,
        }
    }

    pub fn named_import_declaration(
        loc: Option<Loc>,
        comments: Option<ast::Syntax<Loc, ()>>,
        import_kind: statement::ImportKind,
        source: (Loc, ast::StringLiteral<Loc>),
        specifiers: Vec<statement::import_declaration::NamedSpecifier<Loc, Loc>>,
    ) -> statement::Statement<Loc, Loc> {
        let default = None;
        import_declaration(
            loc,
            comments,
            None,
            import_kind,
            source,
            default,
            Some(statement::import_declaration::Specifier::ImportNamedSpecifiers(specifiers)),
        )
    }

    pub fn if_(
        loc: Option<Loc>,
        comments: Option<ast::Syntax<Loc, ()>>,
        test: expression::Expression<Loc, Loc>,
        consequent: statement::Statement<Loc, Loc>,
        alternate: Option<statement::if_::Alternate<Loc, Loc>>,
    ) -> statement::Statement<Loc, Loc> {
        let loc = loc.unwrap_or_else(Loc::none);
        statement::Statement::new(StatementInner::If {
            loc,
            inner: Arc::new(statement::If {
                test,
                consequent,
                alternate,
                comments,
            }),
        })
    }

    pub fn if_alternate(
        loc: Option<Loc>,
        comments: Option<ast::Syntax<Loc, ()>>,
        body: statement::Statement<Loc, Loc>,
    ) -> statement::if_::Alternate<Loc, Loc> {
        let loc = loc.unwrap_or_else(Loc::none);
        statement::if_::Alternate {
            loc,
            body,
            comments,
        }
    }

    pub fn return_(
        loc: Option<Loc>,
        comments: Option<ast::Syntax<Loc, ()>>,
        expr: Option<expression::Expression<Loc, Loc>>,
    ) -> statement::Statement<Loc, Loc> {
        let loc = loc.unwrap_or_else(Loc::none);
        statement::Statement::new(StatementInner::Return {
            loc: loc.dupe(),
            inner: Arc::new(statement::Return {
                argument: expr,
                comments,
                return_out: loc,
            }),
        })
    }

    pub fn directive(loc: Option<Loc>, txt: &str) -> statement::Statement<Loc, Loc> {
        expression(
            loc.dupe(),
            Some(txt.to_owned()),
            None,
            super::string_literal_expression(loc, None, txt),
        )
    }

    pub fn switch(
        loc: Option<Loc>,
        comments: Option<ast::Syntax<Loc, ()>>,
        discriminant: expression::Expression<Loc, Loc>,
        cases: Vec<statement::switch::Case<Loc, Loc>>,
    ) -> statement::Statement<Loc, Loc> {
        let loc = loc.unwrap_or_else(Loc::none);
        let exhaustive_out = discriminant.loc().dupe();
        statement::Statement::new(StatementInner::Switch {
            loc,
            inner: Arc::new(statement::Switch {
                discriminant,
                cases: cases.into(),
                comments,
                exhaustive_out,
            }),
        })
    }

    pub fn switch_case(
        loc: Option<Loc>,
        test: Option<expression::Expression<Loc, Loc>>,
        comments: Option<ast::Syntax<Loc, ()>>,
        case_test_loc: Option<Loc>,
        consequent: Vec<statement::Statement<Loc, Loc>>,
    ) -> statement::switch::Case<Loc, Loc> {
        let loc = loc.unwrap_or_else(Loc::none);
        statement::switch::Case {
            loc,
            test,
            case_test_loc,
            consequent: consequent.into(),
            comments,
        }
    }

    pub fn break_(
        loc: Option<Loc>,
        comments: Option<ast::Syntax<Loc, ()>>,
        label: Option<ast::Identifier<Loc, Loc>>,
    ) -> statement::Statement<Loc, Loc> {
        let loc = loc.unwrap_or_else(Loc::none);
        statement::Statement::new(StatementInner::Break {
            loc,
            inner: Arc::new(statement::Break { label, comments }),
        })
    }

    pub fn try_(
        loc: Option<Loc>,
        comments: Option<ast::Syntax<Loc, ()>>,
        handler: Option<statement::try_::CatchClause<Loc, Loc>>,
        finalizer: Option<(Loc, statement::Block<Loc, Loc>)>,
        stmts: Vec<statement::Statement<Loc, Loc>>,
    ) -> statement::Statement<Loc, Loc> {
        let loc = loc.unwrap_or_else(Loc::none);
        let block = (
            loc.dupe(),
            statement::Block {
                body: stmts.into(),
                comments: None,
            },
        );
        statement::Statement::new(StatementInner::Try {
            loc,
            inner: Arc::new(statement::Try {
                block,
                handler,
                finalizer,
                comments,
            }),
        })
    }

    pub fn type_alias(
        loc: Option<Loc>,
        comments: Option<ast::Syntax<Loc, ()>>,
        tparams: Option<at::TypeParams<Loc, Loc>>,
        name: &str,
        right: at::Type<Loc, Loc>,
    ) -> statement::Statement<Loc, Loc> {
        let loc = loc.unwrap_or_else(Loc::none);
        statement::Statement::new(StatementInner::TypeAlias {
            loc,
            inner: Arc::new(statement::TypeAlias {
                id: identifiers::identifier(None, name),
                tparams,
                right,
                comments,
            }),
        })
    }

    pub fn catch(
        loc: Option<Loc>,
        comments: Option<ast::Syntax<Loc, ()>>,
        param: Option<pattern::Pattern<Loc, Loc>>,
        stmts: Vec<statement::Statement<Loc, Loc>>,
    ) -> statement::try_::CatchClause<Loc, Loc> {
        let loc = loc.unwrap_or_else(Loc::none);
        let body = (
            loc.dupe(),
            statement::Block {
                body: stmts.into(),
                comments: None,
            },
        );
        statement::try_::CatchClause {
            loc,
            param,
            body,
            comments,
        }
    }

    pub fn with_(
        loc: Option<Loc>,
        comments: Option<ast::Syntax<Loc, ()>>,
        object: expression::Expression<Loc, Loc>,
        body: statement::Statement<Loc, Loc>,
    ) -> statement::Statement<Loc, Loc> {
        let loc = loc.unwrap_or_else(Loc::none);
        statement::Statement::new(StatementInner::With {
            loc,
            inner: Arc::new(statement::With {
                object,
                body,
                comments,
            }),
        })
    }

    pub fn enum_declaration(
        loc: Option<Loc>,
        comments: Option<ast::Syntax<Loc, ()>>,
        const_: Option<bool>,
        id: ast::Identifier<Loc, Loc>,
        body: statement::enum_declaration::Body<Loc>,
    ) -> statement::Statement<Loc, Loc> {
        let loc = loc.unwrap_or_else(Loc::none);
        let const_ = const_.unwrap_or(false);
        statement::Statement::new(StatementInner::EnumDeclaration {
            loc,
            inner: Arc::new(statement::EnumDeclaration {
                id,
                body,
                const_,
                comments,
            }),
        })
    }

    pub mod enum_declarations {
        use statement::enum_declaration;

        use super::*;

        pub fn initialized_member<I>(
            loc: Option<Loc>,
            id: enum_declaration::MemberName<Loc>,
            init_value: I,
        ) -> enum_declaration::InitializedMember<I, Loc> {
            let loc = loc.unwrap_or_else(Loc::none);
            enum_declaration::InitializedMember {
                loc: loc.dupe(),
                id,
                init: (loc, init_value),
            }
        }

        pub fn defaulted_member(
            loc: Option<Loc>,
            id: enum_declaration::MemberName<Loc>,
        ) -> enum_declaration::DefaultedMember<Loc> {
            let loc = loc.unwrap_or_else(Loc::none);
            enum_declaration::DefaultedMember { loc, id }
        }

        pub fn boolean_member(
            m: enum_declaration::InitializedMember<ast::BooleanLiteral<Loc>, Loc>,
        ) -> enum_declaration::Member<Loc> {
            enum_declaration::Member::BooleanMember(m)
        }

        pub fn number_member(
            m: enum_declaration::InitializedMember<ast::NumberLiteral<Loc>, Loc>,
        ) -> enum_declaration::Member<Loc> {
            enum_declaration::Member::NumberMember(m)
        }

        pub fn string_member(
            m: enum_declaration::InitializedMember<ast::StringLiteral<Loc>, Loc>,
        ) -> enum_declaration::Member<Loc> {
            enum_declaration::Member::StringMember(m)
        }

        pub fn bigint_member(
            m: enum_declaration::InitializedMember<ast::BigIntLiteral<Loc>, Loc>,
        ) -> enum_declaration::Member<Loc> {
            enum_declaration::Member::BigIntMember(m)
        }

        pub fn defaulted_member_of(
            m: enum_declaration::DefaultedMember<Loc>,
        ) -> enum_declaration::Member<Loc> {
            enum_declaration::Member::DefaultedMember(m)
        }

        pub fn body(
            loc: Loc,
            members: Arc<[enum_declaration::Member<Loc>]>,
            explicit_type: Option<(Loc, enum_declaration::ExplicitType)>,
            has_unknown_members: Option<Loc>,
            comments: Option<ast::Syntax<Loc, Arc<[ast::Comment<Loc>]>>>,
        ) -> enum_declaration::Body<Loc> {
            enum_declaration::Body {
                loc,
                members,
                explicit_type,
                has_unknown_members,
                comments,
            }
        }
    }

    pub fn component_id_param(
        loc: Option<Loc>,
        default: Option<expression::Expression<Loc, Loc>>,
        local: Option<pattern::Pattern<Loc, Loc>>,
        name: &str,
    ) -> statement::component_params::Param<Loc, Loc> {
        let loc = loc.unwrap_or_else(Loc::none);
        let shorthand = local.is_none();
        let local = local.unwrap_or_else(|| super::patterns::identifier(None, None, name));
        statement::component_params::Param {
            loc,
            name: statement::component_params::ParamName::Identifier(identifiers::identifier(
                None, name,
            )),
            local,
            default,
            shorthand,
        }
    }

    pub fn component_string_param(
        loc: Option<Loc>,
        default: Option<expression::Expression<Loc, Loc>>,
        name: &str,
        local: pattern::Pattern<Loc, Loc>,
    ) -> statement::component_params::Param<Loc, Loc> {
        let loc = loc.unwrap_or_else(Loc::none);
        statement::component_params::Param {
            loc,
            name: statement::component_params::ParamName::StringLiteral((
                Loc::none(),
                super::string_literal(None, name),
            )),
            local,
            default,
            shorthand: false,
        }
    }

    pub fn component_params(
        loc: Option<Loc>,
        rest: Option<statement::component_params::RestParam<Loc, Loc>>,
        comments: Option<ast::Syntax<Loc, Arc<[ast::Comment<Loc>]>>>,
        params: Vec<statement::component_params::Param<Loc, Loc>>,
    ) -> statement::component_params::Params<Loc, Loc> {
        let loc = loc.unwrap_or_else(Loc::none);
        statement::component_params::Params {
            loc,
            params: params.into(),
            rest,
            comments,
        }
    }

    pub fn component_declaration(
        loc: Option<Loc>,
        async_: Option<bool>,
        tparams: Option<at::TypeParams<Loc, Loc>>,
        params: Option<statement::component_params::Params<Loc, Loc>>,
        renders: Option<at::ComponentRendersAnnotation<Loc, Loc>>,
        comments: Option<ast::Syntax<Loc, ()>>,
        id: &str,
        body: Option<(Loc, statement::Block<Loc, Loc>)>,
    ) -> statement::Statement<Loc, Loc> {
        let loc = loc.unwrap_or_else(Loc::none);
        let async_ = async_.unwrap_or(false);
        let params = params.unwrap_or_else(|| component_params(None, None, None, vec![]));
        let renders =
            renders.unwrap_or(at::ComponentRendersAnnotation::MissingRenders(Loc::none()));
        statement::Statement::new(StatementInner::ComponentDeclaration {
            loc,
            inner: Arc::new(statement::ComponentDeclaration {
                id: identifiers::identifier(None, id),
                body,
                tparams,
                params,
                renders,
                async_,
                comments,
                sig_loc: Loc::none(),
            }),
        })
    }

    pub fn component_type_param(
        loc: Option<Loc>,
        optional: Option<bool>,
        name: statement::component_params::ParamName<Loc, Loc>,
        annot: at::Annotation<Loc, Loc>,
    ) -> at::component_params::Param<Loc, Loc> {
        let loc = loc.unwrap_or_else(Loc::none);
        let optional = optional.unwrap_or(false);
        at::component_params::Param {
            loc,
            name,
            annot,
            optional,
        }
    }

    pub fn component_type_params(
        loc: Option<Loc>,
        rest: Option<at::component_params::RestParam<Loc, Loc>>,
        comments: Option<ast::Syntax<Loc, Arc<[ast::Comment<Loc>]>>>,
        params: Vec<at::component_params::Param<Loc, Loc>>,
    ) -> at::component_params::Params<Loc, Loc> {
        let loc = loc.unwrap_or_else(Loc::none);
        at::component_params::Params {
            loc,
            params: params.into(),
            rest,
            comments,
        }
    }

    pub fn declare_component(
        loc: Option<Loc>,
        tparams: Option<at::TypeParams<Loc, Loc>>,
        params: Option<statement::component_params::Params<Loc, Loc>>,
        comments: Option<ast::Syntax<Loc, ()>>,
        renders: Option<at::ComponentRendersAnnotation<Loc, Loc>>,
        id: &str,
    ) -> statement::Statement<Loc, Loc> {
        let loc = loc.unwrap_or_else(Loc::none);
        let renders =
            renders.unwrap_or(at::ComponentRendersAnnotation::MissingRenders(Loc::none()));
        let params = params.unwrap_or_else(|| component_params(None, None, None, vec![]));
        statement::Statement::new(StatementInner::DeclareComponent {
            loc,
            inner: Arc::new(statement::DeclareComponent {
                id: identifiers::identifier(None, id),
                tparams,
                params,
                renders,
                comments,
            }),
        })
    }

    pub fn synthesized_component_declaration(
        use_component_syntax: bool,
        tparams: Option<at::TypeParams<Loc, Loc>>,
        typed_props: Vec<(&str, at::Type<Loc, Loc>)>,
        body: Vec<statement::Statement<Loc, Loc>>,
        new_component_name: &str,
    ) -> statement::Statement<Loc, Loc> {
        let component_body = (
            Loc::none(),
            statement::Block {
                body: body.into(),
                comments: None,
            },
        );
        if use_component_syntax {
            let params: Vec<_> = typed_props
                .iter()
                .map(|(def, t)| statement::component_params::Param {
                    loc: Loc::none(),
                    name: statement::component_params::ParamName::Identifier(
                        identifiers::identifier(None, def),
                    ),
                    local: super::patterns::identifier(
                        None,
                        Some(at::AnnotationOrHint::Available(at::Annotation {
                            loc: Loc::none(),
                            annotation: t.dupe(),
                        })),
                        def,
                    ),
                    default: None,
                    shorthand: true,
                })
                .collect();
            let params = component_params(None, None, None, params);
            component_declaration(
                None,
                None,
                tparams,
                Some(params),
                None,
                None,
                new_component_name,
                Some(component_body),
            )
        } else {
            let function_body = function::Body::BodyBlock(component_body);
            let params = if typed_props.is_empty() {
                None
            } else {
                let properties: Vec<_> = typed_props
                    .iter()
                    .map(|(def, _)| {
                        ast::pattern::object::Property::NormalProperty(
                            ast::pattern::object::NormalProperty {
                                loc: Loc::none(),
                                key: ast::pattern::object::Key::Identifier(
                                    ast_utils::ident_of_source(
                                        None,
                                        Loc::none(),
                                        FlowSmolStr::from(*def),
                                    ),
                                ),
                                pattern: super::patterns::identifier(None, None, def),
                                default: None,
                                shorthand: true,
                            },
                        )
                    })
                    .collect();
                let properties_annot: Vec<_> = typed_props
                    .iter()
                    .map(|(def, t)| {
                        at::object::Property::NormalProperty(super::types::objects::property(
                            None,
                            None,
                            None,
                            None,
                            None,
                            None,
                            None,
                            None,
                            None,
                            expression::object::Key::Identifier(identifiers::identifier(None, def)),
                            at::object::PropertyValue::Init(Some(t.clone())),
                            None,
                        ))
                    })
                    .collect();
                let annot = at::AnnotationOrHint::Available(at::Annotation {
                    loc: Loc::none(),
                    annotation: at::Type::new(at::TypeInner::Generic {
                        loc: Loc::none(),
                        inner: Arc::new(at::Generic {
                            id: at::generic::Identifier::Unqualified(identifiers::identifier(
                                None, "Readonly",
                            )),
                            targs: Some(super::types::type_args(
                                None,
                                None,
                                vec![
                                    // Do not use explicit bar or ...
                                    super::types::object_(
                                        None,
                                        Some(false),
                                        Some(false),
                                        properties_annot,
                                    ),
                                ],
                            )),
                            comments: None,
                        }),
                    }),
                });
                Some(super::functions::params(
                    None,
                    None,
                    None,
                    None,
                    vec![super::functions::param(
                        None,
                        None,
                        ast::pattern::Pattern::Object {
                            loc: Loc::none(),
                            inner: Arc::new(ast::pattern::Object {
                                properties: properties.into(),
                                annot,
                                optional: false,
                                comments: None,
                            }),
                        },
                    )],
                ))
            };
            statement::Statement::new(StatementInner::FunctionDeclaration {
                loc: Loc::none(),
                inner: Arc::new(super::functions::make(
                    Some(identifiers::identifier(None, new_component_name)),
                    params,
                    tparams,
                    None,
                    None,
                    None,
                    None,
                    Some(function_body),
                )),
            })
        }
    }
}

pub mod expressions {
    use ast::types as at;

    use super::*;

    pub fn identifier(
        loc: Option<Loc>,
        comments: Option<ast::Syntax<Loc, ()>>,
        name: &str,
    ) -> expression::Expression<Loc, Loc> {
        let loc = loc.unwrap_or_else(Loc::none);
        expression::Expression::new(ExpressionInner::Identifier {
            loc: loc.dupe(),
            inner: ast::Identifier::new(ast::IdentifierInner {
                loc,
                name: FlowSmolStr::from(name),
                comments,
            }),
        })
    }

    pub fn array(
        loc: Option<Loc>,
        comments: Option<ast::Syntax<Loc, Arc<[ast::Comment<Loc>]>>>,
        elements: Vec<expression::ArrayElement<Loc, Loc>>,
    ) -> expression::Expression<Loc, Loc> {
        let loc = loc.unwrap_or_else(Loc::none);
        expression::Expression::new(ExpressionInner::Array {
            loc,
            inner: Arc::new(expression::Array {
                elements: elements.into(),
                trailing_comma: false,
                comments,
            }),
        })
    }

    pub fn array_expression(
        expr: expression::Expression<Loc, Loc>,
    ) -> expression::ArrayElement<Loc, Loc> {
        expression::ArrayElement::Expression(expr)
    }

    pub fn array_hole(loc: Option<Loc>) -> expression::ArrayElement<Loc, Loc> {
        let loc = loc.unwrap_or_else(Loc::none);
        expression::ArrayElement::Hole(loc)
    }

    pub fn arg_list(
        loc: Option<Loc>,
        comments: Option<ast::Syntax<Loc, Arc<[ast::Comment<Loc>]>>>,
        arguments: Vec<expression::ExpressionOrSpread<Loc, Loc>>,
    ) -> expression::ArgList<Loc, Loc> {
        let loc = loc.unwrap_or_else(Loc::none);
        expression::ArgList {
            loc,
            arguments: arguments.into(),
            comments,
        }
    }

    pub fn call_node(
        targs: Option<expression::CallTypeArgs<Loc, Loc>>,
        args: Option<expression::ArgList<Loc, Loc>>,
        comments: Option<ast::Syntax<Loc, ()>>,
        callee: expression::Expression<Loc, Loc>,
    ) -> expression::Call<Loc, Loc> {
        let arguments = args.unwrap_or_else(|| arg_list(None, None, vec![]));
        expression::Call {
            callee,
            targs,
            arguments,
            comments,
        }
    }

    pub fn call(
        loc: Option<Loc>,
        args: Option<expression::ArgList<Loc, Loc>>,
        callee: expression::Expression<Loc, Loc>,
    ) -> expression::Expression<Loc, Loc> {
        let loc = loc.unwrap_or_else(Loc::none);
        expression::Expression::new(ExpressionInner::Call {
            loc,
            inner: Arc::new(call_node(None, args, None, callee)),
        })
    }

    pub fn optional_call(
        loc: Option<Loc>,
        optional: bool,
        args: Option<expression::ArgList<Loc, Loc>>,
        callee: expression::Expression<Loc, Loc>,
    ) -> expression::Expression<Loc, Loc> {
        let loc = loc.unwrap_or_else(Loc::none);
        expression::Expression::new(ExpressionInner::OptionalCall {
            loc: loc.dupe(),
            inner: Arc::new(expression::OptionalCall {
                call: call_node(None, args, None, callee),
                optional: if optional {
                    expression::OptionalCallKind::Optional
                } else {
                    expression::OptionalCallKind::NonOptional
                },
                filtered_out: loc,
            }),
        })
    }

    pub fn function_(
        loc: Option<Loc>,
        async_: Option<bool>,
        generator: Option<bool>,
        params: Option<function::Params<Loc, Loc>>,
        id: Option<ast::Identifier<Loc, Loc>>,
        body: Option<function::Body<Loc, Loc>>,
    ) -> expression::Expression<Loc, Loc> {
        let loc = loc.unwrap_or_else(Loc::none);
        let fn_ = super::functions::make(id, params, None, None, generator, None, async_, body);
        expression::Expression::new(ExpressionInner::Function {
            loc,
            inner: Arc::new(fn_),
        })
    }

    pub fn arrow_function(
        loc: Option<Loc>,
        async_: Option<bool>,
        params: Option<function::Params<Loc, Loc>>,
        body: Option<function::Body<Loc, Loc>>,
    ) -> expression::Expression<Loc, Loc> {
        let loc = loc.unwrap_or_else(Loc::none);
        let fn_ = super::functions::make(None, params, None, None, Some(false), None, async_, body);
        expression::Expression::new(ExpressionInner::ArrowFunction {
            loc,
            inner: Arc::new(fn_),
        })
    }

    pub fn class_(
        loc: Option<Loc>,
        super_: Option<expression::Expression<Loc, Loc>>,
        id: Option<ast::Identifier<Loc, Loc>>,
        elements: Vec<ast::class::BodyElement<Loc, Loc>>,
    ) -> expression::Expression<Loc, Loc> {
        let loc = loc.unwrap_or_else(Loc::none);
        expression::Expression::new(ExpressionInner::Class {
            loc,
            inner: Arc::new(super::classes::make(None, super_, None, None, id, elements)),
        })
    }

    pub fn assignment(
        loc: Option<Loc>,
        comments: Option<ast::Syntax<Loc, ()>>,
        left: pattern::Pattern<Loc, Loc>,
        operator: Option<expression::AssignmentOperator>,
        right: expression::Expression<Loc, Loc>,
    ) -> expression::Expression<Loc, Loc> {
        let loc = loc.unwrap_or_else(Loc::none);
        expression::Expression::new(ExpressionInner::Assignment {
            loc,
            inner: Arc::new(expression::Assignment {
                operator,
                left,
                right,
                comments,
            }),
        })
    }

    pub fn binary(
        loc: Option<Loc>,
        comments: Option<ast::Syntax<Loc, ()>>,
        op: expression::BinaryOperator,
        left: expression::Expression<Loc, Loc>,
        right: expression::Expression<Loc, Loc>,
    ) -> expression::Expression<Loc, Loc> {
        let loc = loc.unwrap_or_else(Loc::none);
        expression::Expression::new(ExpressionInner::Binary {
            loc,
            inner: Arc::new(expression::Binary {
                operator: op,
                left,
                right,
                comments,
            }),
        })
    }

    pub fn plus(
        left: expression::Expression<Loc, Loc>,
        right: expression::Expression<Loc, Loc>,
    ) -> expression::Expression<Loc, Loc> {
        binary(None, None, expression::BinaryOperator::Plus, left, right)
    }

    pub fn minus(
        left: expression::Expression<Loc, Loc>,
        right: expression::Expression<Loc, Loc>,
    ) -> expression::Expression<Loc, Loc> {
        binary(None, None, expression::BinaryOperator::Minus, left, right)
    }

    pub fn mult(
        left: expression::Expression<Loc, Loc>,
        right: expression::Expression<Loc, Loc>,
    ) -> expression::Expression<Loc, Loc> {
        binary(None, None, expression::BinaryOperator::Mult, left, right)
    }

    pub fn instanceof(
        left: expression::Expression<Loc, Loc>,
        right: expression::Expression<Loc, Loc>,
    ) -> expression::Expression<Loc, Loc> {
        binary(
            None,
            None,
            expression::BinaryOperator::Instanceof,
            left,
            right,
        )
    }

    pub fn in_(
        left: expression::Expression<Loc, Loc>,
        right: expression::Expression<Loc, Loc>,
    ) -> expression::Expression<Loc, Loc> {
        binary(None, None, expression::BinaryOperator::In, left, right)
    }

    pub fn equal(
        left: expression::Expression<Loc, Loc>,
        right: expression::Expression<Loc, Loc>,
    ) -> expression::Expression<Loc, Loc> {
        binary(None, None, expression::BinaryOperator::Equal, left, right)
    }

    pub fn conditional(
        loc: Option<Loc>,
        comments: Option<ast::Syntax<Loc, ()>>,
        test: expression::Expression<Loc, Loc>,
        consequent: expression::Expression<Loc, Loc>,
        alternate: expression::Expression<Loc, Loc>,
    ) -> expression::Expression<Loc, Loc> {
        let loc = loc.unwrap_or_else(Loc::none);
        expression::Expression::new(ExpressionInner::Conditional {
            loc,
            inner: Arc::new(expression::Conditional {
                test,
                consequent,
                alternate,
                comments,
            }),
        })
    }

    pub fn logical(
        loc: Option<Loc>,
        comments: Option<ast::Syntax<Loc, ()>>,
        op: expression::LogicalOperator,
        left: expression::Expression<Loc, Loc>,
        right: expression::Expression<Loc, Loc>,
    ) -> expression::Expression<Loc, Loc> {
        let loc = loc.unwrap_or_else(Loc::none);
        expression::Expression::new(ExpressionInner::Logical {
            loc,
            inner: Arc::new(expression::Logical {
                operator: op,
                left,
                right,
                comments,
            }),
        })
    }

    pub fn unary(
        loc: Option<Loc>,
        comments: Option<ast::Syntax<Loc, ()>>,
        op: expression::UnaryOperator,
        argument: expression::Expression<Loc, Loc>,
    ) -> expression::Expression<Loc, Loc> {
        let loc = loc.unwrap_or_else(Loc::none);
        expression::Expression::new(ExpressionInner::Unary {
            loc,
            inner: Arc::new(expression::Unary {
                operator: op,
                argument,
                comments,
            }),
        })
    }

    pub fn unary_plus(b: expression::Expression<Loc, Loc>) -> expression::Expression<Loc, Loc> {
        unary(None, None, expression::UnaryOperator::Plus, b)
    }

    pub fn unary_minus(b: expression::Expression<Loc, Loc>) -> expression::Expression<Loc, Loc> {
        unary(None, None, expression::UnaryOperator::Minus, b)
    }

    pub fn unary_not(b: expression::Expression<Loc, Loc>) -> expression::Expression<Loc, Loc> {
        unary(None, None, expression::UnaryOperator::Not, b)
    }

    pub fn update(
        loc: Option<Loc>,
        comments: Option<ast::Syntax<Loc, ()>>,
        op: expression::UpdateOperator,
        prefix: bool,
        argument: expression::Expression<Loc, Loc>,
    ) -> expression::Expression<Loc, Loc> {
        let loc = loc.unwrap_or_else(Loc::none);
        expression::Expression::new(ExpressionInner::Update {
            loc,
            inner: Arc::new(expression::Update {
                operator: op,
                prefix,
                argument,
                comments,
            }),
        })
    }

    pub fn increment(
        prefix: bool,
        argument: expression::Expression<Loc, Loc>,
    ) -> expression::Expression<Loc, Loc> {
        update(
            None,
            None,
            expression::UpdateOperator::Increment,
            prefix,
            argument,
        )
    }

    pub fn decrement(
        prefix: bool,
        argument: expression::Expression<Loc, Loc>,
    ) -> expression::Expression<Loc, Loc> {
        update(
            None,
            None,
            expression::UpdateOperator::Decrement,
            prefix,
            argument,
        )
    }

    pub fn object_property_key(loc: Option<Loc>, k: &str) -> expression::object::Key<Loc, Loc> {
        let loc = loc.unwrap_or_else(Loc::none);
        expression::object::Key::Identifier(ast_utils::ident_of_source(
            None,
            loc,
            FlowSmolStr::from(k),
        ))
    }

    pub fn object_property_key_string_literal(
        loc: Option<Loc>,
        k: &str,
    ) -> expression::object::Key<Loc, Loc> {
        let loc = loc.unwrap_or_else(Loc::none);
        expression::object::Key::StringLiteral((loc, super::string_literal(None, k)))
    }

    pub fn object_property_key_number_literal(
        loc: Option<Loc>,
        value: f64,
        raw: &str,
    ) -> expression::object::Key<Loc, Loc> {
        let loc = loc.unwrap_or_else(Loc::none);
        expression::object::Key::NumberLiteral((loc, super::number_literal(None, value, raw)))
    }

    pub fn object_property_computed_key(
        comments: Option<ast::Syntax<Loc, ()>>,
        loc: Option<Loc>,
        expr: expression::Expression<Loc, Loc>,
    ) -> expression::object::Key<Loc, Loc> {
        let loc = loc.unwrap_or_else(Loc::none);
        expression::object::Key::Computed(ast::ComputedKey {
            loc,
            expression: expr,
            comments,
        })
    }

    pub fn object_method(
        loc: Option<Loc>,
        body: Option<function::Body<Loc, Loc>>,
        params: Option<function::Params<Loc, Loc>>,
        generator: Option<bool>,
        async_: Option<bool>,
        key: expression::object::Key<Loc, Loc>,
    ) -> expression::object::Property<Loc, Loc> {
        let loc = loc.unwrap_or_else(Loc::none);
        let fn_ = super::functions::make(None, params, None, None, generator, None, async_, body);
        expression::object::Property::NormalProperty(expression::object::NormalProperty::Method {
            loc,
            key,
            value: (Loc::none(), fn_),
        })
    }

    pub fn object_property(
        shorthand: Option<bool>,
        loc: Option<Loc>,
        key: expression::object::Key<Loc, Loc>,
        value: expression::Expression<Loc, Loc>,
    ) -> expression::object::Property<Loc, Loc> {
        let shorthand = shorthand.unwrap_or(false);
        let loc = loc.unwrap_or_else(Loc::none);
        expression::object::Property::NormalProperty(expression::object::NormalProperty::Init {
            loc,
            key,
            value,
            shorthand,
        })
    }

    pub fn object_property_with_string_literal(
        shorthand: Option<bool>,
        loc: Option<Loc>,
        k: &str,
        v: expression::Expression<Loc, Loc>,
    ) -> expression::object::Property<Loc, Loc> {
        object_property(
            shorthand,
            loc.dupe(),
            object_property_key_string_literal(loc, k),
            v,
        )
    }

    pub fn object_(
        comments: Option<ast::Syntax<Loc, Arc<[ast::Comment<Loc>]>>>,
        loc: Option<Loc>,
        properties: Vec<expression::object::Property<Loc, Loc>>,
    ) -> expression::Expression<Loc, Loc> {
        let loc = loc.unwrap_or_else(Loc::none);
        expression::Expression::new(ExpressionInner::Object {
            loc,
            inner: Arc::new(expression::Object {
                properties: properties.into(),
                comments,
            }),
        })
    }

    pub mod members {
        use super::*;

        pub fn identifier(
            comments: Option<ast::Syntax<Loc, ()>>,
            property: ast::Identifier<Loc, Loc>,
            object: expression::Expression<Loc, Loc>,
        ) -> expression::Member<Loc, Loc> {
            expression::Member {
                object,
                property: expression::member::Property::PropertyIdentifier(property),
                comments,
            }
        }

        pub fn identifier_by_name(
            comments: Option<ast::Syntax<Loc, ()>>,
            name: &str,
            object: expression::Expression<Loc, Loc>,
        ) -> expression::Member<Loc, Loc> {
            identifier(comments, identifiers::identifier(None, name), object)
        }

        pub fn private_name(
            comments: Option<ast::Syntax<Loc, ()>>,
            property: ast::PrivateName<Loc>,
            object: expression::Expression<Loc, Loc>,
        ) -> expression::Member<Loc, Loc> {
            expression::Member {
                object,
                property: expression::member::Property::PropertyPrivateName(property),
                comments,
            }
        }

        pub fn expression(
            comments: Option<ast::Syntax<Loc, ()>>,
            property: expression::Expression<Loc, Loc>,
            object: expression::Expression<Loc, Loc>,
        ) -> expression::Member<Loc, Loc> {
            expression::Member {
                object,
                property: expression::member::Property::PropertyExpression(property),
                comments,
            }
        }

        pub fn expression_by_string(
            comments: Option<ast::Syntax<Loc, ()>>,
            str: &str,
            object: expression::Expression<Loc, Loc>,
        ) -> expression::Member<Loc, Loc> {
            let expr = super::super::string_literal_expression(None, None, str);
            expression(comments, expr, object)
        }
    }

    pub fn member(
        loc: Option<Loc>,
        expr: expression::Member<Loc, Loc>,
    ) -> expression::Expression<Loc, Loc> {
        let loc = loc.unwrap_or_else(Loc::none);
        expression::Expression::new(ExpressionInner::Member {
            loc,
            inner: Arc::new(expr),
        })
    }

    pub fn optional_member_expression(
        loc: Option<Loc>,
        optional: bool,
        expr: expression::Member<Loc, Loc>,
    ) -> expression::Expression<Loc, Loc> {
        let loc = loc.unwrap_or_else(Loc::none);
        expression::Expression::new(ExpressionInner::OptionalMember {
            loc: loc.dupe(),
            inner: Arc::new(expression::OptionalMember {
                member: expr,
                optional: if optional {
                    expression::OptionalMemberKind::Optional
                } else {
                    expression::OptionalMemberKind::NonOptional
                },
                filtered_out: loc,
            }),
        })
    }

    pub fn new_(
        loc: Option<Loc>,
        comments: Option<ast::Syntax<Loc, ()>>,
        targs: Option<expression::CallTypeArgs<Loc, Loc>>,
        args: Option<expression::ArgList<Loc, Loc>>,
        callee: expression::Expression<Loc, Loc>,
    ) -> expression::Expression<Loc, Loc> {
        let loc = loc.unwrap_or_else(Loc::none);
        expression::Expression::new(ExpressionInner::New {
            loc,
            inner: Arc::new(expression::New {
                callee,
                targs,
                arguments: args,
                comments,
            }),
        })
    }

    pub fn sequence(
        loc: Option<Loc>,
        comments: Option<ast::Syntax<Loc, ()>>,
        exprs: Vec<expression::Expression<Loc, Loc>>,
    ) -> expression::Expression<Loc, Loc> {
        let loc = loc.unwrap_or_else(Loc::none);
        expression::Expression::new(ExpressionInner::Sequence {
            loc,
            inner: Arc::new(expression::Sequence {
                expressions: exprs.into(),
                comments,
            }),
        })
    }

    pub fn expression_or_spread(
        expr: expression::Expression<Loc, Loc>,
    ) -> expression::ExpressionOrSpread<Loc, Loc> {
        expression::ExpressionOrSpread::Expression(expr)
    }

    pub fn spread(
        loc: Option<Loc>,
        comments: Option<ast::Syntax<Loc, ()>>,
        expr: expression::Expression<Loc, Loc>,
    ) -> expression::ExpressionOrSpread<Loc, Loc> {
        let loc = loc.unwrap_or_else(Loc::none);
        expression::ExpressionOrSpread::Spread(expression::SpreadElement {
            loc,
            argument: expr,
            comments,
        })
    }

    pub fn jsx_element(
        loc: Option<Loc>,
        elem: ast::jsx::Element<Loc, Loc>,
    ) -> expression::Expression<Loc, Loc> {
        let loc = loc.unwrap_or_else(Loc::none);
        expression::Expression::new(ExpressionInner::JSXElement {
            loc,
            inner: Arc::new(elem),
        })
    }

    pub fn true_() -> expression::Expression<Loc, Loc> {
        super::boolean_literal_expression(None, None, true)
    }

    pub fn false_() -> expression::Expression<Loc, Loc> {
        super::boolean_literal_expression(None, None, false)
    }

    pub fn parenthesis_hint() -> expression::Expression<Loc, Loc> {
        super::string_literal_expression(None, None, "_flowmin_paren_")
    }

    pub fn logical_and(
        l: expression::Expression<Loc, Loc>,
        r: expression::Expression<Loc, Loc>,
    ) -> expression::Expression<Loc, Loc> {
        logical(None, None, expression::LogicalOperator::And, l, r)
    }

    pub fn logical_or(
        l: expression::Expression<Loc, Loc>,
        r: expression::Expression<Loc, Loc>,
    ) -> expression::Expression<Loc, Loc> {
        logical(None, None, expression::LogicalOperator::Or, l, r)
    }

    pub fn typecast(
        loc: Option<Loc>,
        comments: Option<ast::Syntax<Loc, ()>>,
        expr: expression::Expression<Loc, Loc>,
        annotation: at::Type<Loc, Loc>,
    ) -> expression::Expression<Loc, Loc> {
        let loc = loc.unwrap_or_else(Loc::none);
        expression::Expression::new(ExpressionInner::TypeCast {
            loc,
            inner: Arc::new(expression::TypeCast {
                expression: expr,
                annot: super::types::annotation(annotation),
                comments,
            }),
        })
    }

    pub fn as_expression(
        loc: Option<Loc>,
        comments: Option<ast::Syntax<Loc, ()>>,
        expr: expression::Expression<Loc, Loc>,
        annotation: at::Type<Loc, Loc>,
    ) -> expression::Expression<Loc, Loc> {
        let loc = loc.unwrap_or_else(Loc::none);
        expression::Expression::new(ExpressionInner::AsExpression {
            loc,
            inner: Arc::new(expression::AsExpression {
                expression: expr,
                annot: super::types::annotation(annotation),
                comments,
            }),
        })
    }

    pub fn yield_(
        loc: Option<Loc>,
        comments: Option<ast::Syntax<Loc, ()>>,
        delegate: bool,
        expr: Option<expression::Expression<Loc, Loc>>,
    ) -> expression::Expression<Loc, Loc> {
        let loc = loc.unwrap_or_else(Loc::none);
        expression::Expression::new(ExpressionInner::Yield {
            loc: loc.dupe(),
            inner: Arc::new(expression::Yield {
                argument: expr,
                comments,
                delegate,
                result_out: loc,
            }),
        })
    }

    pub fn this(
        loc: Option<Loc>,
        comments: Option<ast::Syntax<Loc, ()>>,
    ) -> expression::Expression<Loc, Loc> {
        let loc = loc.unwrap_or_else(Loc::none);
        expression::Expression::new(ExpressionInner::This {
            loc,
            inner: Arc::new(expression::This { comments }),
        })
    }

    pub fn super_(
        loc: Option<Loc>,
        comments: Option<ast::Syntax<Loc, ()>>,
    ) -> expression::Expression<Loc, Loc> {
        let loc = loc.unwrap_or_else(Loc::none);
        expression::Expression::new(ExpressionInner::Super {
            loc,
            inner: Arc::new(expression::Super { comments }),
        })
    }

    pub mod literals {
        use super::*;

        pub fn string(
            loc: Option<Loc>,
            comments: Option<ast::Syntax<Loc, ()>>,
            value: &str,
        ) -> expression::Expression<Loc, Loc> {
            super::super::string_literal_expression(loc, comments, value)
        }

        pub fn number(
            loc: Option<Loc>,
            comments: Option<ast::Syntax<Loc, ()>>,
            value: f64,
            raw: &str,
        ) -> expression::Expression<Loc, Loc> {
            super::super::number_literal_expression(loc, comments, value, raw)
        }

        pub fn int(
            loc: Option<Loc>,
            comments: Option<ast::Syntax<Loc, ()>>,
            value: i32,
        ) -> expression::Expression<Loc, Loc> {
            super::super::int_literal_expression(loc, comments, value)
        }

        pub fn bool_(
            loc: Option<Loc>,
            comments: Option<ast::Syntax<Loc, ()>>,
            is_true: bool,
        ) -> expression::Expression<Loc, Loc> {
            super::super::boolean_literal_expression(loc, comments, is_true)
        }
    }
}

pub mod comments {
    use super::*;

    pub fn block(loc: Option<Loc>, on_newline: Option<bool>, text: &str) -> ast::Comment<Loc> {
        let loc = loc.unwrap_or_else(Loc::none);
        let on_newline = on_newline.unwrap_or(false);
        ast::Comment {
            loc,
            kind: ast::CommentKind::Block,
            text: Arc::from(text),
            on_newline,
        }
    }

    pub fn line(loc: Option<Loc>, on_newline: Option<bool>, text: &str) -> ast::Comment<Loc> {
        let loc = loc.unwrap_or_else(Loc::none);
        let on_newline = on_newline.unwrap_or(false);
        ast::Comment {
            loc,
            kind: ast::CommentKind::Line,
            text: Arc::from(text),
            on_newline,
        }
    }
}

pub fn mk_program(
    loc: Option<Loc>,
    interpreter: Option<(Loc, String)>,
    comments: Option<ast::Syntax<Loc, ()>>,
    all_comments: Option<Vec<ast::Comment<Loc>>>,
    stmts: Vec<statement::Statement<Loc, Loc>>,
) -> ast::Program<Loc, Loc> {
    let loc = loc.unwrap_or_else(Loc::none);
    let all_comments = all_comments.unwrap_or_default();
    ast::Program {
        loc,
        statements: stmts.into(),
        interpreter,
        comments,
        all_comments: all_comments.into(),
    }
}

pub fn test_ast_of_string<T>(
    parser: impl FnOnce(&mut flow_parser::ParserEnv) -> T,
    str: &str,
) -> T {
    test_ast_of_string_with_filename(parser, None, str)
}

pub fn test_ast_of_string_with_filename<T>(
    parser: impl FnOnce(&mut flow_parser::ParserEnv) -> T,
    filename: Option<&str>,
    str: &str,
) -> T {
    let parse_options = Some(flow_parser::PERMISSIVE_PARSE_OPTIONS);
    let source = filename.map(|f| {
        flow_parser::file_key::FileKey::new(flow_parser::file_key::FileKeyInner::SourceFile(
            f.to_string(),
        ))
    });
    let env = flow_parser::init_env::<()>(None, parse_options, source, Ok(str));
    let (ast, _) = flow_parser::do_parse(env, false, |env| {
        if env.is_d_ts() {
            env.with_ambient_context(true, parser)
        } else {
            parser(env)
        }
    })
    .unwrap();
    ast
}

pub fn test_expression_of_string(str: &str) -> expression::Expression<Loc, Loc> {
    test_ast_of_string(|env| flow_parser::parse_expression(env).ok().unwrap(), str)
}

pub fn test_statement_of_string(str: &str) -> statement::Statement<Loc, Loc> {
    test_statement_of_string_with_filename(None, str)
}

pub fn test_statement_of_string_with_filename(
    filename: Option<&str>,
    str: &str,
) -> statement::Statement<Loc, Loc> {
    let ast_list = test_ast_of_string_with_filename(
        |env| {
            flow_parser::parse_module_body_with_directives(env)
                .ok()
                .unwrap()
        },
        filename,
        str,
    );
    match ast_list.as_slice() {
        [ast] => ast.dupe(),
        _ => panic!("Multiple statements found"),
    }
}

pub fn test_program_of_string(str: &str) -> ast::Program<Loc, Loc> {
    let stmts = test_ast_of_string(
        |env| {
            flow_parser::parse_module_body_with_directives(env)
                .ok()
                .unwrap()
        },
        str,
    );
    ast::Program {
        loc: Loc::none(),
        statements: stmts.into(),
        interpreter: None,
        comments: None,
        all_comments: Vec::new().into(),
    }
}
