/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use dupe::Dupe;
use flow_parser::ast;
use flow_parser::ast::expression::ExpressionInner;
use flow_parser::ast::types::TypeInner;
use flow_parser::ast_visitor::AstVisitor;
use flow_parser::ast_visitor::TypeParamsContext;
use flow_parser::ast_visitor::map_expression_default;
use flow_parser::ast_visitor::map_program_default;
use flow_parser::ast_visitor::map_statement_default;
use flow_parser::ast_visitor::map_type_default;
use flow_parser::ast_visitor::map_type_param_default;
use flow_parser::ast_visitor::map_variance_default;
use flow_parser::loc::LOC_NONE;
use flow_parser::loc::Loc;
use flow_parser_utils::ast_builder;

use crate::contains_mapper::ContainsMapper;

#[derive(PartialEq, Eq)]
enum Kind {
    NeverType,
    ReadOnlyArrayType,
    ReadOnlyTupleType,
    TypeParamColon,
    InOutVariance,
    PlusSigilToReadonly,
    PlusSigilToOut,
    MinusSigilToWriteonly,
    MinusSigilToIn,
    AsExpression,
    SatisfiesExpression,
}

struct Mapper {
    contains: ContainsMapper,
    kind: Kind,
}

impl AstVisitor<'_, Loc> for Mapper {
    fn normalize_loc(loc: &Loc) -> &Loc {
        loc
    }

    fn normalize_type(type_: &Loc) -> &Loc {
        type_
    }

    fn map_type_(&mut self, t: &ast::types::Type<Loc, Loc>) -> ast::types::Type<Loc, Loc> {
        match &**t {
            TypeInner::Never { loc, comments }
                if self.kind == Kind::NeverType && self.contains.is_target(loc) =>
            {
                ast_builder::types::empty(None, comments.clone())
            }
            TypeInner::ReadOnly { loc, inner }
                if self.kind == Kind::ReadOnlyArrayType && self.contains.is_target(loc) =>
            {
                if let TypeInner::Array {
                    loc: _,
                    inner: array_inner,
                } = &*inner.argument
                {
                    let targs = ast_builder::types::type_args(
                        None,
                        None,
                        vec![map_type_default(self, &array_inner.argument)],
                    );
                    ast_builder::types::unqualified_generic(
                        inner.comments.clone(),
                        None,
                        Some(targs),
                        "ReadonlyArray",
                    )
                } else {
                    map_type_default(self, t)
                }
            }
            TypeInner::ReadOnly { loc, inner }
                if self.kind == Kind::ReadOnlyTupleType && self.contains.is_target(loc) =>
            {
                if let TypeInner::Tuple { .. } = &*inner.argument {
                    let targs = ast_builder::types::type_args(
                        None,
                        None,
                        vec![map_type_default(self, &inner.argument)],
                    );
                    ast_builder::types::unqualified_generic(
                        inner.comments.clone(),
                        None,
                        Some(targs),
                        "Readonly",
                    )
                } else {
                    map_type_default(self, t)
                }
            }
            _ => map_type_default(self, t),
        }
    }

    fn map_type_param(
        &mut self,
        k: &TypeParamsContext,
        tparam: &ast::types::TypeParam<Loc, Loc>,
    ) -> ast::types::TypeParam<Loc, Loc> {
        use ast::types::type_param::BoundKind;
        match tparam {
            ast::types::TypeParam {
                bound_kind: BoundKind::Colon,
                loc,
                ..
            } if self.kind == Kind::TypeParamColon && self.contains.is_target(loc) => {
                ast::types::TypeParam {
                    loc: LOC_NONE,
                    bound_kind: BoundKind::Extends,
                    ..tparam.clone()
                }
            }
            ast::types::TypeParam {
                variance:
                    Some(ast::Variance {
                        loc: v_loc,
                        kind: ast::VarianceKind::InOut,
                        ..
                    }),
                ..
            } if self.kind == Kind::InOutVariance && self.contains.is_target(v_loc) => {
                ast::types::TypeParam {
                    loc: LOC_NONE,
                    variance: None,
                    ..tparam.clone()
                }
            }
            _ => map_type_param_default(self, k, tparam),
        }
    }

    fn map_variance(&mut self, variance: &ast::Variance<Loc>) -> ast::Variance<Loc> {
        let ast::Variance {
            loc,
            kind: variance_kind,
            comments,
        } = variance;
        match variance_kind {
            ast::VarianceKind::Plus
                if self.kind == Kind::PlusSigilToReadonly && self.contains.is_target(loc) =>
            {
                ast::Variance {
                    loc: LOC_NONE,
                    kind: ast::VarianceKind::Readonly,
                    comments: comments.clone(),
                }
            }
            ast::VarianceKind::Plus
                if self.kind == Kind::PlusSigilToOut && self.contains.is_target(loc) =>
            {
                ast::Variance {
                    loc: LOC_NONE,
                    kind: ast::VarianceKind::Out,
                    comments: comments.clone(),
                }
            }
            ast::VarianceKind::Minus
                if self.kind == Kind::MinusSigilToWriteonly && self.contains.is_target(loc) =>
            {
                ast::Variance {
                    loc: LOC_NONE,
                    kind: ast::VarianceKind::Writeonly,
                    comments: comments.clone(),
                }
            }
            ast::VarianceKind::Minus
                if self.kind == Kind::MinusSigilToIn && self.contains.is_target(loc) =>
            {
                ast::Variance {
                    loc: LOC_NONE,
                    kind: ast::VarianceKind::In,
                    comments: comments.clone(),
                }
            }
            _ => map_variance_default(self, variance),
        }
    }

    fn map_expression(
        &mut self,
        expr: &ast::expression::Expression<Loc, Loc>,
    ) -> ast::expression::Expression<Loc, Loc> {
        match &**expr {
            ExpressionInner::AsExpression { loc: _, inner }
                if self.kind == Kind::AsExpression && self.contains.is_target(expr.loc()) =>
            {
                let expression = map_expression_default(self, &inner.expression);
                let annot = map_type_default(self, &inner.annot.annotation);
                ast_builder::expressions::as_expression(
                    None,
                    inner.comments.clone(),
                    expression,
                    annot,
                )
            }
            ExpressionInner::TSSatisfies { loc: _, inner }
                if self.kind == Kind::SatisfiesExpression
                    && self.contains.is_target(expr.loc()) =>
            {
                let expression = map_expression_default(self, &inner.expression);
                let mapped_annot = self.map_type_annotation(&inner.annot);
                let annot = mapped_annot.annotation;
                ast_builder::expressions::as_expression(
                    None,
                    inner.comments.clone(),
                    expression,
                    annot,
                )
            }
            _ => {
                if self.contains.should_map_expression(expr) {
                    map_expression_default(self, expr)
                } else {
                    expr.dupe()
                }
            }
        }
    }

    fn map_program(&mut self, program: &ast::Program<Loc, Loc>) -> ast::Program<Loc, Loc> {
        if self.contains.should_map_program(program) {
            map_program_default(self, program)
        } else {
            program.clone()
        }
    }

    fn map_statement(
        &mut self,
        stmt: &ast::statement::Statement<Loc, Loc>,
    ) -> ast::statement::Statement<Loc, Loc> {
        if self.contains.should_map_statement(stmt) {
            map_statement_default(self, stmt)
        } else {
            stmt.dupe()
        }
    }
}

