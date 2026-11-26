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
use flow_aloc::ALocFuzzy;
use flow_common::reason::Name;
use flow_common::reason::Reason;
use flow_common_ty::ty::ALocElt;
use flow_common_ty::ty::ALocTy;
use flow_common_ty::ty_printer;
use flow_common_ty::ty_symbol::ImportedIdent;
use flow_parser::ast;
use flow_parser_utils::file_sig::FileSig;
use flow_typing_context::Context;
use flow_typing_flow_common::flow_js_utils;
use flow_typing_flow_js::flow_js;
use flow_typing_ty_normalizer::env::Env;
use flow_typing_ty_normalizer::env::Genv;
pub use flow_typing_ty_normalizer::env::ImportedNames;
use flow_typing_ty_normalizer::env::Options;
use flow_typing_ty_normalizer::normalizer::Error;
use flow_typing_ty_normalizer::normalizer::Normalizer;
use flow_typing_ty_normalizer::normalizer::NormalizerInput;
use flow_typing_ty_normalizer::normalizer::State;
use flow_typing_ty_normalizer::normalizer::lookahead;
use flow_typing_type::type_::ConcretizationKind;
use flow_typing_type::type_::ConcretizeTData;
use flow_typing_type::type_::DefTInner;
use flow_typing_type::type_::Destructor;
use flow_typing_type::type_::ModuleType;
use flow_typing_type::type_::PolyTData;
use flow_typing_type::type_::Type;
use flow_typing_type::type_::TypeDestructorT;
use flow_typing_type::type_::TypeDestructorTInner;
use flow_typing_type::type_::TypeInner;
use flow_typing_type::type_::TypeStrictnessKind;
use flow_typing_type::type_::UseT;
use flow_typing_type::type_::UseTInner;
use flow_typing_type::type_::concretize_seen::ConcretizeSeen;
use flow_typing_type::type_::eval;
use flow_typing_type::type_::type_collector::TypeCollector;
use flow_typing_type::type_::unknown_use;
use flow_typing_type::type_util;

use crate::ty_normalizer_imports;

struct FlowInput;

