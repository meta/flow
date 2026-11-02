/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use dupe::Dupe;
use flow_common_ty::ty::ALocTy;
use flow_common_ty::ty::Elt;
use flow_parser::ast;
use flow_parser::ast_visitor::AstVisitor;
use flow_parser::loc::Loc;
use flow_typing_errors::error_message::ErrorMessage;
use flow_typing_errors::intermediate_error_types::ExplanationWithLazyParts;
use flow_typing_type::type_::type_or_type_desc::ErrorTypeDesc;
use flow_typing_type::type_::type_or_type_desc::TypeOrTypeDescT;

use crate::utils::codemod_context;

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

pub mod stats {
    use super::*;

    #[derive(Debug, Clone, Default)]
    pub struct Stats {
        pub number_of_potential_fix_sites: usize,
        pub number_of_added_annotations: usize,
    }

    impl Stats {
        pub fn empty() -> Self {
            Self {
                number_of_potential_fix_sites: 0,
                number_of_added_annotations: 0,
            }
        }

        pub fn combine(c1: &Self, c2: &Self) -> Self {
            Self {
                number_of_potential_fix_sites: c1.number_of_potential_fix_sites
                    + c2.number_of_potential_fix_sites,
                number_of_added_annotations: c1.number_of_added_annotations
                    + c2.number_of_added_annotations,
            }
        }

        pub fn serialize(&self) -> Vec<String> {
            vec![
                format!(
                    "number_of_potential_fix_sites: {}",
                    self.number_of_potential_fix_sites
                ),
                format!(
                    "number_of_added_annotations: {}",
                    self.number_of_added_annotations
                ),
            ]
        }

        pub fn report(&self) -> Vec<String> {
            vec![
                string_of_row(
                    2,
                    "Number of potential fix sites",
                    self.number_of_potential_fix_sites,
                ),
                string_of_row(
                    2,
                    "Number of annotations added",
                    self.number_of_added_annotations,
                ),
            ]
        }
    }
}

use stats::Stats;

impl flow_services_code_action::insert_type_utils::BaseStats for Stats {
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

pub struct AnnotateLiteralDeclarationsMapper<'a, 'cx> {
    cctx: &'a mut codemod_context::typed::TypedCodemodContext<'cx>,
    max_type_size: i32,
    stats: Option<Stats>,
}

impl<'a, 'cx> AnnotateLiteralDeclarationsMapper<'a, 'cx> {
    pub fn new(
        max_type_size: i32,
        cctx: &'a mut codemod_context::typed::TypedCodemodContext<'cx>,
    ) -> Self {
        Self {
            cctx,
            max_type_size,
            stats: None,
        }
    }

    pub fn post_run(&self) -> Stats {
        self.stats
            .clone()
            .expect("stats should be set by program()")
    }

