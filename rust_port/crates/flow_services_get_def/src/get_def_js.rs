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
use flow_analysis::scope_api;
use flow_analysis::scope_builder;
use flow_common::reason;
use flow_common::reason::Name;
use flow_data_structure_wrapper::smol_str::FlowSmolStr;
use flow_parser::PERMISSIVE_PARSE_OPTIONS;
use flow_parser::ast;
use flow_parser::loc::Loc;
use flow_parser::token::TokenKind;
use flow_parser_utils::file_sig::FileSig;
use flow_typing_context::Context;
use flow_typing_type::type_::Type;
use flow_typing_utils::typed_ast_utils::AvailableAst;
use vec1::Vec1;

use crate::get_def_process_location;
use crate::get_def_request::GetDefRequest;
use crate::get_def_types::Purpose;

#[derive(Debug, Clone)]
pub enum GetDefResult {
    Def(BTreeSet<Loc>, Option<FlowSmolStr>), // (the final location of the definition, name)
    /// if an intermediate get-def failed, return partial progress and the error message
    Partial(BTreeSet<Loc>, /* name */ Option<FlowSmolStr>, String),
    /// the input loc didn't point at anything you can call get-def on
    BadLoc(String),
    /// an unexpected, internal error
    DefError(String),
}

fn extract_member_def<'cx>(
    loc_of_aloc: &dyn Fn(&ALoc) -> Loc,
    cx: &mut Context<'cx>,
    file_sig: &std::sync::Arc<FileSig>,
    typed_ast_opt: Option<&ast::Program<ALoc, (ALoc, Type)>>,
    force_instance: bool,
    t: &Type,
    name: &FlowSmolStr,
) -> Result<(Vec1<Loc>, Option<FlowSmolStr>), String> {
    let extract_result = flow_typing::ty_members::extract(
        force_instance,
        Some(vec![Name::new(name.dupe())]),
        None,
        cx,
        typed_ast_opt,
        file_sig.dupe(),
        t,
    )?;

    let key = Name::new(name.dupe());
    let def_locs = extract_result
        .members
        .get(&key)
        .and_then(|info| Vec1::try_from_vec(info.def_locs.clone()).ok());

    match def_locs {
        Some(def_locs) => {
            let mapped = def_locs.mapped_ref(loc_of_aloc);
            Ok((mapped, Some(name.dupe())))
        }
        None => Err(format!("failed to find member {} in members map", name)),
    }
}

fn process_request<'cx>(
    loc_of_aloc: &dyn Fn(&ALoc) -> Loc,
    cx: &mut Context<'cx>,
    typed_ast_opt: Option<&ast::Program<ALoc, (ALoc, Type)>>,
    file_sig: &std::sync::Arc<FileSig>,
    scope_info: &scope_api::ScopeInfo<Loc>,
    request: &GetDefRequest<ALoc, (ALoc, Type)>,
) -> Result<
    Result<(Vec1<Loc>, Option<FlowSmolStr>), String>,
    flow_utils_concurrency::job_error::JobError,
> {
    match request {
        GetDefRequest::Identifier { name, loc: aloc } => {
            let loc = loc_of_aloc(aloc);
            let all_uses = scope_info.all_uses();
            let matching_uses: Vec<&Loc> =
                all_uses.into_iter().filter(|u| u.contains(&loc)).collect();
            match matching_uses.len() {
                1 => {
                    let use_loc = matching_uses[0];
                    let def = scope_info
                        .def_of_use_opt(use_loc)
                        .expect("use should have a def");
                    let def_locs = def.locs.clone();
                    Ok(Ok((def_locs, Some(name.dupe()))))
                }
                0 => match cx.builtin_value_opt(name) {
                    Some((def_loc, _)) => {
                        Ok(Ok((Vec1::new(loc_of_aloc(&def_loc)), Some(name.dupe()))))
                    }
                    None => match cx.builtin_type_opt(name) {
                        Some((def_loc, _)) => {
                            Ok(Ok((Vec1::new(loc_of_aloc(&def_loc)), Some(name.dupe()))))
                        }
                        None => Ok(Ok((Vec1::new(loc), Some(name.dupe())))),
                    },
                },
                _ => Ok(Err(
                    "Scope builder found multiple matching identifiers".to_string()
                )),
            }
        }

        GetDefRequest::Member(member_info) => {
            let name = &member_info.prop_name;
            let (_, t) = &member_info.object_type;
            let force_instance = member_info.force_instance;
            Ok(extract_member_def(
                loc_of_aloc,
                cx,
                file_sig,
                typed_ast_opt,
                force_instance,
                t,
                name,
            ))
        }
    }
}

