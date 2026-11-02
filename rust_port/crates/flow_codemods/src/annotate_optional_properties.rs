/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::cmp::Ordering;
use std::collections::BTreeSet;
use std::sync::Arc;

use dupe::Dupe;
use flow_aloc::ALoc;
use flow_common::reason::Name;
use flow_common_ty::ty::Prop;
use flow_parser::ast;
use flow_parser::ast_visitor::AstVisitor;
use flow_parser::loc::Loc;
use flow_parser::loc_sig::LocSig;

use crate::utils::codemod_annotator;
use crate::utils::codemod_context;

fn data_of_prop_missing_error(
    loc_of_aloc: &dyn Fn(&ALoc) -> Loc,
    error: &flow_typing_errors::flow_error::FlowError<ALoc>,
) -> Option<PropData> {
    use flow_typing_errors::error_message::EIncompatiblePropData;
    use flow_typing_errors::error_message::EPropNotFoundInLookupData;
    use flow_typing_errors::error_message::ErrorMessage;
    use flow_typing_type::type_::SetPropertyData;
    use flow_typing_type::type_::VirtualRootUseOp;
    let msg = error.msg_of_error();
    let op = flow_typing_errors::error_message::util_use_op_of_msg(
        flow_typing_type::type_::unknown_use(),
        |op| op.clone(),
        msg,
    );
    let root = flow_typing_type::type_::root_of_use_op(&op);
    let value_loc = match root {
        VirtualRootUseOp::SetProperty(box SetPropertyData { value, .. }) => &value.loc,
        VirtualRootUseOp::GetProperty(value) => value.def_loc(),
        _ => return None,
    };
    let (name, object) = match msg {
        ErrorMessage::EIncompatibleProp(box EIncompatiblePropData {
            object,
            prop: Some(name),
            ..
        })
        | ErrorMessage::EPropNotFoundInLookup(box EPropNotFoundInLookupData {
            object,
            prop_name: Some(name),
            ..
        }) => (name, object),
        _ => return None,
    };
    let obj_loc = loc_of_aloc(&object.definition_loc);
    let init_locs = vec![loc_of_aloc(value_loc)];
    let prop_accesses_ = prop_accesses(&op);
    Some(PropData {
        obj_loc,
        name: name.dupe(),
        init_locs,
        prop_accesses: prop_accesses_,
    })
}

const WIDTH: usize = 45;

fn string_of_row(indent: usize, name: &str, i: usize) -> String {
    let len = name.len();
    let padding = WIDTH.saturating_sub(len + indent + 7);
    format!(
        "{:indent$}{name}:{:padding$}{i:6}",
        "",
        "",
        indent = indent,
        name = name,
        padding = padding,
        i = i,
    )
}

#[derive(Debug, Clone)]
pub struct PropData {
    pub obj_loc: Loc,
    pub init_locs: Vec<Loc>,
    pub prop_accesses: Vec<Name>,
    pub name: Name,
}

impl Eq for PropData {}

impl PartialEq for PropData {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl PartialOrd for PropData {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for PropData {
    fn cmp(&self, other: &Self) -> Ordering {
        self.obj_loc
            .cmp(&other.obj_loc)
            .then_with(|| self.name.cmp(&other.name))
            .then_with(|| self.init_locs.cmp(&other.init_locs))
            .then_with(|| self.prop_accesses.cmp(&other.prop_accesses))
    }
}

pub type PropDataSet = BTreeSet<PropData>;

pub fn prop_accesses(op: &flow_typing_type::type_::UseOp) -> Vec<Name> {
    flow_typing_type::type_::fold_use_op(
        op,
        |_| vec![],
        &|mut acc: Vec<Name>, frame: &flow_typing_type::type_::FrameUseOp| match frame {
            flow_typing_type::type_::FrameUseOp::PropertyCompatibility(
                box flow_typing_type::type_::PropertyCompatibilityData {
                    prop: Some(name), ..
                },
            ) => {
                acc.push(name.clone());
                acc
            }
            _ => acc,
        },
    )
}

pub mod error_stats {
    use super::*;

    #[derive(Debug, Clone, Default)]
    pub struct ErrorStats {
        pub num_total_errors: usize,
    }

    impl ErrorStats {
        pub fn empty() -> Self {
            Self {
                num_total_errors: 0,
            }
        }