impl NormalizerInput for FlowInput {
    fn eval<'cx>(
        env: &mut Env<'_, 'cx>,
        state: &mut State,
        should_eval: bool,
        cont: impl FnOnce(&mut Env<'_, 'cx>, &mut State, &Type) -> Result<ALocTy, Error>,
        default: impl FnOnce(&mut Env<'_, 'cx>, &mut State, &Type) -> Result<ALocTy, Error>,
        non_eval: impl FnOnce(
            &mut Env<'_, 'cx>,
            &mut State,
            &Type,
            &Destructor,
        ) -> Result<ALocTy, Error>,
        x: (&Type, &TypeDestructorT, &eval::Id),
    ) -> Result<ALocTy, Error> {
        let (t, defer_use_t, id) = x;
        let TypeDestructorTInner(use_op, reason, d) = defer_use_t.deref();
        if should_eval {
            let tout = flow_js::mk_type_destructor(env.cx, use_op.dupe(), reason, t, d, id.dupe());
            match tout {
                Ok(tout) => match lookahead::peek(env.cx, &tout) {
                    lookahead::Lookahead::LowerBounds(ref bounds) if bounds.len() == 1 => {
                        cont(env, state, &bounds[0])
                    }
                    _ => default(env, state, &tout),
                },
                Err(_) => default(env, state, t),
            }
        } else {
            non_eval(env, state, t, d)
        }
    }

    fn keys<'cx, A>(
        env: &mut Env<'_, 'cx>,
        state: &mut State,
        should_evaluate: bool,
        cont: impl FnOnce(&mut Env<'_, 'cx>, &mut State, Type) -> Result<A, Error>,
        default: impl FnOnce(&mut Env<'_, 'cx>, &mut State) -> Result<A, Error>,
        reason: Reason,
        t: Type,
    ) -> Result<A, Error> {
        if should_evaluate {
            let collector = TypeCollector::create();
            let tout = UseT::new(UseTInner::ConcretizeT(Box::new(ConcretizeTData {
                reason: reason.dupe(),
                kind: ConcretizationKind::ConcretizeForLowerBounds,
                seen: ConcretizeSeen::new(),
                collector: collector.dupe(),
            })));
            let use_t = UseT::new(UseTInner::GetKeysT {
                reason,
                t_out: Box::new(tout),
                include_symbols: true,
            });
            flow_js::flow_non_speculating(env.cx, (&t, &use_t))?;
            match lookahead::peek_types(env.cx, &collector.collect_to_vec()) {
                lookahead::Lookahead::LowerBounds(ref bounds) if bounds.len() == 1 => {
                    let t = type_util::mod_reason_of_t(&|r| r.replace_desc(None), &bounds[0]);
                    cont(env, state, t)
                }
                _ => default(env, state),
            }
        } else {
            default(env, state)
        }
    }

    fn typeapp_regular<'cx, A>(
        env: &mut Env<'_, 'cx>,
        state: &mut State,
        cont: &mut dyn FnMut(&mut Env<'_, 'cx>, &mut State, &Type) -> Result<A, Error>,
        _type_: &mut dyn FnMut(&mut Env<'_, 'cx>, &mut State, &Type) -> Result<A, Error>,
        _app: impl FnOnce(Result<A, Error>, Vec<Result<A, Error>>) -> Result<A, Error>,
        from_value: bool,
        reason: Reason,
        t: Type,
        targs: &[Type],
        strictness_kind: TypeStrictnessKind,
    ) -> Result<A, Error> {
        let t = flow_js::mk_typeapp_instance_annot_non_speculating(
            env.cx,
            unknown_use(),
            &reason,
            &reason,
            from_value,
            &t,
            Rc::from(targs),
            strictness_kind,
        )?;
        cont(env, state, &t)
    }

    fn builtin_type<'cx, A>(
        env: &mut Env<'_, 'cx>,
        state: &mut State,
        cont: &mut dyn FnMut(&mut Env<'_, 'cx>, &mut State, Type) -> Result<A, Error>,
        reason: Reason,
        name: &str,
    ) -> Result<A, Error> {
        let t = flow_js::get_builtin_type_non_speculating(env.cx, &reason, None, name)?;
        cont(env, state, t)
    }

    fn builtin_typeapp<'cx, A>(
        env: &mut Env<'_, 'cx>,
        state: &mut State,
        cont: &mut dyn FnMut(&mut Env<'_, 'cx>, &mut State, Type) -> Result<A, Error>,
        _type_: &mut dyn FnMut(&mut Env<'_, 'cx>, &mut State, Type) -> Result<A, Error>,
        _app: impl FnOnce(Result<A, Error>, Vec<Result<A, Error>>) -> Result<A, Error>,
        reason: Reason,
        name: &str,
        targs: &[Type],
    ) -> Result<A, Error> {
        let t = flow_js_utils::lookup_builtin_type(env.cx, name, reason.dupe());
        // The code whose type is normalized applies the builtin.
        let strictness_kind = env.cx.type_strictness_kind();
        let t = type_util::typeapp(false, false, reason, t, targs.to_vec(), strictness_kind);
        cont(env, state, t)
    }

    // What the check inherits: the extends clause instantiated as `ThisTypeAppT` instantiates it.
    fn constructed_instance<'cx>(
        env: &mut Env<'_, 'cx>,
        this_typeapp: &Type,
        c: &Type,
        _targs: Option<&[Type]>,
    ) -> Result<Option<Type>, Error> {
        let is_class = |t: &Type| match t.deref() {
            TypeInner::DefT(_, def_t) => match def_t.deref() {
                DefTInner::ClassT(_) => true,
                DefTInner::PolyT(deref!(PolyTData { t_out, .. })) => matches!(
                    t_out.deref(),
                    TypeInner::DefT(_, def_t) if matches!(def_t.deref(), DefTInner::ClassT(_))
                ),
                _ => false,
            },
            _ => false,
        };
        let cs = flow_js_utils::flow_js_result_to_job_error(
            flow_js::FlowJs::possible_concrete_types_for_inspection(
                env.cx,
                type_util::reason_of_t(c),
                c,
            ),
        )?;
        if cs.iter().all(is_class) {
            return Ok(None);
        }
        let instance = flow_js_utils::flow_js_result_to_job_error(
            flow_js::FlowJs::singleton_concrete_type_for_inspection(
                env.cx,
                type_util::reason_of_t(this_typeapp),
                this_typeapp,
            ),
        )?;
        Ok(Some(instance))
    }
}

type FlowNormalizer = Normalizer<FlowInput>;

// Exposed API

fn print_normalizer_banner(genv: &Genv<'_, '_>) {
    if genv.options.verbose_normalizer {
        let banner = "\n======================================== Normalization =======================================\n";
        eprintln!("{}", banner);
    }
}

// The following differ from mapping `from_type` on each input as it folds over
// the input elements of the input propagating the state (caches) after each
// transformation to the next element.
pub fn from_types<'cx, A>(
    f: Option<&dyn Fn(&A)>,
    cx: &mut Context<'cx>,
    genv: &Genv<'_, 'cx>,
    ts: Vec<(A, Type)>,
) -> Vec<(A, Result<ALocElt, Error>)> {
    print_normalizer_banner(genv);
    let mut state = State::empty();
    let mut result = Vec::new();
    for (a, t) in ts {
        if let Some(f) = f {
            f(&a);
        }
        let r = FlowNormalizer::run_type(cx, genv, &mut state, &t);
        match r {
            Ok(t) => result.push((a, Ok(t))),
            Err(s) => result.push((a, Err(s))),
        }
    }
    result
}

