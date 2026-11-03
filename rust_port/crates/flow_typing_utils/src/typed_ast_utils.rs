/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::collections::BTreeMap;

use dupe::Dupe;
use flow_aloc::ALoc;
use flow_aloc::ALocMap;
use flow_common::polarity::Polarity;
use flow_parser::ast;
use flow_parser::ast::VarianceKind;
use flow_parser::polymorphic_ast_mapper;
use flow_parser::polymorphic_ast_mapper::LocMapper;
use flow_typing_context::Context;
use flow_typing_type::type_::AnySource;
use flow_typing_type::type_::Type;
use flow_typing_type::type_::UnsoundnessKind;
use flow_typing_type::type_::any_t;

pub enum AvailableAst<'a> {
    TypedAst(&'a ast::Program<ALoc, (ALoc, Type)>),
    ALocAst(&'a ast::Program<ALoc, ALoc>),
}

pub fn typed_ast_of_available_ast(
    ast: AvailableAst<'_>,
) -> Option<&ast::Program<ALoc, (ALoc, Type)>> {
    match ast {
        AvailableAst::TypedAst(tast) => Some(tast),
        AvailableAst::ALocAst(_) => None,
    }
}

pub fn polarity<M: Dupe>(variance: Option<&ast::Variance<M>>) -> Polarity {
    match variance {
        Some(ast::Variance {
            kind: VarianceKind::Plus,
            ..
        }) => Polarity::Positive,
        Some(ast::Variance {
            kind: VarianceKind::Minus,
            ..
        }) => Polarity::Negative,
        Some(ast::Variance {
            kind: VarianceKind::InOut,
            ..
        }) => Polarity::Neutral,
        Some(ast::Variance {
            kind: VarianceKind::Readonly | VarianceKind::Out,
            ..
        }) => Polarity::Positive,
        Some(ast::Variance {
            kind: VarianceKind::In | VarianceKind::Writeonly,
            ..
        }) => Polarity::Negative,
        None => Polarity::Neutral,
    }
}

struct TypeAtALocMapFolder<M, T> {
    map: BTreeMap<M, T>,
}

impl<M, T> TypeAtALocMapFolder<M, T> {
    fn new() -> Self {
        Self {
            map: BTreeMap::new(),
        }
    }

    fn to_map(self) -> BTreeMap<M, T> {
        self.map
    }
}

impl<M: Ord + Dupe, T: Dupe> LocMapper<M, (M, T), M, (M, T)> for TypeAtALocMapFolder<M, T> {
    fn on_loc_annot(&mut self, x: &M) -> Result<M, !> {
        Ok(x.dupe())
    }

    fn on_type_annot(&mut self, x: &(M, T)) -> Result<(M, T), !> {
        let (loc, t) = x;
        self.map.insert(loc.dupe(), t.dupe());
        Ok((loc.dupe(), t.dupe()))
    }

    fn on_jsx_props_annot(&mut self, x: &(M, T)) -> Result<(M, T), !> {
        Ok(x.dupe())
    }
}

struct TypeAtALocListFolder<M, T> {
    list: Vec<(M, T)>,
}

impl<M, T> TypeAtALocListFolder<M, T> {
    fn new() -> Self {
        Self { list: Vec::new() }
    }

    fn to_list(self) -> Vec<(M, T)> {
        self.list
    }
}

impl<M: Dupe, T: Dupe> LocMapper<M, (M, T), M, (M, T)> for TypeAtALocListFolder<M, T> {
    fn on_loc_annot(&mut self, x: &M) -> Result<M, !> {
        Ok(x.clone())
    }

    fn on_type_annot(&mut self, x: &(M, T)) -> Result<(M, T), !> {
        let (loc, t) = x;
        self.list.push((loc.clone(), t.clone()));
        Ok(x.clone())
    }

    fn on_jsx_props_annot(&mut self, x: &(M, T)) -> Result<(M, T), !> {
        Ok(x.clone())
    }
}

pub fn typed_ast_to_map(typed_ast: &ast::Program<ALoc, (ALoc, Type)>) -> ALocMap<Type> {
    let mut folder = TypeAtALocMapFolder::new();
    let Ok(_) = polymorphic_ast_mapper::program(&mut folder, typed_ast);
    folder.to_map().into_iter().collect()
}

pub fn typed_ast_to_list(typed_ast: &ast::Program<ALoc, (ALoc, Type)>) -> Vec<(ALoc, Type)> {
    let mut folder = TypeAtALocListFolder::new();
    let Ok(_) = polymorphic_ast_mapper::program(&mut folder, typed_ast);
    folder.to_list()
}

/// Error nodes are typed at `any`. Do not change this type as it might change current behavior.
pub struct ErrorMapper;

impl LocMapper<ALoc, ALoc, ALoc, (ALoc, Type)> for ErrorMapper {
    fn on_loc_annot(&mut self, loc: &ALoc) -> Result<ALoc, !> {
        Ok(loc.dupe())
    }

    fn on_type_annot(&mut self, loc: &ALoc) -> Result<(ALoc, Type), !> {
        Ok((loc.dupe(), any_t::at(AnySource::AnyError(None), loc.dupe())))
    }
}

/// Used in skipped body due to placeholder function types.
pub struct PlaceholderMapper<'a, 'cx> {
    cx: &'a mut Context<'cx>,
}

impl<'a, 'cx> PlaceholderMapper<'a, 'cx> {
    pub fn new(cx: &'a mut Context<'cx>) -> Self {
        Self { cx }
    }
}

impl LocMapper<ALoc, ALoc, ALoc, (ALoc, Type)> for PlaceholderMapper<'_, '_> {
    fn on_loc_annot(&mut self, loc: &ALoc) -> Result<ALoc, !> {
        Ok(loc.dupe())
    }

    fn on_type_annot(&mut self, loc: &ALoc) -> Result<(ALoc, Type), !> {
        let reason = flow_common::reason::mk_reason(None, loc.dupe());
        Ok((
            loc.dupe(),
            self.cx
                .mk_placeholder(reason.dupe(), flow_typing_type::type_::mixed_t::why(reason)),
        ))
    }
}

/// Used in unimplemented cases or unsupported nodes
pub struct UnimplementedMapper;

impl LocMapper<ALoc, ALoc, ALoc, (ALoc, Type)> for UnimplementedMapper {
    fn on_loc_annot(&mut self, loc: &ALoc) -> Result<ALoc, !> {
        Ok(loc.dupe())
    }

    fn on_type_annot(&mut self, loc: &ALoc) -> Result<(ALoc, Type), !> {
        Ok((
            loc.dupe(),
            any_t::at(
                AnySource::Unsound(UnsoundnessKind::Unimplemented),
                loc.dupe(),
            ),
        ))
    }
}

/// Code is not checked at all
pub struct UncheckedMapper;

impl LocMapper<ALoc, ALoc, ALoc, (ALoc, Type)> for UncheckedMapper {
    fn on_loc_annot(&mut self, loc: &ALoc) -> Result<ALoc, !> {
        Ok(loc.dupe())
    }

    fn on_type_annot(&mut self, loc: &ALoc) -> Result<(ALoc, Type), !> {
        Ok((
            loc.dupe(),
            any_t::at(AnySource::Unsound(UnsoundnessKind::Unchecked), loc.dupe()),
        ))
    }
}

pub struct UntypedAstMapper;

impl LocMapper<ALoc, (ALoc, Type), ALoc, ALoc> for UntypedAstMapper {
    fn on_loc_annot(&mut self, loc: &ALoc) -> Result<ALoc, !> {
        Ok(loc.dupe())
    }

    fn on_type_annot(&mut self, annot: &(ALoc, Type)) -> Result<ALoc, !> {
        let (loc, _) = annot;
        Ok(loc.dupe())
    }
}