        pub fn combine(c1: &Self, c2: &Self) -> Self {
            Self {
                num_total_errors: c1.num_total_errors + c2.num_total_errors,
            }
        }

        pub fn serialize(&self) -> Vec<String> {
            vec![format!("total_errors: {}", self.num_total_errors)]
        }

        pub fn report(&self) -> Vec<String> {
            vec![string_of_row(2, "Number of errors", self.num_total_errors)]
        }
    }
}

use error_stats::ErrorStats;

impl flow_services_code_action::insert_type_utils::BaseStats for ErrorStats {
    fn empty() -> Self {
        Self::empty()
    }

    fn combine(a: &Self, b: &Self) -> Self {
        Self::combine(a, b)
    }

    fn serialize(&self) -> Vec<String> {
        self.serialize()
    }

    fn report(&self) -> Vec<String> {
        self.report()
    }
}

pub struct AnnotateOptionalPropertiesMapper<'a, 'cx> {
    max_type_size: i32,
    default_any: bool,
    prop_data: PropDataSet,
    codemod_error_locs: BTreeSet<Loc>,
    mapper: codemod_annotator::Mapper<'a, 'cx, ErrorStats>,
}

impl<'a, 'cx> AnnotateOptionalPropertiesMapper<'a, 'cx> {
    pub fn new(
        max_type_size: i32,
        default_any: bool,
        cctx: &'a mut codemod_context::typed::TypedCodemodContext<'cx>,
    ) -> Self {
        let lint_severities = codemod_context::typed::lint_severities(cctx);
        Self {
            max_type_size,
            default_any,
            prop_data: PropDataSet::new(),
            codemod_error_locs: BTreeSet::new(),
            mapper: codemod_annotator::Mapper {
                added_annotations_locmap: std::collections::BTreeMap::new(),
                wont_annotate_locs: BTreeSet::new(),
                codemod_error_locs: BTreeSet::new(),
                remote_converter: None,
                cctx,
                default_any,
                generalize_maybe: false,
                generalize_react_mixed_element: false,
                lint_severities,
                max_type_size: max_type_size as usize,
                merge_arrays: false,
                acc: flow_services_code_action::insert_type_utils::Acc::empty(),
                _phantom: std::marker::PhantomData,
            },
        }
    }

    pub fn post_run(&self) -> ErrorStats {
        ErrorStats {
            num_total_errors: self.prop_data.len(),
        }
    }

    pub fn acc(&mut self) -> flow_services_code_action::insert_type_utils::Acc<ErrorStats> {
        self.mapper.acc.stats.extra = self.post_run();
        self.mapper.acc.clone()
    }