pub fn from_type_with_found_computed_type<'cx>(
    cx: &mut Context<'cx>,
    genv: &Genv<'_, 'cx>,
    t: &Type,
) -> (Result<ALocElt, Error>, bool) {
    print_normalizer_banner(genv);
    let mut state = State::empty();
    let result = FlowNormalizer::run_type(cx, genv, &mut state, t);
    (result, state.found_computed_type())
}

pub fn from_type<'cx>(
    cx: &mut Context<'cx>,
    genv: &Genv<'_, 'cx>,
    t: &Type,
) -> Result<ALocElt, Error> {
    print_normalizer_banner(genv);
    let mut state = State::empty();
    FlowNormalizer::run_type(cx, genv, &mut state, t)
}

pub fn from_module_type<'cx>(
    cx: &mut Context<'cx>,
    genv: &Genv<'_, 'cx>,
    t: &ModuleType,
) -> Result<flow_common_ty::ty::Decl<ALoc>, Error> {
    print_normalizer_banner(genv);
    let mut state = State::empty();
    FlowNormalizer::run_module_type(cx, genv, &mut state, t)
}

pub fn expand_members<'cx>(
    force_instance: bool,
    allowed_prop_names: Option<Vec<Name>>,
    cx: &mut Context<'cx>,
    genv: &Genv<'_, 'cx>,
    t: &Type,
) -> Result<ALocTy, Error> {
    print_normalizer_banner(genv);
    let mut state = State::empty();
    FlowNormalizer::run_expand_members(force_instance, allowed_prop_names, cx, genv, &mut state, t)
}

pub fn expand_literal_union<'cx>(
    cx: &mut Context<'cx>,
    genv: &Genv<'_, 'cx>,
    t: &Type,
) -> Result<ALocTy, Error> {
    print_normalizer_banner(genv);
    let mut state = State::empty();
    FlowNormalizer::run_expand_literal_union(cx, genv, &mut state, t)
}

pub fn mk_genv<'a, 'cx: 'a>(
    options: Options,
    typed_ast_opt: Option<&'a ast::Program<ALoc, (ALoc, Type)>>,
    file_sig: Arc<FileSig>,
) -> Genv<'a, 'cx> {
    let file_sig_for_lazy = file_sig.dupe();
    let options_for_lazy = options.dupe();
    let imported_names = ImportedNames::new(move |cx: &mut Context<'cx>| {
        let import_types =
            ty_normalizer_imports::extract_types(cx, &file_sig_for_lazy, typed_ast_opt)?;
        let import_types = import_types
            .into_iter()
            .map(|(name, loc, mode, t)| (name.to_string(), loc, mode, t))
            .collect();
        let mut imported_names = FlowNormalizer::normalize_imports(
            cx,
            file_sig_for_lazy.dupe(),
            typed_ast_opt,
            &options_for_lazy,
            import_types,
        );
        // A declaration imported under a name of its own keeps that name.
        for (name, loc, mode, def_loc) in
            ty_normalizer_imports::namespace_members(cx, &file_sig_for_lazy)?
        {
            imported_names
                .entry(ALocFuzzy(def_loc))
                .or_insert(ImportedIdent(loc, name.to_string(), mode));
        }
        Ok(imported_names)
    });

    Genv {
        options,
        typed_ast_opt,
        file_sig,
        imported_names,
        ref_type_bodies: None,
        instantiation: None,
    }
}

pub fn mk_genv_with_imported_names<'a, 'cx>(
    options: Options,
    typed_ast_opt: Option<&'a ast::Program<ALoc, (ALoc, Type)>>,
    file_sig: Arc<FileSig>,
    imported_names: ImportedNames<'a, 'cx>,
) -> Genv<'a, 'cx> {
    Genv {
        options,
        typed_ast_opt,
        file_sig,
        imported_names,
        ref_type_bodies: None,
        instantiation: None,
    }
}

// A debugging facility for getting quick string representations of Type.t
pub fn debug_string_of_t<'cx>(cx: &mut Context<'cx>, t: &Type) -> String {
    thread_local! {
        static EMPTY_TYPED_AST: &'static ast::Program<ALoc, (ALoc, Type)> =
            Box::leak(Box::new(ast::Program {
                loc: ALoc::default(),
                statements: Arc::from([]),
                interpreter: None,
                comments: None,
                all_comments: Arc::from([]),
            }));
    }
    let typed_ast = EMPTY_TYPED_AST.with(|ast| *ast);
    let file_sig = Arc::new(FileSig::empty());
    let options = Options::default();
    let genv = mk_genv(options, Some(typed_ast), file_sig);
    match from_type(cx, &genv, t) {
        Err(e) => format!("<Error {}>", e.kind),
        Ok(elt) => {
            let opts = ty_printer::PrinterOptions::default();
            ty_printer::string_of_elt_single_line(&elt, &opts)
        }
    }
}