    pub fn program(&mut self, prog: ast::Program<Loc, Loc>) -> ast::Program<Loc, Loc> {
        self.stats = None;
        let cx = &mut self.cctx.cx;
        let reader = &self.cctx.reader;
        let file_sig = &self.cctx.file_sig;
        let typed_ast = &self.cctx.typed_ast;
        let loc_of_aloc = |aloc: &flow_aloc::ALoc| reader.loc_of_aloc(aloc);
        let get_ast_from_transaction =
            |file: &flow_parser::file_key::FileKey| -> Option<ast::Program<Loc, Loc>> {
                reader.get_ast(file).map(|arc| (*arc).clone())
            };
        let get_type_sig = |file: &flow_parser::file_key::FileKey| -> Option<
            flow_type_sig::packed_type_sig::Module<
                flow_type_sig::compact_table::Index<flow_aloc::ALoc>,
            >,
        > {
            reader.get_type_sig(file).map(|arc| {
                let bytes = bincode::serde::encode_to_vec(&*arc, bincode::config::legacy())
                    .expect("get_type_sig: serialize");
                bincode::serde::decode_from_slice(&bytes, bincode::config::legacy())
                    .expect("get_type_sig: deserialize")
                    .0
            })
        };
        let get_haste_module_info =
            |file: &flow_parser::file_key::FileKey| -> Option<flow_common_modulename::HasteModuleInfo> {
                reader.get_haste_module_info(file)
            };
        let include_suppressions = cx.include_suppressions();
        let mut suppressions = self.cctx.error_suppressions.clone();
        let (errors, _warnings) = suppressions.filter_lints(
            cx.errors(),
            cx.aloc_tables(),
            include_suppressions,
            cx.severity_cover(),
        );
        let raw_sites: Vec<(Loc, ALocTy)> = errors.fold(Vec::new(), |mut acc, error| {
            match error.msg_of_error() {
                ErrorMessage::EInvariantSubtypingWithUseOp(data) if data.explanation.is_some() => {
                    let explanation = data.explanation.as_ref().unwrap();
                    let extracted: Option<(&flow_aloc::ALoc, ALocTy)> = match explanation {
                        ExplanationWithLazyParts::LazyExplanationInvariantSubtypingDueToMutableArray {
                            lower_array_loc,
                            lower_array_desc: TypeOrTypeDescT::TypeDesc(ErrorTypeDesc::ObjectLiteral | ErrorTypeDesc::ArrayLiteral),
                            upper_array_desc: TypeOrTypeDescT::TypeDesc(ErrorTypeDesc::Normalized(Elt::Type(upper_ty))),
                            ..
                        } => Some((lower_array_loc, upper_ty.dupe())),
                        ExplanationWithLazyParts::LazyExplanationInvariantSubtypingDueToMutableProperty {
                            lower_obj_loc,
                            lower_obj_desc: TypeOrTypeDescT::TypeDesc(ErrorTypeDesc::ObjectLiteral | ErrorTypeDesc::ArrayLiteral),
                            upper_obj_desc: TypeOrTypeDescT::TypeDesc(ErrorTypeDesc::Normalized(Elt::Type(upper_ty))),
                            ..
                        } => Some((lower_obj_loc, upper_ty.dupe())),
                        ExplanationWithLazyParts::LazyExplanationInvariantSubtypingDueToMutableProperties {
                            lower_obj_loc,
                            lower_obj_desc: TypeOrTypeDescT::TypeDesc(ErrorTypeDesc::ObjectLiteral | ErrorTypeDesc::ArrayLiteral),
                            upper_obj_desc: TypeOrTypeDescT::TypeDesc(ErrorTypeDesc::Normalized(Elt::Type(upper_ty))),
                            ..
                        } => Some((lower_obj_loc, upper_ty.dupe())),
                        _ => None,
                    };
                    if let Some((lower_loc, upper_ty)) = extracted
                    {
                        acc.push((loc_of_aloc(lower_loc), upper_ty));
                    }
                    acc
                }
                _ => acc,
            }
        });
        let mut sorted_sites = raw_sites;
        sorted_sites.sort_by(|(loc1, _), (loc2, _)| loc1.cmp(loc2));
        let mut annotation_sites: Vec<(Loc, ALocTy)> = Vec::new();
        {
            let mut i = 0;
            while i < sorted_sites.len() {
                let loc = sorted_sites[i].0.clone();
                let mut tys: Vec<ALocTy> = vec![sorted_sites[i].1.dupe()];
                let mut j = i + 1;
                while j < sorted_sites.len() && sorted_sites[j].0 == loc {
                    tys.push(sorted_sites[j].1.dupe());
                    j += 1;
                }
                let ty = if tys.len() == 1 {
                    tys.into_iter().next().unwrap()
                } else {
                    flow_common_ty::ty::mk_inter(tys).unwrap()
                };
                let ty = flow_common_ty::ty_utils::simplify_type(true, None, ty);
                annotation_sites.push((loc, ty));
                i = j;
            }
        }
        let (prog_, fixed_count) = annotation_sites.iter().fold(
            (prog, 0usize),
            |(ast, fixed_count), (lower_loc, upper_ty)| {
                if flow_common_ty::ty_utils::size_of_type(
                    Some(self.max_type_size as usize),
                    upper_ty,
                )
                .is_none()
                {
                    (ast, fixed_count)
                } else {
                    let result = flow_services_code_action::insert_type::insert_type_ty(
                        cx,
                        &loc_of_aloc,
                        &get_ast_from_transaction,
                        &get_haste_module_info,
                        &get_type_sig,
                        file_sig,
                        typed_ast,
                        false,
                        None, // remote_converter
                        &ast,
                        lower_loc.clone(),
                        upper_ty.dupe(),
                    );
                    match result {
                        Ok(ast_) => {
                            let fixed_count = if ast == ast_ {
                                fixed_count
                            } else {
                                fixed_count + 1
                            };
                            (ast_, fixed_count)
                        }
                        Err(_) => (ast, fixed_count),
                    }
                }
            },
        );
        self.stats = Some(Stats {
            number_of_potential_fix_sites: annotation_sites.len(),
            number_of_added_annotations: fixed_count,
        });
        self.map_program(&prog_)
    }
}

impl<'ast, 'a, 'cx> AstVisitor<'ast, Loc> for AnnotateLiteralDeclarationsMapper<'a, 'cx> {
    fn normalize_loc(loc: &'ast Loc) -> &'ast Loc {
        loc
    }

    fn normalize_type(type_: &'ast Loc) -> &'ast Loc {
        type_
    }
}