    fn get_annot(
        &mut self,
        ploc: Loc,
        ty: &Result<
            flow_common_ty::ty::ALocTy,
            Vec<flow_services_code_action::insert_type_utils::error::Kind>,
        >,
        annot: ast::types::AnnotationOrHint<Loc, Loc>,
    ) -> ast::types::AnnotationOrHint<Loc, Loc> {
        match ty {
            Err(errs) => {
                for err in errs {
                    self.mapper.acc.error(&ploc, err);
                }
                self.mapper.codemod_error_locs.insert(ploc.clone());
                if self.default_any {
                    flow_services_code_action::insert_type_utils::Acc::<ErrorStats>::info(
                        &ploc,
                        &flow_services_code_action::insert_type_utils::info::T::DefaultAny,
                    );
                    let explicit_any = flow_common_ty::ty::explicit_any();
                    let ty_or = codemod_annotator::TyOrTypeAst::Ty_(explicit_any);
                    match self.mapper.annotate_node(ploc.clone(), ty_or, |a| {
                        ast::types::AnnotationOrHint::Available(a)
                    }) {
                        Ok(y) => {
                            flow_services_code_action::insert_type_utils::Acc::<ErrorStats>::debug(
                                &ploc,
                                &flow_services_code_action::insert_type_utils::debug::T::AddAnnotation(
                                    flow_services_code_action::insert_type_utils::debug::NodeKind::Prop,
                                ),
                            );
                            y
                        }
                        Err(e) => {
                            self.mapper.acc.error(&ploc, &e);
                            self.mapper.codemod_error_locs.insert(ploc.clone());
                            let _desc =
                                flow_services_code_action::insert_type_utils::error::serialize(&e);
                            flow_services_code_action::insert_type_utils::Acc::<ErrorStats>::info(
                                &ploc,
                                &flow_services_code_action::insert_type_utils::info::T::DefaultAny,
                            );
                            let flowfixme_ast =
                                codemod_context::typed::flowfixme_ast(&self.mapper.lint_severities);
                            ast::types::AnnotationOrHint::Available(ast::types::Annotation {
                                loc: Loc::none(),
                                annotation: flowfixme_ast,
                            })
                        }
                    }
                } else {
                    annot
                }
            }
            Ok(aloc_ty) => {
                let ty_or = codemod_annotator::TyOrTypeAst::Ty_(aloc_ty.dupe());
                match self.mapper.annotate_node(ploc.clone(), ty_or, |a| {
                    ast::types::AnnotationOrHint::Available(a)
                }) {
                    Ok(y) => {
                        flow_services_code_action::insert_type_utils::Acc::<ErrorStats>::debug(
                            &ploc,
                            &flow_services_code_action::insert_type_utils::debug::T::AddAnnotation(
                                flow_services_code_action::insert_type_utils::debug::NodeKind::Prop,
                            ),
                        );
                        y
                    }
                    Err(e) if self.default_any => {
                        self.mapper.acc.error(&ploc, &e);
                        self.mapper.codemod_error_locs.insert(ploc.clone());
                        let _desc =
                            flow_services_code_action::insert_type_utils::error::serialize(&e);
                        flow_services_code_action::insert_type_utils::Acc::<ErrorStats>::info(
                            &ploc,
                            &flow_services_code_action::insert_type_utils::info::T::DefaultAny,
                        );
                        let flowfixme_ast =
                            codemod_context::typed::flowfixme_ast(&self.mapper.lint_severities);
                        ast::types::AnnotationOrHint::Available(ast::types::Annotation {
                            loc: Loc::none(),
                            annotation: flowfixme_ast,
                        })
                    }
                    Err(_) => annot,
                }
            }
        }
    }

    fn report_errors(
        &mut self,
        loc: Loc,
        errors: &[flow_services_code_action::insert_type_utils::error::Kind],
    ) {
        for e in errors {
            self.mapper.acc.error(&loc, e);
        }
        self.codemod_error_locs.insert(loc);
    }

    fn dedup_props(&self, mut props: Vec<Prop<ALoc>>) -> Vec<Prop<ALoc>> {
        props.sort_by(|p1, p2| match (p1, p2) {
            (Prop::NamedProp { name: name1, .. }, Prop::NamedProp { name: name2, .. }) => {
                name1.cmp(name2)
            }
            _ => p1.cmp(p2),
        });
        props.dedup_by(|a, b| match (a as &Prop<ALoc>, b as &Prop<ALoc>) {
            (Prop::NamedProp { name: name1, .. }, Prop::NamedProp { name: name2, .. }) => {
                name1 == name2
            }
            _ => a == b,
        });
        props
    }