mod depth {
    use super::*;

    pub const LIMIT: usize = 100;

    pub enum DepthError {
        Cycle(Vec<Loc>),
        DepthExceeded(Vec<Loc>),
    }

    pub struct Depth {
        pub length: usize,
        pub seen: BTreeSet<Loc>,
        pub results: BTreeMap<Loc, GetDefResult>,
        pub locs: Vec<Loc>,
    }

    pub enum DepthOk {
        NoResult,
        CachedResult(GetDefResult),
    }

    impl Depth {
        pub fn empty() -> Self {
            Depth {
                length: 0,
                seen: BTreeSet::new(),
                results: BTreeMap::new(),
                locs: Vec::new(),
            }
        }

        pub fn add(&mut self, loc: &Loc) -> Result<DepthOk, DepthError> {
            if let Some(result) = self.results.get(loc) {
                return Ok(DepthOk::CachedResult(result.clone()));
            }
            let mut new_locs = vec![loc.clone()];
            new_locs.extend(self.locs.iter().cloned());
            if self.seen.contains(loc) {
                return Err(DepthError::Cycle(new_locs));
            }
            if self.length >= LIMIT {
                return Err(DepthError::DepthExceeded(new_locs));
            }
            self.seen.insert(loc.clone());
            self.length += 1;
            self.locs = new_locs;
            Ok(DepthOk::NoResult)
        }

        pub fn cache_result(&mut self, loc: Loc, result: GetDefResult) {
            self.results.insert(loc, result);
        }
    }
}

