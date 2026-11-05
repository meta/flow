/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::ops::Deref;
use std::rc::Rc;
use std::sync::Arc;

use dupe::Dupe;
use flow_aloc::ALoc;
use flow_common::reason;
use flow_common::reason::Name;
use flow_common::reason::Reason;
use flow_common_ty::ty::FunParam;
use flow_common_ty::ty::ReturnT;
use flow_common_ty::ty::TupleElement;
use flow_common_ty::ty::Ty;
use flow_common_ty::ty_printer;
use flow_common_ty::ty_printer::PrinterOptions;
use flow_data_structure_wrapper::smol_str::FlowSmolStr;
use flow_parser::ast;
use flow_parser::ast_visitor;
use flow_parser::jsdoc::Jsdoc;
use flow_parser::jsdoc::param;
use flow_parser::loc::Loc;
use flow_server_env::server_prot::response::FuncDetailsResult;
use flow_server_env::server_prot::response::FuncParamResult;
use flow_services_autocomplete::find_documentation;
use flow_services_get_def::get_def_js::GetDefResult;
use flow_typing_context::Context;
use flow_typing_flow_js::flow_js;
use flow_typing_flow_js::flow_js::FlowJs;
use flow_typing_type::type_::CallAction;
use flow_typing_type::type_::CallTData;
use flow_typing_type::type_::DefTInner;
use flow_typing_type::type_::GenericTData;
use flow_typing_type::type_::MethodTData;
use flow_typing_type::type_::PolyTData;
use flow_typing_type::type_::PropRef;
use flow_typing_type::type_::Type;
use flow_typing_type::type_::TypeInner;
use flow_typing_type::type_::UseT;
use flow_typing_type::type_::UseTInner;
use flow_typing_type::type_::hint_unavailable;
use flow_typing_type::type_::inter_rep;
use flow_typing_type::type_::property;
use flow_typing_type::type_::type_collector::TypeCollector;
use flow_typing_type::type_::union_rep::UnionKind;
use flow_typing_type::type_::unknown_use;
use flow_typing_type::type_util;

fn parameter_name(is_opt: bool, name: &Option<FlowSmolStr>) -> String {
    let opt = if is_opt { "?" } else { "" };
    let name_str = match name {
        Some(n) => n.as_str(),
        None => "_",
    };
    format!("{}{}", name_str, opt)
}

fn string_of_ty<L: Dupe>(t: &Ty<L>) -> String {
    let opts = PrinterOptions {
        with_comments: false,
        ..Default::default()
    };
    ty_printer::string_of_t_single_line(t, &opts)
}

fn string_of_return_t<L: Dupe>(return_t: &ReturnT<L>) -> String {
    match return_t {
        ReturnT::ReturnType(t) => {
            let opts = PrinterOptions {
                with_comments: false,
                ..Default::default()
            };
            ty_printer::string_of_t_single_line(t, &opts)
        }
        ReturnT::TypeGuard(kind, x, t) => {
            let prefix = flow_common_ty::ty::type_guard_kind_prefix(*kind);
            match t {
                Some(t) => {
                    let opts = PrinterOptions {
                        with_comments: false,
                        ..Default::default()
                    };
                    let t_str = ty_printer::string_of_t_single_line(t, &opts);
                    format!("{}{} is {}", prefix, x, t_str)
                }
                None => format!("{}{}", prefix, x),
            }
        }
    }
}