    #[allow(unreachable_code)]
    fn get_props_for_obj(&mut self, oloc: Loc) -> Vec<Prop<ALoc>> {
        let relevant_errors: PropDataSet = self
            .prop_data
            .iter()
            .filter(|data| data.obj_loc == oloc)
            .cloned()
            .collect();
        let mut merged_errors = PropDataSet::new();
        for data in &relevant_errors {
            let existing = merged_errors
                .iter()
                .find(|d: &&PropData| d.name == data.name)
                .cloned();
            match existing {
                Some(previous) => {
                    let mut new_data = data.clone();
                    new_data.init_locs = data.init_locs.clone();
                    new_data
                        .init_locs
                        .extend(previous.init_locs.iter().cloned());
                    merged_errors.remove(&previous);
                    merged_errors.insert(new_data);
                }
                None => {
                    merged_errors.insert(data.clone());
                }
            }
        }
        use std::sync::Arc;

        use flow_common_ty::ty::ALocTy;
        use flow_common_ty::ty::AnyKind;
        use flow_common_ty::ty::NamedProp as NP;
        use flow_common_ty::ty::ObjT;
        use flow_common_ty::ty::Polarity;
        use flow_common_ty::ty::PropSource;
        use flow_common_ty::ty::Ty as TyEnum;

        fn remove_anys(t: &ALocTy) -> Vec<ALocTy> {
            match &**t {
                TyEnum::Any(_) => vec![],
                TyEnum::Union(_, t1, t2, ts) => {
                    let mut result = remove_anys(t1);
                    result.extend(remove_anys(t2));
                    for ti in ts.iter() {
                        result.extend(remove_anys(ti));
                    }
                    result
                }
                _ => vec![t.clone()],
            }
        }

        fn access_prop(accesses: &[Name], t: &ALocTy) -> ALocTy {
            match &**t {
                TyEnum::Obj(box ObjT { obj_props, .. }) => match accesses {
                    [] => t.clone(),
                    [n, rest @ ..] => obj_props
                        .iter()
                        .find_map(|p| match p {
                            Prop::NamedProp {
                                name,
                                prop: NP::Field { t, .. },
                                ..
                            } if name == n => Some(access_prop(rest, t)),
                            _ => None,
                        })
                        .unwrap_or_else(|| Arc::new(TyEnum::Any(AnyKind::Untyped))),
                },
                _ if !accesses.is_empty() => Arc::new(TyEnum::Any(AnyKind::Untyped)),
                _ => t.clone(),
            }
        }

        merged_errors.iter().fold(vec![], |mut acc, data| {
            let PropData {
                init_locs,
                name,
                prop_accesses,
                ..
            } = data;
            if name.matches_str("") {
                acc
            } else {
                let ts: Vec<ALocTy> = init_locs
                    .iter()
                    .flat_map(|loc| {
                        crate::utils::codemod_annotator::get_validated_ty(
                            self.mapper.cctx,
                            self.max_type_size,
                            loc.clone(),
                        )
                        .into_iter()
                    })
                    .flat_map(|ty| remove_anys(&ty))
                    .map(|ty| access_prop(prop_accesses, &ty))
                    .collect();
                let t = match &ts[..] {
                    [] => Some(Arc::new(TyEnum::Any(AnyKind::Untyped))),
                    [t] => Some(t.clone()),
                    [t, rest @ ..] => {
                        let mut ts = Vec::with_capacity(1 + rest.len());
                        ts.push(t.clone());
                        ts.extend(rest.iter().cloned());
                        Some(flow_common_ty::ty::mk_union(false, ts).unwrap_or_else(|| t.clone()))
                    }
                };
                match t {
                    Some(t) => {
                        acc.push(Prop::NamedProp {
                            name: name.clone(),
                            inherited: false,
                            source: PropSource::Other,
                            def_locs: Arc::from([]),
                            prop: NP::Field {
                                t,
                                optional: true,
                                polarity: Polarity::Neutral,
                            },
                        });
                        acc
                    }
                    None => acc,
                }
            }
        })
    }