pub fn get_def<'cx>(
    loc_of_aloc: &dyn Fn(&ALoc) -> Loc,
    cx: &mut Context<'cx>,
    file_sig: &std::sync::Arc<FileSig>,
    file_content: Option<&str>,
    ast: &ast::Program<Loc, Loc>,
    available_ast: AvailableAst<'_>,
    purpose: &Purpose,
    requested_loc: &Loc,
) -> Result<GetDefResult, flow_utils_concurrency::job_error::JobError> {
    // Note: The Result here propagates Worker_should_cancel/timeout via JobError.
    // Both inner errors (from process_request) propagate via the Result chain.
    let require_loc_map = file_sig.require_loc_map();
    let scope_info =
        scope_builder::program(cx.enable_enums(), true, file_sig.requires().is_empty(), ast);
    let is_local_use = |aloc: &ALoc| -> bool {
        let loc = loc_of_aloc(aloc);
        scope_info.is_local_use(&loc)
    };
    let is_legit_require = |source_aloc: &ALoc| -> bool {
        let source_loc = loc_of_aloc(source_aloc);
        require_loc_map
            .values()
            .any(|locs| locs.contains(&source_loc))
    };

    let typed_ast_opt = match &available_ast {
        AvailableAst::TypedAst(tast) => Some(*tast),
        AvailableAst::ALocAst(_) => None,
    };

    fn loop_fn<'cx>(
        loc_of_aloc: &dyn Fn(&ALoc) -> Loc,
        cx: &mut Context<'cx>,
        is_legit_require: &dyn Fn(&ALoc) -> bool,
        typed_ast_opt: Option<&ast::Program<ALoc, (ALoc, Type)>>,
        file_sig: &std::sync::Arc<FileSig>,
        scope_info: &scope_api::ScopeInfo<Loc>,
        available_ast: &AvailableAst<'_>,
        file_content: Option<&str>,
        is_local_use: &dyn Fn(&ALoc) -> bool,
        purpose: &Purpose,
        requested_loc: &Loc,
        depth_state: &mut depth::Depth,
        req_loc: &Loc,
        loop_name: Option<FlowSmolStr>,
    ) -> Result<GetDefResult, flow_utils_concurrency::job_error::JobError> {
        match depth_state.add(req_loc) {
            Err(error) => {
                let trace_of_locs = |locs: &Vec<Loc>| -> String {
                    locs.iter()
                        .map(|l| reason::string_of_loc(None, l))
                        .collect::<Vec<_>>()
                        .join("\n")
                };
                let log_message = match &error {
                    depth::DepthError::DepthExceeded(locs) => format!(
                        "GetDef_js loop depth exceeded {}. Trace (most recent first):\n{}",
                        depth::LIMIT,
                        trace_of_locs(locs)
                    ),
                    depth::DepthError::Cycle(locs) => format!(
                        "GetDef_js cycle detected. Trace (most recent first):\n{}",
                        trace_of_locs(locs)
                    ),
                };
                let mut locs = BTreeSet::new();
                locs.insert(req_loc.dupe());
                Ok(GetDefResult::Partial(locs, loop_name, log_message))
            }
            Ok(depth::DepthOk::CachedResult(result)) => Ok(result),
            Ok(depth::DepthOk::NoResult) => {
                let process_result = get_def_process_location::process_location(
                    cx,
                    available_ast,
                    is_local_use,
                    is_legit_require,
                    *purpose,
                    req_loc.clone(),
                );
                let result = match process_result {
                    get_def_process_location::ProcessLocationResult::OwnNamedDef(aloc, name) => {
                        let mut locs = BTreeSet::new();
                        locs.insert(loc_of_aloc(&aloc));
                        GetDefResult::Def(locs, Some(name))
                    }
                    get_def_process_location::ProcessLocationResult::OwnUnnamedDef(aloc) => {
                        let mut locs = BTreeSet::new();
                        locs.insert(loc_of_aloc(&aloc));
                        GetDefResult::Def(locs, None)
                    }
                    get_def_process_location::ProcessLocationResult::ModuleDef(aloc) => {
                        let mut locs = BTreeSet::new();
                        locs.insert(loc_of_aloc(&aloc));
                        GetDefResult::Def(locs, None)
                    }
                    get_def_process_location::ProcessLocationResult::ModuleTypeDef(t) => {
                        match get_def_process_location::process_type_request(cx, *purpose, &t) {
                            Ok(res_loc) => {
                                let mut locs = BTreeSet::new();
                                locs.insert(loc_of_aloc(&res_loc));
                                GetDefResult::Def(locs, None)
                            }
                            Err(e) => GetDefResult::DefError(e),
                        }
                    }
                    get_def_process_location::ProcessLocationResult::Request(request) => {
                        let inner = process_request(
                            loc_of_aloc,
                            cx,
                            typed_ast_opt,
                            file_sig,
                            scope_info,
                            &request,
                        )?;
                        match inner {
                            Ok((res_locs, name)) => {
                                let mut results: Vec<GetDefResult> = Vec::new();
                                for res_loc in res_locs.iter() {
                                    let r = if *res_loc == *req_loc
                                        || res_loc.source != requested_loc.source
                                    {
                                        let mut locs = BTreeSet::new();
                                        locs.insert(res_loc.clone());
                                        GetDefResult::Def(locs, name.dupe())
                                    } else {
                                        match loop_fn(
                                            loc_of_aloc,
                                            cx,
                                            is_legit_require,
                                            typed_ast_opt,
                                            file_sig,
                                            scope_info,
                                            available_ast,
                                            file_content,
                                            is_local_use,
                                            purpose,
                                            requested_loc,
                                            depth_state,
                                            res_loc,
                                            name.dupe(),
                                        )? {
                                            GetDefResult::BadLoc(_) => {
                                                let mut locs = BTreeSet::new();
                                                locs.insert(res_loc.clone());
                                                GetDefResult::Def(locs, name.dupe())
                                            }
                                            GetDefResult::DefError(msg) => {
                                                let mut locs = BTreeSet::new();
                                                locs.insert(res_loc.clone());
                                                GetDefResult::Partial(locs, name.dupe(), msg)
                                            }
                                            res @ (GetDefResult::Def(..)
                                            | GetDefResult::Partial(..)) => res,
                                        }
                                    };
                                    results.push(r);
                                }
                                results
                                    .into_iter()
                                    .reduce(|res1, res2| match (res1, res2) {
                                        (
                                            GetDefResult::Def(locs1, n),
                                            GetDefResult::Def(locs2, _),
                                        ) => {
                                            let locs = &locs1 | &locs2;
                                            GetDefResult::Def(locs, n)
                                        }
                                        (
                                            GetDefResult::Partial(locs1, n, msg1),
                                            GetDefResult::Partial(locs2, _, msg2),
                                        ) => {
                                            let locs = &locs1 | &locs2;
                                            GetDefResult::Partial(
                                                locs,
                                                n,
                                                format!("{}{}", msg1, msg2),
                                            )
                                        }
                                        (
                                            GetDefResult::Def(locs1, n),
                                            GetDefResult::Partial(locs2, _, msg),
                                        )
                                        | (
                                            GetDefResult::Partial(locs1, n, msg),
                                            GetDefResult::Def(locs2, _),
                                        ) => {
                                            let locs = &locs1 | &locs2;
                                            GetDefResult::Partial(locs, n, msg)
                                        }
                                        (
                                            GetDefResult::BadLoc(_) | GetDefResult::DefError(_),
                                            other,
                                        )
                                        | (
                                            other,
                                            GetDefResult::BadLoc(_) | GetDefResult::DefError(_),
                                        ) => other,
                                    })
                                    .expect("results is non-empty because res_locs is Vec1")
                            }
                            Err(msg) => GetDefResult::DefError(msg),
                        }
                    }
                    get_def_process_location::ProcessLocationResult::Empty(msg) => {
                        GetDefResult::BadLoc(msg.into())
                    }
                    get_def_process_location::ProcessLocationResult::LocNotFound => {
                        let token_at_req_loc = match file_content {
                            None => None,
                            Some(file_content) => {
                                let mut found_token: Option<TokenKind> = None;
                                let mut token_sink = |result: flow_parser::TokenSinkResult| {
                                    if found_token.is_none() && result.token_loc.contains(req_loc) {
                                        found_token = Some(result.token_kind);
                                    }
                                };
                                flow_parser::parse_program_file::<()>(
                                    false,
                                    Some(&mut token_sink),
                                    Some(PERMISSIVE_PARSE_OPTIONS),
                                    cx.file().dupe(),
                                    Ok(file_content),
                                );
                                found_token
                            }
                        };
                        match token_at_req_loc {
                            Some(token) => {
                                GetDefResult::BadLoc(format!("unsupported token: {:?}", token))
                            }
                            None => GetDefResult::BadLoc("not found".to_string()),
                        }
                    }
                    get_def_process_location::ProcessLocationResult::InternalError(err) => {
                        GetDefResult::DefError(format!("{:?}", err))
                    }
                };
                depth_state.cache_result(req_loc.clone(), result.clone());
                Ok(result)
            }
        }
    }

    let mut depth_state = depth::Depth::empty();
    loop_fn(
        loc_of_aloc,
        cx,
        &is_legit_require,
        typed_ast_opt,
        file_sig,
        &scope_info,
        &available_ast,
        file_content,
        &is_local_use,
        purpose,
        requested_loc,
        &mut depth_state,
        requested_loc,
        None,
    )
}