fn documentation_of_param_infos(name: &str, param_infos: &param::T) -> String {
    fn string_of_path(path: &param::Path) -> String {
        match path {
            param::Path::Name => String::new(),
            param::Path::Element(p) => format!("{}[]", string_of_path(p)),
            param::Path::Member(p, mem) => format!("{}.{}", string_of_path(p), mem),
        }
    }
    fn string_of_optional(opt: &param::Optionality) -> String {
        match opt {
            param::Optionality::NotOptional => String::new(),
            param::Optionality::Optional => " (optional) ".to_string(),
            param::Optionality::OptionalWithDefault(default) => {
                format!(" (optional, defaults to {}) ", default)
            }
        }
    }
    fn string_of_description(desc: &Option<String>) -> String {
        match desc {
            None => String::new(),
            Some(description) => format!(" - {}", description),
        }
    }
    param_infos
        .iter()
        .map(|(path, info)| {
            format!(
                "{}{}{}{}",
                name,
                string_of_path(path),
                string_of_optional(&info.optional),
                string_of_description(&info.description)
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn func_details<L: Dupe>(
    jsdoc: &Option<Jsdoc>,
    params: &[(Option<FlowSmolStr>, Arc<Ty<L>>, FunParam)],
    rest_param: &Option<(Option<FlowSmolStr>, Arc<Ty<L>>)>,
    return_t: &ReturnT<L>,
) -> FuncDetailsResult {
    let documentation_of_param = |name_opt: &Option<FlowSmolStr>| -> Option<String> {
        let name = name_opt.as_ref()?;
        let jsdoc = jsdoc.as_ref()?;
        let param_infos = jsdoc
            .params()
            .0
            .iter()
            .find(|(n, _)| n == name.as_str())
            .map(|(_, infos)| infos)?;
        Some(documentation_of_param_infos(name.as_str(), param_infos))
    };
    let mut param_tys: Vec<FuncParamResult> = params
        .iter()
        .map(|(n, t, fp)| {
            let param_name = parameter_name(fp.prm_optional, n);
            let param_ty = string_of_ty(t);
            let param_documentation = documentation_of_param(n);
            FuncParamResult {
                param_name,
                param_ty,
                param_documentation,
            }
        })
        .collect();
    match rest_param {
        None => {}
        Some((rest_param_name, t)) => {
            let param_documentation = documentation_of_param(rest_param_name);
            let els_result: Option<(i32, Vec<(String, &Ty<L>)>)> = match t.as_ref() {
                Ty::Tup { elements, .. } => {
                    elements
                        .iter()
                        .try_fold((0i32, Vec::new()), |(i, mut els), el| match el {
                            TupleElement::TupleElement {
                                name,
                                t: el_t,
                                polarity: _,
                                optional: _,
                            } => {
                                let param_name = match name {
                                    Some(name) => name.to_string(),
                                    None => {
                                        let rest_name = rest_param_name
                                            .as_ref()
                                            .map(|n| n.as_str())
                                            .unwrap_or("arg");
                                        format!("{}[{}]", rest_name, i)
                                    }
                                };
                                els.push((param_name, el_t.as_ref()));
                                Some((i + 1, els))
                            }
                            TupleElement::TupleSpread { .. } => None,
                        })
                }
                _ => None,
            };
            let rest = match els_result {
                Some((_, els)) => els
                    .into_iter()
                    .map(|(param_name, t)| FuncParamResult {
                        param_name,
                        param_ty: string_of_ty(t),
                        param_documentation: param_documentation.clone(),
                    })
                    .collect::<Vec<_>>(),
                None => {
                    let param_name = format!("...{}", parameter_name(false, rest_param_name));
                    let param_ty = string_of_ty(t);
                    vec![FuncParamResult {
                        param_name,
                        param_ty,
                        param_documentation: param_documentation.clone(),
                    }]
                }
            };
            param_tys.extend(rest);
        }
    }
    let return_ty = string_of_return_t(return_t);
    let func_documentation = jsdoc
        .as_ref()
        .and_then(find_documentation::documentation_of_jsdoc);
    FuncDetailsResult::SigHelpFunc {
        param_tys,
        return_ty,
        func_documentation,
    }
}

/// Given a location within a function call or `new` expression,
/// returns the type of the function or constructor being called.
pub mod callee_finder {
    use flow_parser::ast::expression::ExpressionOrSpread;
    use flow_parser::ast_visitor::AstVisitor;

    use super::*;

    pub enum T {
        FunCallData {
            type_: Type,
            active_parameter: i32,
            loc: ALoc,
        },
        JsxAttrData {
            props: Type,
            name: String,
            key_loc: ALoc,
        },
    }

    /// Ty_normalizer_flow will attempt to recover an alias name for this type. Given that
    /// in collect_functions we try to match against the structure of the type, we
    /// would rather bypass the alias on the toplevel of the type. It is still
    /// desirable that deeper within the type aliases are maintained. Note that
    /// alternatively we could updated the normalizer to perform a one-off expansion
    /// of the toplevel alias, but that would be more complex that fixing this here.
    pub fn fix_reason_of_t(t: &Type) -> Type {
        type_util::mod_reason_of_t(
            &|reason| {
                reason.update_desc(|desc| {
                    desc.map(flow_common::reason::VirtualReasonDesc::invalidate_rtype_alias)
                })
            },
            t,
        )
    }

    pub fn simplify_fun_t(
        cx: &mut Context,
        func_t: &Type,
    ) -> Result<Type, flow_utils_concurrency::job_error::JobError> {
        let reason = type_util::reason_of_t(func_t).dupe();
        let collector = TypeCollector::create();
        let u = UseT::new(UseTInner::CallT(Box::new(CallTData {
            use_op: unknown_use(),
            reason: reason.dupe(),
            call_action: Box::new(CallAction::ConcretizeCallee(collector.dupe())),
            return_hint: hint_unavailable(),
        })));
        flow_js::flow_non_speculating(cx, (func_t, &u))?;
        Ok(type_util::union_of_ts(
            reason,
            collector.collect_to_vec(),
            Some(UnionKind::ResolvedKind),
        ))
    }

    pub fn get_func(
        cx: &mut Context,
        reason: &Reason,
        t: &Type,
    ) -> Result<Vec<Type>, flow_utils_concurrency::job_error::JobError> {
        let t = simplify_fun_t(cx, t)?;
        match FlowJs::possible_concrete_types_for_inspection(cx, reason, &t) {
            Ok(ts) => match ts.as_slice() {
                [] => Ok(vec![]),
                // NOTE since we're not merging unions of signatures to a single one,
                // keep the first member. This shouldn't be common.
                [t, ..] => get_func_no_union(cx, type_util::reason_of_t(t), t),
            },
            Err(_) => Ok(vec![]),
        }
    }

    pub fn get_func_no_union(
        cx: &mut Context,
        reason: &Reason,
        t: &Type,
    ) -> Result<Vec<Type>, flow_utils_concurrency::job_error::JobError> {
        match t.deref() {
            TypeInner::GenericT(box GenericTData { bound, .. }) => get_func(cx, reason, bound),
            TypeInner::DefT(_, def) if matches!(&**def, DefTInner::FunT(..)) => {
                Ok(vec![fix_reason_of_t(t)])
            }
            TypeInner::DefT(_, def)
                if let DefTInner::PolyT(box PolyTData { t_out, .. }) = &**def
                    && matches!(t_out.deref(), TypeInner::DefT(_, d) if matches!(&**d, DefTInner::FunT(..))) =>
            {
                Ok(vec![fix_reason_of_t(t)])
            }
            TypeInner::IntersectionT(_, rep) => rep
                .members_iter()
                .map(|t| get_func(cx, reason, t))
                .collect::<Result<Vec<Vec<Type>>, _>>()
                .map(|vs| vs.into_iter().flatten().collect()),
            _ => Ok(vec![]),
        }
    }

    pub fn get_prop_of_obj(
        cx: &mut Context,
        name: &Name,
        reason: &Reason,
        t: &Type,
    ) -> Option<(Type, bool)> {
        let ts: Vec<(Type, bool)> =
            match FlowJs::possible_concrete_types_for_inspection(cx, reason, t) {
                Ok(concrete_ts) => concrete_ts
                    .iter()
                    .filter_map(|t| {
                        get_prop_of_obj_no_union(cx, name, type_util::reason_of_t(t), t)
                    })
                    .collect(),
                Err(_) => vec![],
            };
        match ts.as_slice() {
            [] => None,
            [single] => Some(single.dupe()),
            _ => {
                let opt = ts.iter().any(|(_, o)| *o);
                let ts_types: Vec<Type> = ts.into_iter().map(|(t, _)| t).collect();
                Some((type_util::union_of_ts(reason.dupe(), ts_types, None), opt))
            }
        }
    }

    pub fn get_prop_of_obj_no_union(
        cx: &mut Context,
        name: &Name,
        reason: &Reason,
        t: &Type,
    ) -> Option<(Type, bool)> {
        match t.deref() {
            TypeInner::GenericT(box GenericTData { bound, .. }) => {
                get_prop_of_obj(cx, name, reason, bound)
            }
            TypeInner::DefT(_, def) if let DefTInner::ObjT(obj) = &**def => {
                match cx.get_prop(obj.props_tmap.dupe(), name) {
                    None => None,
                    Some(prop) => match property::read_t(&prop) {
                        Some(read_t) => match read_t.deref() {
                            TypeInner::OptionalT { type_, .. } => Some((type_.dupe(), true)),
                            _ => Some((read_t, false)),
                        },
                        None => None,
                    },
                }
            }
            TypeInner::IntersectionT(_, rep) => {
                let (ts, opt) = rep
                    .members_iter()
                    .fold(
                        (Vec::new(), false),
                        |(mut ts, opt), t| match get_prop_of_obj(cx, name, reason, t) {
                            Some((t, opt2)) => {
                                ts.push(t);
                                (ts, opt || opt2)
                            }
                            None => (ts, opt),
                        },
                    );
                match ts.as_slice() {
                    [] => None,
                    [t] => Some((t.dupe(), opt)),
                    [t0, t1, ..] => {
                        let r = reason::locationless_reason(None);
                        let rest: Rc<[Type]> = ts[2..].iter().map(|t| t.dupe()).collect();
                        Some((
                            Type::new(TypeInner::IntersectionT(
                                r,
                                inter_rep::make(t0.dupe(), t1.dupe(), rest),
                            )),
                            opt,
                        ))
                    }
                }
            }
            _ => None,
        }
    }

    pub fn get_prop_of_obj_toplevel(
        cx: &mut Context,
        name: &Name,
        reason: &Reason,
        t: &Type,
    ) -> Vec<(Type, bool)> {
        match FlowJs::possible_concrete_types_for_inspection(cx, reason, t) {
            Ok(concrete_ts) => concrete_ts
                .iter()
                .filter_map(|t| get_prop_of_obj_no_union(cx, name, type_util::reason_of_t(t), t))
                .collect(),
            Err(_) => vec![],
        }
    }

    /// find the argument whose Loc contains `loc`, or the first one past it.
    /// the latter covers the `f(x,| y)` case, where your cursor is after the
    /// comma and before the space; this is not contained in `x` nor `y`, but should
    /// select `y`.
    pub fn find_argument(
        loc_of_aloc: &dyn Fn(&ALoc) -> Loc,
        cursor: &Loc,
        arguments: &[ExpressionOrSpread<ALoc, (ALoc, Type)>],
        i: i32,
    ) -> i32 {
        match arguments {
            [] => i,
            [first, rest @ ..] => {
                let arg_aloc = match first {
                    ExpressionOrSpread::Expression(expr) => &expr.loc().0,
                    ExpressionOrSpread::Spread(spread) => &spread.loc,
                };
                // if the cursor is within this arg, we obviously found it. if it's before the arg,
                // but hasn't been found by earlier args, then we must be in the whitespace before
                // the current arg, so we found it.
                let arg_loc = loc_of_aloc(arg_aloc);
                if reason::in_range(cursor, &arg_loc) || cursor < &arg_loc {
                    i
                } else {
                    find_argument(loc_of_aloc, cursor, rest, i + 1)
                }
            }
        }
    }

    enum Found {
        Done(Option<T>),
        // Cancellation/timeout escaping out of an inline `flow_*_non_speculating`
        // call inside `get_callee_type`. Surfaced via the visitor's short-circuit
        // mechanism so it can be reported at the LSP boundary.
        Job(flow_utils_concurrency::job_error::JobError),
    }

    struct Finder<'a, 'cx> {
        loc_of_aloc: &'a dyn Fn(&ALoc) -> Loc,
        cx: &'a mut Context<'cx>,
        cursor: Loc,
    }

    impl Finder<'_, '_> {
        fn covers_target(&self, loc: &ALoc) -> bool {
            reason::in_range(&self.cursor, &(self.loc_of_aloc)(loc))
        }

        // only recurse if this loc covers the target.
        fn short_circuit<R>(
            &mut self,
            loc: &ALoc,
            default: R,
            f: impl FnOnce(&mut Self) -> R,
        ) -> R {
            if self.covers_target(loc) {
                f(self)
            } else {
                default
            }
        }

        fn find(
            &mut self,
            recurse: impl FnOnce(&mut Self) -> Result<(), Found>,
            get_callee_type: impl FnOnce(
                &mut Context<'_>,
            )
                -> Result<Type, flow_utils_concurrency::job_error::JobError>,
            args_loc: &ALoc,
            args: &[ast::expression::ExpressionOrSpread<ALoc, (ALoc, Type)>],
            callee_loc: &ALoc,
        ) -> Result<(), Found> {
            let args_loc_loc = (self.loc_of_aloc)(args_loc);
            // exclude the parens
            let inside_loc = ALoc::of_loc(Loc {
                source: args_loc_loc.source.dupe(),
                start: flow_parser::loc::Position {
                    line: args_loc_loc.start.line,
                    column: args_loc_loc.start.column + 1,
                },
                end: args_loc_loc.end,
            });
            if self.covers_target(&inside_loc) {
                // recurse to see if we find a nested call that's narrower. it will raise if so,
                // so after this line we know we found the right call.
                recurse(self)?;
                let active_parameter = find_argument(self.loc_of_aloc, &self.cursor, args, 0);
                let type_ = get_callee_type(self.cx).map_err(Found::Job)?;
                Err(Found::Done(Some(T::FunCallData {
                    type_,
                    active_parameter,
                    loc: callee_loc.dupe(),
                })))
            } else {
                recurse(self)
            }
        }
    }

    impl<'ast> ast_visitor::AstVisitor<'ast, ALoc, (ALoc, Type), &'ast ALoc, Found> for Finder<'_, '_> {
        fn normalize_loc(loc: &'ast ALoc) -> &'ast ALoc {
            loc
        }

        fn normalize_type(type_: &'ast (ALoc, Type)) -> &'ast ALoc {
            &type_.0
        }

        fn statement(
            &mut self,
            stmt: &'ast ast::statement::Statement<ALoc, (ALoc, Type)>,
        ) -> Result<(), Found> {
            let loc = stmt.loc();
            self.short_circuit(loc, Ok(()), |this| {
                ast_visitor::statement_default(this, stmt)
            })
        }

        fn expression(
            &mut self,
            expr: &'ast ast::expression::Expression<ALoc, (ALoc, Type)>,
        ) -> Result<(), Found> {
            let loc = &expr.loc().0;
            self.short_circuit(loc, Ok(()), |this| {
                ast_visitor::expression_default(this, expr)
            })
        }

        fn call(
            &mut self,
            loc: &'ast (ALoc, Type),
            expr: &'ast ast::expression::Call<ALoc, (ALoc, Type)>,
        ) -> Result<(), Found> {
            let callee_loc = &expr.callee.loc().0;
            let callee_t = &expr.callee.loc().1;
            let t = self
                .cx
                .get_signature_help_callee(callee_loc)
                .unwrap_or_else(|| callee_t.dupe());
            let callee_loc = match expr.callee.deref() {
                ast::expression::ExpressionInner::Member { inner, .. } => match &inner.property {
                    ast::expression::member::Property::PropertyIdentifier(id) => &id.loc.0,
                    ast::expression::member::Property::PropertyPrivateName(pn) => &pn.loc,
                    ast::expression::member::Property::PropertyExpression(e) => &e.loc().0,
                },
                _ => callee_loc,
            };
            let arguments = &expr.arguments;
            self.find(
                |this| ast_visitor::call_default(this, loc, expr),
                |_cx| Ok(t),
                &arguments.loc,
                &arguments.arguments,
                callee_loc,
            )
        }

        fn new(
            &mut self,
            loc: &'ast (ALoc, Type),
            expr: &'ast ast::expression::New<ALoc, (ALoc, Type)>,
        ) -> Result<(), Found> {
            let callee_loc = &expr.callee.loc().0;
            let class_t = &expr.callee.loc().1;
            match &expr.arguments {
                Some(arguments) => {
                    let class_t = class_t.dupe();
                    let callee_loc_owned = callee_loc.dupe();
                    let get_callee_type = move |cx: &mut Context<'_>| -> Result<
                        Type,
                        flow_utils_concurrency::job_error::JobError,
                    > {
                        let desc = type_util::desc_of_t(&class_t).cloned();
                        let ctor_reason = reason::mk_reason(desc, callee_loc_owned);
                        let instance = flow_typing_tvar::mk_where::<
                            flow_utils_concurrency::job_error::JobError,
                        >(
                            cx,
                            ctor_reason.dupe(),
                            |cx, instance| {
                                let class_def = Type::new(TypeInner::DefT(
                                    ctor_reason.dupe(),
                                    flow_typing_type::type_::DefT::new(DefTInner::ClassT(
                                        instance.dupe(),
                                    )),
                                ));
                                flow_js::flow_t_non_speculating(cx, (&class_t, &class_def))?;
                                Ok(())
                            },
                        )?;
                        let propref = PropRef::Constructor(ctor_reason.dupe());
                        let collector = TypeCollector::create();
                        let use_t = UseT::new(UseTInner::MethodT(Box::new(MethodTData {
                            use_op: unknown_use(),
                            reason: ctor_reason.dupe(),
                            prop_reason: ctor_reason.dupe(),
                            propref: Box::new(propref),
                            method_action: Box::new(
                                flow_typing_type::type_::MethodAction::NoMethodAction(
                                    collector.dupe(),
                                ),
                            ),
                        })));
                        flow_js::flow_non_speculating(cx, (&instance, &use_t))?;
                        Ok(type_util::union_of_ts(
                            ctor_reason,
                            collector.collect_to_vec(),
                            Some(UnionKind::ResolvedKind),
                        ))
                    };
                    self.find(
                        |this| ast_visitor::new_default(this, loc, expr),
                        get_callee_type,
                        &arguments.loc,
                        &arguments.arguments,
                        callee_loc,
                    )
                }
                None => ast_visitor::new_default(self, loc, expr),
            }
        }

        fn class_body(
            &mut self,
            cls_body: &'ast ast::class::Body<ALoc, (ALoc, Type)>,
        ) -> Result<(), Found> {
            ast_visitor::class_body_default(self, cls_body)?;
            if self.covers_target(&cls_body.loc) {
                return Err(Found::Done(None));
            }
            Ok(())
        }

        fn function_body_any(
            &mut self,
            body: &'ast ast::function::Body<ALoc, (ALoc, Type)>,
        ) -> Result<(), Found> {
            ast_visitor::function_body_any_default(self, body)?;
            match body {
                ast::function::Body::BodyBlock(block) if self.covers_target(&block.0) => {
                    Err(Found::Done(None))
                }
                _ => Ok(()),
            }
        }

        fn jsx_element(
            &mut self,
            loc: &'ast (ALoc, Type),
            elt: &'ast ast::jsx::Element<ALoc, (ALoc, Type)>,
        ) -> Result<(), Found> {
            ast_visitor::jsx_element_default(self, loc, elt)?;
            let elt_name = &elt.opening_element.name;
            let attributes = &elt.opening_element.attributes;
            for attr in attributes.iter() {
                match attr {
                    ast::jsx::OpeningAttribute::SpreadAttribute(_) => {}
                    ast::jsx::OpeningAttribute::Attribute(attr) => {
                        let attr_name = &attr.name;
                        if let Some(value) = &attr.value {
                            let attr_loc = match value {
                                ast::jsx::attribute::Value::StringLiteral(sl) => Some(&sl.0.0),
                                ast::jsx::attribute::Value::ExpressionContainer((annot, _)) => {
                                    Some(&annot.0)
                                }
                            };
                            if let Some(attr_loc) = attr_loc {
                                if self.covers_target(attr_loc) {
                                    match (elt_name, attr_name) {
                                        (
                                            ast::jsx::Name::Identifier(_),
                                            ast::jsx::attribute::Name::Identifier(attr_id),
                                        ) => {
                                            let key_loc = &attr_id.loc.0;
                                            let name = attr_id.name.to_string();
                                            return Err(Found::Done(Some(T::JsxAttrData {
                                                props: elt.opening_element.props.1.dupe(),
                                                name,
                                                key_loc: key_loc.dupe(),
                                            })));
                                        }
                                        _ => return Err(Found::Done(None)),
                                    }
                                }
                            }
                        }
                    }
                }
            }
            Ok(())
        }
    }

    pub fn find_opt(
        loc_of_aloc: &dyn Fn(&ALoc) -> Loc,
        cx: &mut Context,
        typed_ast: &ast::Program<ALoc, (ALoc, Type)>,
        loc: Loc,
    ) -> Result<Option<T>, flow_utils_concurrency::job_error::JobError> {
        let mut finder = Finder {
            loc_of_aloc,
            cx,
            cursor: loc,
        };
        match finder.program(typed_ast) {
            Err(Found::Done(Some(data))) => Ok(Some(data)),
            Err(Found::Done(None)) => Ok(None),
            Err(Found::Job(err)) => Err(err),
            Ok(()) => Ok(None),
        }
    }
}

pub fn find_signatures<'a>(
    loc_of_aloc: &dyn Fn(&ALoc) -> Loc,
    get_ast_from_heap: &dyn Fn(&flow_parser::file_key::FileKey) -> Option<ast::Program<Loc, Loc>>,
    cx: &mut Context<'a>,
    file_sig: Arc<flow_parser_utils::file_sig::FileSig>,
    ast: &ast::Program<Loc, Loc>,
    typed_ast: &ast::Program<ALoc, (ALoc, Type)>,
    loc: Loc,
) -> Result<
    Result<Option<(Vec<FuncDetailsResult>, i32)>, flow_typing_ty_normalizer::normalizer::Error>,
    flow_utils_concurrency::job_error::JobError,
> {
    match callee_finder::find_opt(loc_of_aloc, cx, typed_ast, loc)? {
        Some(callee_finder::T::FunCallData {
            type_: t,
            active_parameter,
            loc: callee_loc,
        }) => {
            let ts = callee_finder::get_func(cx, type_util::reason_of_t(&t), &t)?;
            let norm_options = flow_typing_ty_normalizer::env::Options::default();
            let genv = flow_typing::ty_normalizer_flow::mk_genv(
                norm_options,
                Some(typed_ast),
                file_sig.dupe(),
            );

            let func_details_of_type = |cx: &mut Context<'a>,
                                        jsdoc: &Option<Jsdoc>,
                                        fn_t: &Type|
             -> Option<FuncDetailsResult> {
                let ty = flow_typing::ty_normalizer_flow::from_type(cx, &genv, fn_t);
                match ty {
                    Ok(flow_common_ty::ty::Elt::Type(ty)) => match ty.as_ref() {
                        Ty::Fun(fun) => Some(func_details(
                            jsdoc,
                            &fun.fun_params,
                            &fun.fun_rest_param,
                            &fun.fun_return,
                        )),
                        _ => None,
                    },
                    _ => None,
                }
            };
            let funs = match ts.as_slice() {
                [] => vec![],
                [t] => {
                    // non-overloaded case: get-def returns reasonable JSDoc results
                    let jsdoc = match flow_services_get_def::get_def_js::get_def(
                        loc_of_aloc,
                        cx,
                        &file_sig,
                        None,
                        ast,
                        flow_typing_utils::typed_ast_utils::AvailableAst::TypedAst(typed_ast),
                        &flow_services_get_def::get_def_types::Purpose::JSDoc,
                        &loc_of_aloc(&callee_loc),
                    )? {
                        GetDefResult::Def(locs, _) | GetDefResult::Partial(locs, _, _)
                            if locs.len() == 1 =>
                        {
                            let getdef_loc = locs.into_iter().next().unwrap();
                            find_documentation::jsdoc_of_getdef_loc(
                                ast,
                                get_ast_from_heap,
                                getdef_loc,
                            )
                        }
                        _ => None,
                    };
                    func_details_of_type(cx, &jsdoc, t).into_iter().collect()
                }
                _ => {
                    // overloaded case: get-def does not distinguish between overloads, get the
                    // JSDoc from the def_loc of the specific overload instead.
                    ts.iter()
                        .filter_map(|fn_t| {
                            let jsdoc = {
                                let loc = loc_of_aloc(type_util::def_loc_of_t(fn_t));
                                find_documentation::jsdoc_of_getdef_loc(ast, get_ast_from_heap, loc)
                            };
                            func_details_of_type(cx, &jsdoc, fn_t)
                        })
                        .collect()
                }
            };
            Ok(Ok(Some((funs, active_parameter))))
        }
        Some(callee_finder::T::JsxAttrData {
            props,
            name,
            key_loc,
        }) => {
            let ts = if name == "key" {
                // 'key' is special-cased
                let reason_key = reason::mk_reason(None, key_loc);
                let key_t =
                    flow_js::get_builtin_type_non_speculating(cx, &reason_key, None, "React$Key")?;
                let t = type_util::maybe(key_t);
                vec![(t, true)]
            } else {
                callee_finder::get_prop_of_obj_toplevel(
                    cx,
                    &Name::new(&name),
                    type_util::reason_of_t(&props),
                    &props,
                )
            };
            let norm_options = flow_typing_ty_normalizer::env::Options::default();
            let genv = flow_typing::ty_normalizer_flow::mk_genv(
                norm_options,
                Some(typed_ast),
                file_sig.dupe(),
            );
            let tys = ts
                .iter()
                .filter_map(|(t, optional)| {
                    match flow_typing::ty_normalizer_flow::from_type(cx, &genv, t) {
                        Ok(flow_common_ty::ty::Elt::Type(ty)) => {
                            let ty_str = string_of_ty(ty.as_ref());
                            let loc = loc_of_aloc(type_util::loc_of_t(t));
                            let jsdoc = find_documentation::jsdoc_of_getdef_loc(
                                ast,
                                get_ast_from_heap,
                                loc,
                            );
                            let documentation =
                                jsdoc.as_ref().and_then(|j| j.description().clone());
                            Some(FuncDetailsResult::SigHelpJsxAttr {
                                documentation,
                                name: name.clone(),
                                ty: ty_str,
                                optional: *optional,
                            })
                        }
                        _ => None,
                    }
                })
                .collect();
            Ok(Ok(Some((tys, 0))))
        }
        None => Ok(Ok(None)),
    }
}