    pub fn variable_declarator(
        &mut self,
        kind: ast::VariableKind,
        decl: ast::statement::variable::Declarator<Loc, Loc>,
    ) -> ast::statement::variable::Declarator<Loc, Loc> {
        let loc = decl.loc.clone();
        let id = decl.id.clone();
        let init = decl.init.clone();
        match &init {
            Some(init_expr)
                if matches!(
                    &**init_expr,
                    ast::expression::ExpressionInner::Object { .. }
                ) =>
            {
                let oloc = init_expr.loc().clone();
                let obj_props = self.get_props_for_obj(oloc.clone());
                if obj_props.is_empty() {
                    flow_parser::ast_visitor::map_variable_declarator_default(self, kind, &decl)
                } else {
                    let id = match &id {
                        ast::pattern::Pattern::Identifier { loc: ploc, inner }
                            if matches!(&inner.annot, ast::types::AnnotationOrHint::Missing(_)) =>
                        {
                            let ploc = ploc.clone();
                            match crate::utils::codemod_annotator::get_validated_ty(
                                self.mapper.cctx,
                                self.max_type_size,
                                ploc.clone(),
                            ) {
                                Ok(ref ty_val)
                                    if matches!(&**ty_val, flow_common_ty::ty::Ty::Obj { .. }) =>
                                {
                                    use std::sync::Arc;

                                    use flow_common_ty::ty::ObjT;
                                    use flow_common_ty::ty::Ty as TyEnum;
                                    let ty_obj = match &**ty_val {
                                        TyEnum::Obj(obj) => {
                                            let mut merged_props: Vec<Prop<ALoc>> =
                                                obj_props.clone();
                                            merged_props.extend(obj.obj_props.iter().cloned());
                                            let deduped = self.dedup_props(merged_props);
                                            Arc::new(TyEnum::Obj(Box::new(ObjT {
                                                obj_def_loc: obj.obj_def_loc.clone(),
                                                obj_props: Arc::from(deduped),
                                                obj_kind: obj.obj_kind.clone(),
                                            })))
                                        }
                                        _ => unreachable!(),
                                    };
                                    let annot_ = self.get_annot(
                                        ploc.clone(),
                                        &Ok(ty_obj),
                                        inner.annot.clone(),
                                    );
                                    ast::pattern::Pattern::Identifier {
                                        loc: ploc,
                                        inner: Arc::new(ast::pattern::Identifier {
                                            name: inner.name.clone(),
                                            annot: annot_,
                                            optional: inner.optional,
                                        }),
                                    }
                                }
                                Ok(_) => id.clone(),
                                Err(errs) => {
                                    self.report_errors(oloc.clone(), &errs);
                                    id.clone()
                                }
                            }
                        }
                        _ => id.clone(),
                    };
                    ast::statement::variable::Declarator { loc, id, init }
                }
            }
            _ => flow_parser::ast_visitor::map_variable_declarator_default(self, kind, &decl),
        }
    }

    pub fn program(&mut self, prog: ast::Program<Loc, Loc>) -> ast::Program<Loc, Loc> {
        let cx = &self.mapper.cctx.cx;
        let unsuppressable_error_codes: BTreeSet<
            flow_data_structure_wrapper::smol_str::FlowSmolStr,
        > = self
            .mapper
            .cctx
            .options
            .unsuppressable_error_codes
            .iter()
            .cloned()
            .collect();
        let reader = &self.mapper.cctx.reader;
        let loc_of_aloc = |aloc: &flow_aloc::ALoc| reader.loc_of_aloc(aloc);
        let get_ast =
            |file: &flow_parser::file_key::FileKey| -> Option<Arc<ast::Program<Loc, Loc>>> {
                reader.get_ast(file)
            };
        let suppressions = self.mapper.cctx.error_suppressions.clone();
        let root = &self.mapper.cctx.options.root;
        let error_is_suppressed =
            |error: &flow_typing_errors::flow_error::FlowError<flow_aloc::ALoc>| -> bool {
                let mut unused = suppressions.clone();
                let (errors, _) = suppressions.filter_suppressed_errors(
                    root,
                    None,  // file_options
                    false, // node_modules_errors
                    &unsuppressable_error_codes,
                    loc_of_aloc,
                    get_ast,
                    |file| cx.is_global_libdef(file),
                    &flow_typing_errors::flow_error::ErrorSet::singleton(error.clone()),
                    &mut unused,
                );
                errors.is_empty()
            };
        self.prop_data = cx.errors().fold(PropDataSet::new(), |mut acc, error| {
            if error_is_suppressed(&error) {
                acc
            } else {
                match data_of_prop_missing_error(&loc_of_aloc, &error) {
                    Some(data) => {
                        acc.insert(data);
                        acc
                    }
                    None => acc,
                }
            }
        });
        if self.prop_data.is_empty() {
            prog
        } else {
            self.mapper.initialize_program_state(&prog);
            let prog_ = self.map_program(&prog);
            let extra = self.post_run();
            self.mapper.finalize_program(&prog, prog_, extra)
        }
    }
}

impl<'ast, 'a, 'cx> AstVisitor<'ast, Loc> for AnnotateOptionalPropertiesMapper<'a, 'cx> {
    fn normalize_loc(loc: &'ast Loc) -> &'ast Loc {
        loc
    }

    fn normalize_type(type_: &'ast Loc) -> &'ast Loc {
        type_
    }

    fn map_variable_declarator(
        &mut self,
        kind: ast::VariableKind,
        decl: &'ast ast::statement::variable::Declarator<Loc, Loc>,
    ) -> ast::statement::variable::Declarator<Loc, Loc> {
        self.variable_declarator(kind, decl.clone())
    }
}