pub fn convert_never_type(ast: &ast::Program<Loc, Loc>, loc: Loc) -> ast::Program<Loc, Loc> {
    let mut mapper = Mapper {
        contains: ContainsMapper::new(loc),
        kind: Kind::NeverType,
    };
    mapper.map_program(ast)
}

pub fn convert_type_param_colon(ast: &ast::Program<Loc, Loc>, loc: Loc) -> ast::Program<Loc, Loc> {
    let mut mapper = Mapper {
        contains: ContainsMapper::new(loc),
        kind: Kind::TypeParamColon,
    };
    mapper.map_program(ast)
}

pub fn remove_in_out_variance(ast: &ast::Program<Loc, Loc>, loc: Loc) -> ast::Program<Loc, Loc> {
    let mut mapper = Mapper {
        contains: ContainsMapper::new(loc),
        kind: Kind::InOutVariance,
    };
    mapper.map_program(ast)
}

pub fn convert_plus_sigil_to_readonly(
    ast: &ast::Program<Loc, Loc>,
    loc: Loc,
) -> ast::Program<Loc, Loc> {
    let mut mapper = Mapper {
        contains: ContainsMapper::new(loc),
        kind: Kind::PlusSigilToReadonly,
    };
    mapper.map_program(ast)
}

pub fn convert_plus_sigil_to_out(ast: &ast::Program<Loc, Loc>, loc: Loc) -> ast::Program<Loc, Loc> {
    let mut mapper = Mapper {
        contains: ContainsMapper::new(loc),
        kind: Kind::PlusSigilToOut,
    };
    mapper.map_program(ast)
}

pub fn convert_minus_sigil_to_writeonly(
    ast: &ast::Program<Loc, Loc>,
    loc: Loc,
) -> ast::Program<Loc, Loc> {
    let mut mapper = Mapper {
        contains: ContainsMapper::new(loc),
        kind: Kind::MinusSigilToWriteonly,
    };
    mapper.map_program(ast)
}

pub fn convert_minus_sigil_to_in(ast: &ast::Program<Loc, Loc>, loc: Loc) -> ast::Program<Loc, Loc> {
    let mut mapper = Mapper {
        contains: ContainsMapper::new(loc),
        kind: Kind::MinusSigilToIn,
    };
    mapper.map_program(ast)
}

pub fn convert_as_expression(ast: &ast::Program<Loc, Loc>, loc: Loc) -> ast::Program<Loc, Loc> {
    let mut mapper = Mapper {
        contains: ContainsMapper::new(loc),
        kind: Kind::AsExpression,
    };
    mapper.map_program(ast)
}

pub fn convert_satisfies_expression(
    ast: &ast::Program<Loc, Loc>,
    loc: Loc,
) -> ast::Program<Loc, Loc> {
    let mut mapper = Mapper {
        contains: ContainsMapper::new(loc),
        kind: Kind::SatisfiesExpression,
    };
    mapper.map_program(ast)
}

pub fn convert_readonly_array_type(
    ast: &ast::Program<Loc, Loc>,
    loc: Loc,
) -> ast::Program<Loc, Loc> {
    let mut mapper = Mapper {
        contains: ContainsMapper::new(loc),
        kind: Kind::ReadOnlyArrayType,
    };
    mapper.map_program(ast)
}

pub fn convert_readonly_tuple_type(
    ast: &ast::Program<Loc, Loc>,
    loc: Loc,
) -> ast::Program<Loc, Loc> {
    let mut mapper = Mapper {
        contains: ContainsMapper::new(loc),
        kind: Kind::ReadOnlyTupleType,
    };
    mapper.map_program(ast)
}
