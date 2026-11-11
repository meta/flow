/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::ops::Deref;
use std::sync::Arc;

use dupe::Dupe;

use crate::ast;

pub trait LocMapper<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E = !> {
    fn on_loc_annot(&mut self, loc: &M) -> Result<N, E>;
    fn on_type_annot(&mut self, annot: &T) -> Result<U, E>;

    /// Maps the inferred props of a JSX opening element. The props share the opening
    /// element's loc, which is the element's own loc when it is self-closing, so mappers
    /// that collect the type at each loc should skip them.
    fn on_jsx_props_annot(&mut self, props: &T) -> Result<U, E> {
        self.on_type_annot(props)
    }
}

fn try_map_vec<T, U, E>(items: &[T], mut f: impl FnMut(&T) -> Result<U, E>) -> Result<Vec<U>, E> {
    let mut result = Vec::with_capacity(items.len());
    for item in items {
        result.push(f(item)?);
    }
    Ok(result)
}

pub fn program<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    program: &ast::Program<M, T>,
) -> Result<ast::Program<N, U>, E> {
    let ast::Program {
        loc,
        statements,
        interpreter,
        comments,
        all_comments,
    } = program;
    let annot_ = mapper.on_loc_annot(loc)?;
    let statements_ = toplevel_statement_list(mapper, statements)?;
    let interpreter_ = interpreter
        .as_ref()
        .map(|i| interpreter_directive(mapper, i))
        .transpose()?;
    let comments_ = syntax_opt(mapper, comments.as_ref())?;
    let all_comments_ = try_map_vec(all_comments, |c| comment(mapper, c))?.into();
    Ok(ast::Program {
        loc: annot_,
        statements: statements_.into(),
        interpreter: interpreter_,
        comments: comments_,
        all_comments: all_comments_,
    })
}

pub fn interpreter_directive<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    directive: &(M, String),
) -> Result<(N, String), E> {
    let (annot, value) = directive;
    Ok((mapper.on_loc_annot(annot)?, value.clone()))
}

pub fn comment<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    c: &ast::Comment<M>,
) -> Result<ast::Comment<N>, E> {
    let ast::Comment {
        loc,
        kind,
        text,
        on_newline,
    } = c;
    Ok(ast::Comment {
        loc: mapper.on_loc_annot(loc)?,
        kind: *kind,
        text: text.clone(),
        on_newline: *on_newline,
    })
}

pub fn syntax<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E, Internal: Dupe>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    attached: &ast::Syntax<M, Internal>,
) -> Result<ast::Syntax<N, Internal>, E> {
    let ast::Syntax {
        leading,
        trailing,
        internal,
    } = attached;
    let leading_ = try_map_vec(leading, |c| comment(mapper, c))?.into();
    let trailing_ = try_map_vec(trailing, |c| comment(mapper, c))?.into();
    Ok(ast::Syntax {
        leading: leading_,
        trailing: trailing_,
        internal: internal.dupe(),
    })
}

pub fn syntax_opt<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E, Internal: Dupe>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    attached: Option<&ast::Syntax<M, Internal>>,
) -> Result<Option<ast::Syntax<N, Internal>>, E> {
    attached.map(|s| syntax(mapper, s)).transpose()
}

pub fn syntax_with_internal<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    comments: &ast::Syntax<M, Arc<[ast::Comment<M>]>>,
) -> Result<ast::Syntax<N, Arc<[ast::Comment<N>]>>, E> {
    let ast::Syntax {
        leading,
        trailing,
        internal,
    } = comments;
    let leading_: Arc<[ast::Comment<N>]> = try_map_vec(leading, |c| comment(mapper, c))?.into();
    let trailing_: Arc<[ast::Comment<N>]> = try_map_vec(trailing, |c| comment(mapper, c))?.into();
    let internal_: Arc<[ast::Comment<N>]> = try_map_vec(internal, |c| comment(mapper, c))?.into();
    Ok(ast::Syntax {
        leading: leading_,
        trailing: trailing_,
        internal: internal_,
    })
}

pub fn syntax_with_internal_opt<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    comments: Option<&ast::Syntax<M, Arc<[ast::Comment<M>]>>>,
) -> Result<Option<ast::Syntax<N, Arc<[ast::Comment<N>]>>>, E> {
    comments
        .map(|c| syntax_with_internal(mapper, c))
        .transpose()
}

pub fn toplevel_statement_list<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    stmts: &[ast::statement::Statement<M, T>],
) -> Result<Vec<ast::statement::Statement<N, U>>, E> {
    statement_list(mapper, stmts)
}

pub fn statement_list<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    stmts: &[ast::statement::Statement<M, T>],
) -> Result<Vec<ast::statement::Statement<N, U>>, E> {
    let mut result = Vec::with_capacity(stmts.len());
    for stmt in stmts {
        result.extend(statement_fork_point(mapper, stmt)?);
    }
    Ok(result)
}

pub fn statement_fork_point<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    stmt: &ast::statement::Statement<M, T>,
) -> Result<Vec<ast::statement::Statement<N, U>>, E> {
    Ok(vec![statement(mapper, stmt)?])
}

pub fn statement<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    stmt: &ast::statement::Statement<M, T>,
) -> Result<ast::statement::Statement<N, U>, E> {
    use ast::statement::StatementInner;
    Ok(match stmt.deref() {
        StatementInner::Block { loc, inner } => {
            ast::statement::Statement::new(StatementInner::Block {
                loc: mapper.on_loc_annot(loc)?,
                inner: Arc::new(block(mapper, inner)?),
            })
        }
        StatementInner::Break { loc, inner } => {
            ast::statement::Statement::new(StatementInner::Break {
                loc: mapper.on_loc_annot(loc)?,
                inner: Arc::new(break_(mapper, inner)?),
            })
        }
        StatementInner::ClassDeclaration { loc, inner } => {
            ast::statement::Statement::new(StatementInner::ClassDeclaration {
                loc: mapper.on_loc_annot(loc)?,
                inner: Arc::new(class_declaration(mapper, inner)?),
            })
        }
        StatementInner::ComponentDeclaration { loc, inner } => {
            ast::statement::Statement::new(StatementInner::ComponentDeclaration {
                loc: mapper.on_loc_annot(loc)?,
                inner: Arc::new(component_declaration(mapper, inner)?),
            })
        }
        StatementInner::Continue { loc, inner } => {
            ast::statement::Statement::new(StatementInner::Continue {
                loc: mapper.on_loc_annot(loc)?,
                inner: Arc::new(continue_(mapper, inner)?),
            })
        }
        StatementInner::Debugger { loc, inner } => {
            ast::statement::Statement::new(StatementInner::Debugger {
                loc: mapper.on_loc_annot(loc)?,
                inner: Arc::new(debugger(mapper, inner)?),
            })
        }
        StatementInner::DeclareClass { loc, inner } => {
            ast::statement::Statement::new(StatementInner::DeclareClass {
                loc: mapper.on_loc_annot(loc)?,
                inner: Arc::new(declare_class(mapper, inner)?),
            })
        }
        StatementInner::DeclareComponent { loc, inner } => {
            ast::statement::Statement::new(StatementInner::DeclareComponent {
                loc: mapper.on_loc_annot(loc)?,
                inner: Arc::new(declare_component(mapper, inner)?),
            })
        }
        StatementInner::DeclareEnum { loc, inner } => {
            ast::statement::Statement::new(StatementInner::DeclareEnum {
                loc: mapper.on_loc_annot(loc)?,
                inner: Arc::new(declare_enum(mapper, inner)?),
            })
        }
        StatementInner::DeclareExportDeclaration { loc, inner } => {
            ast::statement::Statement::new(StatementInner::DeclareExportDeclaration {
                loc: mapper.on_loc_annot(loc)?,
                inner: Arc::new(declare_export_declaration(mapper, loc, inner)?),
            })
        }
        StatementInner::DeclareFunction { loc, inner } => {
            ast::statement::Statement::new(StatementInner::DeclareFunction {
                loc: mapper.on_loc_annot(loc)?,
                inner: Arc::new(declare_function(mapper, inner)?),
            })
        }
        StatementInner::DeclareInterface { loc, inner } => {
            ast::statement::Statement::new(StatementInner::DeclareInterface {
                loc: mapper.on_loc_annot(loc)?,
                inner: Arc::new(declare_interface(mapper, loc, inner)?),
            })
        }
        StatementInner::DeclareModule { loc, inner } => {
            ast::statement::Statement::new(StatementInner::DeclareModule {
                loc: mapper.on_loc_annot(loc)?,
                inner: Arc::new(declare_module(mapper, loc, inner)?),
            })
        }
        StatementInner::DeclareModuleExports { loc, inner } => {
            ast::statement::Statement::new(StatementInner::DeclareModuleExports {
                loc: mapper.on_loc_annot(loc)?,
                inner: Arc::new(declare_module_exports(mapper, inner)?),
            })
        }
        StatementInner::DeclareNamespace { loc, inner } => {
            ast::statement::Statement::new(StatementInner::DeclareNamespace {
                loc: mapper.on_loc_annot(loc)?,
                inner: Arc::new(declare_namespace(mapper, loc, inner)?),
            })
        }
        StatementInner::DeclareTypeAlias { loc, inner } => {
            ast::statement::Statement::new(StatementInner::DeclareTypeAlias {
                loc: mapper.on_loc_annot(loc)?,
                inner: Arc::new(declare_type_alias(mapper, loc, inner)?),
            })
        }
        StatementInner::DeclareOpaqueType { loc, inner } => {
            ast::statement::Statement::new(StatementInner::DeclareOpaqueType {
                loc: mapper.on_loc_annot(loc)?,
                inner: Arc::new(declare_opaque_type(mapper, loc, inner)?),
            })
        }
        StatementInner::DeclareVariable { loc, inner } => {
            ast::statement::Statement::new(StatementInner::DeclareVariable {
                loc: mapper.on_loc_annot(loc)?,
                inner: Arc::new(declare_variable(mapper, inner)?),
            })
        }
        StatementInner::DoWhile { loc, inner } => {
            ast::statement::Statement::new(StatementInner::DoWhile {
                loc: mapper.on_loc_annot(loc)?,
                inner: Arc::new(do_while(mapper, inner)?),
            })
        }
        StatementInner::Empty { loc, inner } => {
            ast::statement::Statement::new(StatementInner::Empty {
                loc: mapper.on_loc_annot(loc)?,
                inner: Arc::new(empty(mapper, inner)?),
            })
        }
        StatementInner::EnumDeclaration { loc, inner } => {
            ast::statement::Statement::new(StatementInner::EnumDeclaration {
                loc: mapper.on_loc_annot(loc)?,
                inner: Arc::new(enum_declaration(mapper, inner)?),
            })
        }
        StatementInner::ExportDefaultDeclaration { loc, inner } => {
            ast::statement::Statement::new(StatementInner::ExportDefaultDeclaration {
                loc: mapper.on_loc_annot(loc)?,
                inner: Arc::new(export_default_declaration(mapper, loc, inner)?),
            })
        }
        StatementInner::ExportNamedDeclaration { loc, inner } => {
            ast::statement::Statement::new(StatementInner::ExportNamedDeclaration {
                loc: mapper.on_loc_annot(loc)?,
                inner: Arc::new(export_named_declaration(mapper, loc, inner)?),
            })
        }
        StatementInner::ExportAssignment { loc, inner } => {
            ast::statement::Statement::new(StatementInner::ExportAssignment {
                loc: mapper.on_loc_annot(loc)?,
                inner: Arc::new(export_assignment(mapper, inner)?),
            })
        }
        StatementInner::NamespaceExportDeclaration { loc, inner } => {
            ast::statement::Statement::new(StatementInner::NamespaceExportDeclaration {
                loc: mapper.on_loc_annot(loc)?,
                inner: Arc::new(namespace_export_declaration(mapper, inner)?),
            })
        }
        StatementInner::Expression { loc, inner } => {
            ast::statement::Statement::new(StatementInner::Expression {
                loc: mapper.on_loc_annot(loc)?,
                inner: Arc::new(expression_statement(mapper, inner)?),
            })
        }
        StatementInner::For { loc, inner } => ast::statement::Statement::new(StatementInner::For {
            loc: mapper.on_loc_annot(loc)?,
            inner: Arc::new(for_statement(mapper, inner)?),
        }),
        StatementInner::ForIn { loc, inner } => {
            ast::statement::Statement::new(StatementInner::ForIn {
                loc: mapper.on_loc_annot(loc)?,
                inner: Arc::new(for_in_statement(mapper, inner)?),
            })
        }
        StatementInner::ForOf { loc, inner } => {
            ast::statement::Statement::new(StatementInner::ForOf {
                loc: mapper.on_loc_annot(loc)?,
                inner: Arc::new(for_of_statement(mapper, inner)?),
            })
        }
        StatementInner::FunctionDeclaration { loc, inner } => {
            ast::statement::Statement::new(StatementInner::FunctionDeclaration {
                loc: mapper.on_loc_annot(loc)?,
                inner: Arc::new(function_declaration(mapper, inner)?),
            })
        }
        StatementInner::If { loc, inner } => ast::statement::Statement::new(StatementInner::If {
            loc: mapper.on_loc_annot(loc)?,
            inner: Arc::new(if_statement(mapper, inner)?),
        }),
        StatementInner::ImportDeclaration { loc, inner } => {
            ast::statement::Statement::new(StatementInner::ImportDeclaration {
                loc: mapper.on_loc_annot(loc)?,
                inner: Arc::new(import_declaration(mapper, loc, inner)?),
            })
        }
        StatementInner::ImportEqualsDeclaration { loc, inner } => {
            ast::statement::Statement::new(StatementInner::ImportEqualsDeclaration {
                loc: mapper.on_loc_annot(loc)?,
                inner: Arc::new(import_equals_declaration(mapper, inner)?),
            })
        }
        StatementInner::InterfaceDeclaration { loc, inner } => {
            ast::statement::Statement::new(StatementInner::InterfaceDeclaration {
                loc: mapper.on_loc_annot(loc)?,
                inner: Arc::new(interface_declaration(mapper, loc, inner)?),
            })
        }
        StatementInner::Labeled { loc, inner } => {
            ast::statement::Statement::new(StatementInner::Labeled {
                loc: mapper.on_loc_annot(loc)?,
                inner: Arc::new(labeled_statement(mapper, inner)?),
            })
        }
        StatementInner::Match { loc, inner } => {
            ast::statement::Statement::new(StatementInner::Match {
                loc: mapper.on_loc_annot(loc)?,
                inner: Arc::new(match_statement(mapper, inner)?),
            })
        }
        StatementInner::RecordDeclaration { loc, inner } => {
            ast::statement::Statement::new(StatementInner::RecordDeclaration {
                loc: mapper.on_loc_annot(loc)?,
                inner: Arc::new(record_declaration(mapper, inner)?),
            })
        }
        StatementInner::Return { loc, inner } => {
            ast::statement::Statement::new(StatementInner::Return {
                loc: mapper.on_loc_annot(loc)?,
                inner: Arc::new(return_(mapper, inner)?),
            })
        }
        StatementInner::Switch { loc, inner } => {
            ast::statement::Statement::new(StatementInner::Switch {
                loc: mapper.on_loc_annot(loc)?,
                inner: Arc::new(switch(mapper, inner)?),
            })
        }
        StatementInner::Throw { loc, inner } => {
            ast::statement::Statement::new(StatementInner::Throw {
                loc: mapper.on_loc_annot(loc)?,
                inner: Arc::new(throw(mapper, inner)?),
            })
        }
        StatementInner::Try { loc, inner } => ast::statement::Statement::new(StatementInner::Try {
            loc: mapper.on_loc_annot(loc)?,
            inner: Arc::new(try_catch(mapper, inner)?),
        }),
        StatementInner::TypeAlias { loc, inner } => {
            ast::statement::Statement::new(StatementInner::TypeAlias {
                loc: mapper.on_loc_annot(loc)?,
                inner: Arc::new(type_alias(mapper, loc, inner)?),
            })
        }
        StatementInner::OpaqueType { loc, inner } => {
            ast::statement::Statement::new(StatementInner::OpaqueType {
                loc: mapper.on_loc_annot(loc)?,
                inner: Arc::new(opaque_type(mapper, loc, inner)?),
            })
        }
        StatementInner::VariableDeclaration { loc, inner } => {
            ast::statement::Statement::new(StatementInner::VariableDeclaration {
                loc: mapper.on_loc_annot(loc)?,
                inner: Arc::new(variable_declaration(mapper, inner)?),
            })
        }
        StatementInner::While { loc, inner } => {
            ast::statement::Statement::new(StatementInner::While {
                loc: mapper.on_loc_annot(loc)?,
                inner: Arc::new(while_(mapper, inner)?),
            })
        }
        StatementInner::With { loc, inner } => {
            ast::statement::Statement::new(StatementInner::With {
                loc: mapper.on_loc_annot(loc)?,
                inner: Arc::new(with_(mapper, inner)?),
            })
        }
    })
}

pub fn block<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    stmt: &ast::statement::Block<M, T>,
) -> Result<ast::statement::Block<N, U>, E> {
    let ast::statement::Block { body, comments } = stmt;
    let body_ = statement_list(mapper, body)?;
    let comments_ = syntax_with_internal_opt(mapper, comments.as_ref())?;
    Ok(ast::statement::Block {
        body: body_.into(),
        comments: comments_,
    })
}

pub fn break_<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    brk: &ast::statement::Break<M>,
) -> Result<ast::statement::Break<N>, E> {
    let ast::statement::Break { label, comments } = brk;
    let label_ = label
        .as_ref()
        .map(|l| label_identifier(mapper, l))
        .transpose()?;
    let comments_ = syntax_opt(mapper, comments.as_ref())?;
    Ok(ast::statement::Break {
        label: label_,
        comments: comments_,
    })
}

pub fn continue_<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    cont: &ast::statement::Continue<M>,
) -> Result<ast::statement::Continue<N>, E> {
    let ast::statement::Continue { label, comments } = cont;
    let label_ = label
        .as_ref()
        .map(|l| label_identifier(mapper, l))
        .transpose()?;
    let comments_ = syntax_opt(mapper, comments.as_ref())?;
    Ok(ast::statement::Continue {
        label: label_,
        comments: comments_,
    })
}

pub fn debugger<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    dbg: &ast::statement::Debugger<M>,
) -> Result<ast::statement::Debugger<N>, E> {
    let ast::statement::Debugger { comments } = dbg;
    let comments_ = syntax_opt(mapper, comments.as_ref())?;
    Ok(ast::statement::Debugger {
        comments: comments_,
    })
}

pub fn empty<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    e: &ast::statement::Empty<M>,
) -> Result<ast::statement::Empty<N>, E> {
    let ast::statement::Empty { comments } = e;
    Ok(ast::statement::Empty {
        comments: syntax_opt(mapper, comments.as_ref())?,
    })
}

pub fn label_identifier<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    ident: &ast::Identifier<M, M>,
) -> Result<ast::Identifier<N, N>, E> {
    let ast::IdentifierInner {
        loc,
        name,
        comments,
    } = ident.deref();
    let loc_ = mapper.on_loc_annot(loc)?;
    let comments_ = syntax_opt(mapper, comments.as_ref())?;
    Ok(ast::Identifier::new(ast::IdentifierInner {
        loc: loc_,
        name: name.dupe(),
        comments: comments_,
    }))
}

pub fn identifier<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    ident: &ast::Identifier<M, M>,
) -> Result<ast::Identifier<N, N>, E> {
    let ast::IdentifierInner {
        loc,
        name,
        comments,
    } = ident.deref();
    let loc_ = mapper.on_loc_annot(loc)?;
    let comments_ = syntax_opt(mapper, comments.as_ref())?;
    Ok(ast::Identifier::new(ast::IdentifierInner {
        loc: loc_,
        name: name.dupe(),
        comments: comments_,
    }))
}

pub fn t_identifier<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    ident: &ast::Identifier<M, T>,
) -> Result<ast::Identifier<N, U>, E> {
    let ast::IdentifierInner {
        loc,
        name,
        comments,
    } = ident.deref();
    let loc_ = mapper.on_type_annot(loc)?;
    let comments_ = syntax_opt(mapper, comments.as_ref())?;
    Ok(ast::Identifier::new(ast::IdentifierInner {
        loc: loc_,
        name: name.dupe(),
        comments: comments_,
    }))
}

pub fn private_name<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    name: &ast::PrivateName<M>,
) -> Result<ast::PrivateName<N>, E> {
    let ast::PrivateName {
        loc,
        name: name_,
        comments,
    } = name;
    let loc_ = mapper.on_loc_annot(loc)?;
    let comments_ = syntax_opt(mapper, comments.as_ref())?;
    Ok(ast::PrivateName {
        loc: loc_,
        name: name_.dupe(),
        comments: comments_,
    })
}

pub fn computed_key<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    key: &ast::ComputedKey<M, T>,
) -> Result<ast::ComputedKey<N, U>, E> {
    let ast::ComputedKey {
        loc,
        expression: expr,
        comments,
    } = key;
    let loc_ = mapper.on_loc_annot(loc)?;
    let expression_ = expression(mapper, expr)?;
    let comments_ = syntax_opt(mapper, comments.as_ref())?;
    Ok(ast::ComputedKey {
        loc: loc_,
        expression: expression_,
        comments: comments_,
    })
}

pub fn type_identifier<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    ident: &ast::Identifier<M, T>,
) -> Result<ast::Identifier<N, U>, E> {
    t_identifier(mapper, ident)
}

pub fn type_identifier_reference<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    ident: &ast::Identifier<M, T>,
) -> Result<ast::Identifier<N, U>, E> {
    type_identifier(mapper, ident)
}

pub fn binding_type_identifier<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    ident: &ast::Identifier<M, T>,
) -> Result<ast::Identifier<N, U>, E> {
    type_identifier(mapper, ident)
}

pub fn pattern_identifier<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    _kind: ast::VariableKind,
    ident: &ast::Identifier<M, T>,
) -> Result<ast::Identifier<N, U>, E> {
    let ast::IdentifierInner {
        loc,
        name,
        comments,
    } = ident.deref();
    let loc_ = mapper.on_type_annot(loc)?;
    let comments_ = syntax_opt(mapper, comments.as_ref())?;
    Ok(ast::Identifier::new(ast::IdentifierInner {
        loc: loc_,
        name: name.dupe(),
        comments: comments_,
    }))
}

pub fn function_identifier<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    ident: &ast::Identifier<M, T>,
) -> Result<ast::Identifier<N, U>, E> {
    pattern_identifier(mapper, ast::VariableKind::Var, ident)
}

pub fn expression<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    expr: &ast::expression::Expression<M, T>,
) -> Result<ast::expression::Expression<N, U>, E> {
    use ast::expression::Expression;
    use ast::expression::ExpressionInner;
    Ok(match expr.deref() {
        ExpressionInner::Array { loc, inner } => Expression::new(ExpressionInner::Array {
            loc: mapper.on_type_annot(loc)?,
            inner: Arc::new(array(mapper, inner)?),
        }),
        ExpressionInner::ArrowFunction { loc, inner } => {
            Expression::new(ExpressionInner::ArrowFunction {
                loc: mapper.on_type_annot(loc)?,
                inner: Arc::new(arrow_function(mapper, inner)?),
            })
        }
        ExpressionInner::AsConstExpression { loc, inner } => {
            Expression::new(ExpressionInner::AsConstExpression {
                loc: mapper.on_type_annot(loc)?,
                inner: Arc::new(as_const_expression(mapper, inner)?),
            })
        }
        ExpressionInner::AsExpression { loc, inner } => {
            Expression::new(ExpressionInner::AsExpression {
                loc: mapper.on_type_annot(loc)?,
                inner: Arc::new(as_expression(mapper, inner)?),
            })
        }
        ExpressionInner::Assignment { loc, inner } => {
            Expression::new(ExpressionInner::Assignment {
                loc: mapper.on_type_annot(loc)?,
                inner: Arc::new(assignment(mapper, inner)?),
            })
        }
        ExpressionInner::Binary { loc, inner } => Expression::new(ExpressionInner::Binary {
            loc: mapper.on_type_annot(loc)?,
            inner: Arc::new(binary(mapper, inner)?),
        }),
        ExpressionInner::Call { loc, inner } => Expression::new(ExpressionInner::Call {
            loc: mapper.on_type_annot(loc)?,
            inner: Arc::new(call(mapper, loc, inner)?),
        }),
        ExpressionInner::Class { loc, inner } => Expression::new(ExpressionInner::Class {
            loc: mapper.on_type_annot(loc)?,
            inner: Arc::new(class_expression(mapper, inner)?),
        }),
        ExpressionInner::Conditional { loc, inner } => {
            Expression::new(ExpressionInner::Conditional {
                loc: mapper.on_type_annot(loc)?,
                inner: Arc::new(conditional(mapper, inner)?),
            })
        }
        ExpressionInner::Function { loc, inner } => Expression::new(ExpressionInner::Function {
            loc: mapper.on_type_annot(loc)?,
            inner: Arc::new(function_expression(mapper, inner)?),
        }),
        ExpressionInner::Identifier { loc, inner } => {
            Expression::new(ExpressionInner::Identifier {
                loc: mapper.on_type_annot(loc)?,
                inner: t_identifier(mapper, inner)?,
            })
        }
        ExpressionInner::Import { loc, inner } => Expression::new(ExpressionInner::Import {
            loc: mapper.on_type_annot(loc)?,
            inner: Arc::new(import_expr(mapper, loc, inner)?),
        }),
        ExpressionInner::JSXElement { loc, inner } => {
            Expression::new(ExpressionInner::JSXElement {
                loc: mapper.on_type_annot(loc)?,
                inner: Arc::new(jsx_element(mapper, loc, inner)?),
            })
        }
        ExpressionInner::JSXFragment { loc, inner } => {
            Expression::new(ExpressionInner::JSXFragment {
                loc: mapper.on_type_annot(loc)?,
                inner: Arc::new(jsx_fragment(mapper, inner)?),
            })
        }
        ExpressionInner::StringLiteral { loc, inner } => {
            Expression::new(ExpressionInner::StringLiteral {
                loc: mapper.on_type_annot(loc)?,
                inner: Arc::new(string_literal(mapper, inner)?),
            })
        }
        ExpressionInner::BooleanLiteral { loc, inner } => {
            Expression::new(ExpressionInner::BooleanLiteral {
                loc: mapper.on_type_annot(loc)?,
                inner: Arc::new(boolean_literal(mapper, inner)?),
            })
        }
        ExpressionInner::NullLiteral { loc, inner } => {
            Expression::new(ExpressionInner::NullLiteral {
                loc: mapper.on_type_annot(loc)?,
                inner: Arc::new(syntax_opt(mapper, inner.as_ref().as_ref())?),
            })
        }
        ExpressionInner::NumberLiteral { loc, inner } => {
            Expression::new(ExpressionInner::NumberLiteral {
                loc: mapper.on_type_annot(loc)?,
                inner: Arc::new(number_literal(mapper, inner)?),
            })
        }
        ExpressionInner::BigIntLiteral { loc, inner } => {
            Expression::new(ExpressionInner::BigIntLiteral {
                loc: mapper.on_type_annot(loc)?,
                inner: Arc::new(bigint_literal(mapper, inner)?),
            })
        }
        ExpressionInner::RegExpLiteral { loc, inner } => {
            Expression::new(ExpressionInner::RegExpLiteral {
                loc: mapper.on_type_annot(loc)?,
                inner: Arc::new(regexp_literal(mapper, inner)?),
            })
        }
        ExpressionInner::ModuleRefLiteral { loc, inner } => {
            Expression::new(ExpressionInner::ModuleRefLiteral {
                loc: mapper.on_type_annot(loc)?,
                inner: Arc::new(module_ref_literal(mapper, inner)?),
            })
        }
        ExpressionInner::Logical { loc, inner } => Expression::new(ExpressionInner::Logical {
            loc: mapper.on_type_annot(loc)?,
            inner: Arc::new(logical(mapper, inner)?),
        }),
        ExpressionInner::Match { loc, inner } => Expression::new(ExpressionInner::Match {
            loc: mapper.on_type_annot(loc)?,
            inner: Arc::new(match_expression(mapper, inner)?),
        }),
        ExpressionInner::Member { loc, inner } => Expression::new(ExpressionInner::Member {
            loc: mapper.on_type_annot(loc)?,
            inner: Arc::new(member(mapper, loc, inner)?),
        }),
        ExpressionInner::MetaProperty { loc, inner } => {
            Expression::new(ExpressionInner::MetaProperty {
                loc: mapper.on_type_annot(loc)?,
                inner: Arc::new(meta_property(mapper, inner)?),
            })
        }
        ExpressionInner::New { loc, inner } => Expression::new(ExpressionInner::New {
            loc: mapper.on_type_annot(loc)?,
            inner: Arc::new(new_(mapper, loc, inner)?),
        }),
        ExpressionInner::Object { loc, inner } => Expression::new(ExpressionInner::Object {
            loc: mapper.on_type_annot(loc)?,
            inner: Arc::new(object_(mapper, inner)?),
        }),
        ExpressionInner::OptionalCall { loc, inner } => {
            Expression::new(ExpressionInner::OptionalCall {
                loc: mapper.on_type_annot(loc)?,
                inner: Arc::new(optional_call(mapper, loc, inner)?),
            })
        }
        ExpressionInner::OptionalMember { loc, inner } => {
            Expression::new(ExpressionInner::OptionalMember {
                loc: mapper.on_type_annot(loc)?,
                inner: Arc::new(optional_member(mapper, loc, inner)?),
            })
        }
        ExpressionInner::Record { loc, inner } => Expression::new(ExpressionInner::Record {
            loc: mapper.on_type_annot(loc)?,
            inner: Arc::new(record(mapper, inner)?),
        }),
        ExpressionInner::Sequence { loc, inner } => Expression::new(ExpressionInner::Sequence {
            loc: mapper.on_type_annot(loc)?,
            inner: Arc::new(sequence(mapper, inner)?),
        }),
        ExpressionInner::Super { loc, inner } => Expression::new(ExpressionInner::Super {
            loc: mapper.on_type_annot(loc)?,
            inner: Arc::new(super_expression(mapper, inner)?),
        }),
        ExpressionInner::TaggedTemplate { loc, inner } => {
            Expression::new(ExpressionInner::TaggedTemplate {
                loc: mapper.on_type_annot(loc)?,
                inner: Arc::new(tagged_template(mapper, inner)?),
            })
        }
        ExpressionInner::TemplateLiteral { loc, inner } => {
            Expression::new(ExpressionInner::TemplateLiteral {
                loc: mapper.on_type_annot(loc)?,
                inner: Arc::new(template_literal(mapper, inner)?),
            })
        }
        ExpressionInner::This { loc, inner } => Expression::new(ExpressionInner::This {
            loc: mapper.on_type_annot(loc)?,
            inner: Arc::new(this_expression(mapper, inner)?),
        }),
        ExpressionInner::TypeCast { loc, inner } => Expression::new(ExpressionInner::TypeCast {
            loc: mapper.on_type_annot(loc)?,
            inner: Arc::new(type_cast(mapper, inner)?),
        }),
        ExpressionInner::TSSatisfies { loc, inner } => {
            Expression::new(ExpressionInner::TSSatisfies {
                loc: mapper.on_type_annot(loc)?,
                inner: Arc::new(ts_satisfies(mapper, inner)?),
            })
        }
        ExpressionInner::Unary { loc, inner } => Expression::new(ExpressionInner::Unary {
            loc: mapper.on_type_annot(loc)?,
            inner: Arc::new(unary_expression(mapper, inner)?),
        }),
        ExpressionInner::Update { loc, inner } => Expression::new(ExpressionInner::Update {
            loc: mapper.on_type_annot(loc)?,
            inner: Arc::new(update_expression(mapper, inner)?),
        }),
        ExpressionInner::Yield { loc, inner } => Expression::new(ExpressionInner::Yield {
            loc: mapper.on_type_annot(loc)?,
            inner: Arc::new(yield_(mapper, inner)?),
        }),
    })
}

pub fn array<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    expr: &ast::expression::Array<M, T>,
) -> Result<ast::expression::Array<N, U>, E> {
    let ast::expression::Array {
        elements,
        trailing_comma,
        comments,
    } = expr;
    let elements_ = try_map_vec(elements, |e| array_element(mapper, e))?.into();
    let comments_ = syntax_with_internal_opt(mapper, comments.as_ref())?;
    Ok(ast::expression::Array {
        elements: elements_,
        trailing_comma: *trailing_comma,
        comments: comments_,
    })
}

pub fn array_element<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    element: &ast::expression::ArrayElement<M, T>,
) -> Result<ast::expression::ArrayElement<N, U>, E> {
    Ok(match element {
        ast::expression::ArrayElement::Hole(loc) => {
            ast::expression::ArrayElement::Hole(mapper.on_loc_annot(loc)?)
        }
        ast::expression::ArrayElement::Expression(expr) => {
            ast::expression::ArrayElement::Expression(expression(mapper, expr)?)
        }
        ast::expression::ArrayElement::Spread(spread) => {
            ast::expression::ArrayElement::Spread(spread_element(mapper, spread)?)
        }
    })
}

pub fn spread_element<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    expr: &ast::expression::SpreadElement<M, T>,
) -> Result<ast::expression::SpreadElement<N, U>, E> {
    let ast::expression::SpreadElement {
        loc,
        argument,
        comments,
    } = expr;
    let loc_ = mapper.on_loc_annot(loc)?;
    let argument_ = expression(mapper, argument)?;
    let comments_ = syntax_opt(mapper, comments.as_ref())?;
    Ok(ast::expression::SpreadElement {
        loc: loc_,
        argument: argument_,
        comments: comments_,
    })
}

pub fn arrow_function<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    expr: &ast::function::Function<M, T>,
) -> Result<ast::function::Function<N, U>, E> {
    function_(mapper, expr)
}

pub fn as_const_expression<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    expr: &ast::expression::AsConstExpression<M, T>,
) -> Result<ast::expression::AsConstExpression<N, U>, E> {
    let ast::expression::AsConstExpression {
        expression: expr_,
        comments,
    } = expr;
    let expression_ = expression(mapper, expr_)?;
    let comments_ = syntax_opt(mapper, comments.as_ref())?;
    Ok(ast::expression::AsConstExpression {
        expression: expression_,
        comments: comments_,
    })
}

pub fn as_expression<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    expr: &ast::expression::AsExpression<M, T>,
) -> Result<ast::expression::AsExpression<N, U>, E> {
    let ast::expression::AsExpression {
        expression: expr_,
        annot,
        comments,
    } = expr;
    let expression_ = expression(mapper, expr_)?;
    let annot_ = type_annotation(mapper, annot)?;
    let comments_ = syntax_opt(mapper, comments.as_ref())?;
    Ok(ast::expression::AsExpression {
        expression: expression_,
        annot: annot_,
        comments: comments_,
    })
}

pub fn assignment<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    expr: &ast::expression::Assignment<M, T>,
) -> Result<ast::expression::Assignment<N, U>, E> {
    let ast::expression::Assignment {
        operator,
        left,
        right,
        comments,
    } = expr;
    let left_ = assignment_pattern(mapper, left)?;
    let right_ = expression(mapper, right)?;
    let comments_ = syntax_opt(mapper, comments.as_ref())?;
    Ok(ast::expression::Assignment {
        operator: *operator,
        left: left_,
        right: right_,
        comments: comments_,
    })
}

pub fn binary<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    expr: &ast::expression::Binary<M, T>,
) -> Result<ast::expression::Binary<N, U>, E> {
    let ast::expression::Binary {
        operator,
        left,
        right,
        comments,
    } = expr;
    let left_ = expression(mapper, left)?;
    let right_ = expression(mapper, right)?;
    let comments_ = syntax_opt(mapper, comments.as_ref())?;
    Ok(ast::expression::Binary {
        operator: *operator,
        left: left_,
        right: right_,
        comments: comments_,
    })
}

pub fn call<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    _annot: &T,
    expr: &ast::expression::Call<M, T>,
) -> Result<ast::expression::Call<N, U>, E> {
    let ast::expression::Call {
        callee,
        targs,
        arguments,
        comments,
    } = expr;
    let callee_ = expression(mapper, callee)?;
    let targs_ = targs
        .as_ref()
        .map(|t| call_type_args(mapper, t))
        .transpose()?;
    let arguments_ = arg_list(mapper, arguments)?;
    let comments_ = syntax_opt(mapper, comments.as_ref())?;
    Ok(ast::expression::Call {
        callee: callee_,
        targs: targs_,
        arguments: arguments_,
        comments: comments_,
    })
}

pub fn call_type_args<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    targs: &ast::expression::CallTypeArgs<M, T>,
) -> Result<ast::expression::CallTypeArgs<N, U>, E> {
    let ast::expression::CallTypeArgs {
        loc,
        arguments,
        comments,
    } = targs;
    let loc_ = mapper.on_type_annot(loc)?;
    let arguments_ = try_map_vec(arguments, |a| call_type_arg(mapper, a))?.into();
    let comments_ = syntax_with_internal_opt(mapper, comments.as_ref())?;
    Ok(ast::expression::CallTypeArgs {
        loc: loc_,
        arguments: arguments_,
        comments: comments_,
    })
}

pub fn call_type_arg<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    arg: &ast::expression::CallTypeArg<M, T>,
) -> Result<ast::expression::CallTypeArg<N, U>, E> {
    Ok(match arg {
        ast::expression::CallTypeArg::Explicit(t) => {
            ast::expression::CallTypeArg::Explicit(type_(mapper, t)?)
        }
        ast::expression::CallTypeArg::Implicit(t) => {
            ast::expression::CallTypeArg::Implicit(implicit(mapper, t)?)
        }
    })
}

pub fn implicit<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    expr: &ast::expression::CallTypeArgImplicit<M, T>,
) -> Result<ast::expression::CallTypeArgImplicit<N, U>, E> {
    let ast::expression::CallTypeArgImplicit { loc, comments } = expr;
    Ok(ast::expression::CallTypeArgImplicit {
        loc: mapper.on_type_annot(loc)?,
        comments: syntax_opt(mapper, comments.as_ref())?,
    })
}

pub fn arg_list<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    args: &ast::expression::ArgList<M, T>,
) -> Result<ast::expression::ArgList<N, U>, E> {
    let ast::expression::ArgList {
        loc,
        arguments,
        comments,
    } = args;
    let loc_ = mapper.on_loc_annot(loc)?;
    let arguments_ = try_map_vec(arguments, |a| expression_or_spread(mapper, a))?;
    let comments_ = syntax_with_internal_opt(mapper, comments.as_ref())?;
    Ok(ast::expression::ArgList {
        loc: loc_,
        arguments: arguments_.into(),
        comments: comments_,
    })
}

pub fn expression_or_spread<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    expr_or_spread: &ast::expression::ExpressionOrSpread<M, T>,
) -> Result<ast::expression::ExpressionOrSpread<N, U>, E> {
    Ok(match expr_or_spread {
        ast::expression::ExpressionOrSpread::Expression(expr) => {
            ast::expression::ExpressionOrSpread::Expression(expression(mapper, expr)?)
        }
        ast::expression::ExpressionOrSpread::Spread(spread) => {
            ast::expression::ExpressionOrSpread::Spread(spread_element(mapper, spread)?)
        }
    })
}

pub fn class_expression<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    cls: &ast::class::Class<M, T>,
) -> Result<ast::class::Class<N, U>, E> {
    class_(mapper, cls)
}

pub fn conditional<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    expr: &ast::expression::Conditional<M, T>,
) -> Result<ast::expression::Conditional<N, U>, E> {
    let ast::expression::Conditional {
        test,
        consequent,
        alternate,
        comments,
    } = expr;
    let test_ = predicate_expression(mapper, test)?;
    let consequent_ = expression(mapper, consequent)?;
    let alternate_ = expression(mapper, alternate)?;
    let comments_ = syntax_opt(mapper, comments.as_ref())?;
    Ok(ast::expression::Conditional {
        test: test_,
        consequent: consequent_,
        alternate: alternate_,
        comments: comments_,
    })
}

pub fn function_expression<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    expr: &ast::function::Function<M, T>,
) -> Result<ast::function::Function<N, U>, E> {
    function_(mapper, expr)
}

pub fn import_expr<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    _annot: &T,
    expr: &ast::expression::Import<M, T>,
) -> Result<ast::expression::Import<N, U>, E> {
    let ast::expression::Import {
        argument,
        options,
        phase,
        comments,
    } = expr;
    let argument_ = expression(mapper, argument)?;
    let options_ = options
        .as_ref()
        .map(|o| expression(mapper, o))
        .transpose()?;
    let comments_ = syntax_opt(mapper, comments.as_ref())?;
    Ok(ast::expression::Import {
        argument: argument_,
        phase: *phase,
        options: options_,
        comments: comments_,
    })
}

pub fn jsx_element<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    _annot: &T,
    expr: &ast::jsx::Element<M, T>,
) -> Result<ast::jsx::Element<N, U>, E> {
    let ast::jsx::Element {
        opening_element,
        closing_element,
        children,
        comments,
    } = expr;
    let opening_element_ = jsx_opening_element(mapper, opening_element)?;
    let closing_element_ = closing_element
        .as_ref()
        .map(|e| jsx_closing_element(mapper, e))
        .transpose()?;
    let children_ = jsx_children(mapper, children)?;
    let comments_ = syntax_opt(mapper, comments.as_ref())?;
    Ok(ast::jsx::Element {
        opening_element: opening_element_,
        closing_element: closing_element_,
        children: children_,
        comments: comments_,
    })
}

pub fn jsx_fragment<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    expr: &ast::jsx::Fragment<M, T>,
) -> Result<ast::jsx::Fragment<N, U>, E> {
    let ast::jsx::Fragment {
        frag_opening_element,
        frag_closing_element,
        frag_children,
        frag_comments,
    } = expr;
    let opening_ = mapper.on_loc_annot(frag_opening_element)?;
    let closing_ = mapper.on_loc_annot(frag_closing_element)?;
    let children_ = jsx_children(mapper, frag_children)?;
    let frag_comments_ = syntax_opt(mapper, frag_comments.as_ref())?;
    Ok(ast::jsx::Fragment {
        frag_opening_element: opening_,
        frag_closing_element: closing_,
        frag_children: children_,
        frag_comments: frag_comments_,
    })
}

pub fn jsx_opening_element<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    elem: &ast::jsx::Opening<M, T>,
) -> Result<ast::jsx::Opening<N, U>, E> {
    let ast::jsx::Opening {
        loc,
        name,
        targs,
        self_closing,
        attributes,
        props,
    } = elem;
    let name_ = jsx_element_name(mapper, name)?;
    let targs_ = targs
        .as_ref()
        .map(|t| call_type_args(mapper, t))
        .transpose()?;
    let attributes_ = try_map_vec(attributes, |a| jsx_opening_attribute(mapper, a))?;
    let props_ = mapper.on_jsx_props_annot(props)?;
    Ok(ast::jsx::Opening {
        loc: mapper.on_loc_annot(loc)?,
        name: name_,
        targs: targs_,
        self_closing: *self_closing,
        attributes: attributes_.into(),
        props: props_,
    })
}

pub fn jsx_closing_element<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    elem: &ast::jsx::Closing<M, T>,
) -> Result<ast::jsx::Closing<N, U>, E> {
    let ast::jsx::Closing { loc, name } = elem;
    let name_ = jsx_element_name(mapper, name)?;
    Ok(ast::jsx::Closing {
        loc: mapper.on_loc_annot(loc)?,
        name: name_,
    })
}

pub fn jsx_opening_attribute<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    attr: &ast::jsx::OpeningAttribute<M, T>,
) -> Result<ast::jsx::OpeningAttribute<N, U>, E> {
    Ok(match attr {
        ast::jsx::OpeningAttribute::Attribute(attr) => {
            ast::jsx::OpeningAttribute::Attribute(jsx_attribute(mapper, attr)?)
        }
        ast::jsx::OpeningAttribute::SpreadAttribute(spread) => {
            ast::jsx::OpeningAttribute::SpreadAttribute(jsx_spread_attribute(mapper, spread)?)
        }
    })
}

pub fn jsx_spread_attribute<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    attr: &ast::jsx::SpreadAttribute<M, T>,
) -> Result<ast::jsx::SpreadAttribute<N, U>, E> {
    let ast::jsx::SpreadAttribute {
        loc,
        argument,
        comments,
    } = attr;
    let argument_ = expression(mapper, argument)?;
    let comments_ = syntax_opt(mapper, comments.as_ref())?;
    Ok(ast::jsx::SpreadAttribute {
        loc: mapper.on_loc_annot(loc)?,
        argument: argument_,
        comments: comments_,
    })
}

pub fn jsx_attribute<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    attr: &ast::jsx::Attribute<M, T>,
) -> Result<ast::jsx::Attribute<N, U>, E> {
    let ast::jsx::Attribute { loc, name, value } = attr;
    let name_ = jsx_attribute_name(mapper, name)?;
    let value_ = value
        .as_ref()
        .map(|v| jsx_attribute_value(mapper, v))
        .transpose()?;
    Ok(ast::jsx::Attribute {
        loc: mapper.on_loc_annot(loc)?,
        name: name_,
        value: value_,
    })
}

pub fn jsx_attribute_name<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    name: &ast::jsx::attribute::Name<M, T>,
) -> Result<ast::jsx::attribute::Name<N, U>, E> {
    Ok(match name {
        ast::jsx::attribute::Name::Identifier(id) => {
            ast::jsx::attribute::Name::Identifier(jsx_attribute_name_identifier(mapper, id)?)
        }
        ast::jsx::attribute::Name::NamespacedName(ns) => {
            ast::jsx::attribute::Name::NamespacedName(jsx_attribute_name_namespaced(mapper, ns)?)
        }
    })
}

pub fn jsx_attribute_name_identifier<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    id: &ast::jsx::Identifier<M, T>,
) -> Result<ast::jsx::Identifier<N, U>, E> {
    jsx_identifier(mapper, id)
}

pub fn jsx_attribute_name_namespaced<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    nname: &ast::jsx::NamespacedName<M, T>,
) -> Result<ast::jsx::NamespacedName<N, U>, E> {
    jsx_namespaced_name(mapper, nname)
}

pub fn jsx_attribute_value<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    value: &ast::jsx::attribute::Value<M, T>,
) -> Result<ast::jsx::attribute::Value<N, U>, E> {
    Ok(match value {
        ast::jsx::attribute::Value::StringLiteral(lit) => {
            ast::jsx::attribute::Value::StringLiteral(jsx_attribute_value_literal(mapper, lit)?)
        }
        ast::jsx::attribute::Value::ExpressionContainer(expr) => {
            ast::jsx::attribute::Value::ExpressionContainer(jsx_attribute_value_expression(
                mapper, expr,
            )?)
        }
    })
}

pub fn jsx_attribute_value_literal<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    lit: &(T, ast::StringLiteral<M>),
) -> Result<(U, ast::StringLiteral<N>), E> {
    let (annot, lit_inner) = lit;
    Ok((
        mapper.on_type_annot(annot)?,
        string_literal(mapper, lit_inner)?,
    ))
}

pub fn jsx_attribute_value_expression<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    expr: &(T, ast::jsx::ExpressionContainer<M, T>),
) -> Result<(U, ast::jsx::ExpressionContainer<N, U>), E> {
    let (annot, expr_inner) = expr;
    Ok((
        mapper.on_type_annot(annot)?,
        jsx_expression(mapper, expr_inner)?,
    ))
}

pub fn jsx_children<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    children: &(M, Vec<ast::jsx::Child<M, T>>),
) -> Result<(N, Vec<ast::jsx::Child<N, U>>), E> {
    let (annot, children_list) = children;
    Ok((
        mapper.on_loc_annot(annot)?,
        try_map_vec(children_list, |c| jsx_child(mapper, c))?,
    ))
}

pub fn jsx_child<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    child: &ast::jsx::Child<M, T>,
) -> Result<ast::jsx::Child<N, U>, E> {
    Ok(match child {
        ast::jsx::Child::Element { loc, inner } => ast::jsx::Child::Element {
            loc: mapper.on_type_annot(loc)?,
            inner: jsx_element(mapper, loc, inner)?,
        },
        ast::jsx::Child::Fragment { loc, inner } => ast::jsx::Child::Fragment {
            loc: mapper.on_type_annot(loc)?,
            inner: jsx_fragment(mapper, inner)?,
        },
        ast::jsx::Child::ExpressionContainer { loc, inner } => {
            ast::jsx::Child::ExpressionContainer {
                loc: mapper.on_type_annot(loc)?,
                inner: jsx_expression(mapper, inner)?,
            }
        }
        ast::jsx::Child::SpreadChild { loc, inner } => ast::jsx::Child::SpreadChild {
            loc: mapper.on_type_annot(loc)?,
            inner: jsx_spread_child(mapper, inner)?,
        },
        ast::jsx::Child::Text { loc, inner } => ast::jsx::Child::Text {
            loc: mapper.on_type_annot(loc)?,
            inner: inner.clone(),
        },
    })
}

pub fn jsx_expression<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    expr: &ast::jsx::ExpressionContainer<M, T>,
) -> Result<ast::jsx::ExpressionContainer<N, U>, E> {
    let ast::jsx::ExpressionContainer {
        expression: expr_,
        comments,
    } = expr;
    let expression_ = match expr_ {
        ast::jsx::expression_container::Expression::Expression(e) => {
            ast::jsx::expression_container::Expression::Expression(expression(mapper, e)?)
        }
        ast::jsx::expression_container::Expression::EmptyExpression => {
            ast::jsx::expression_container::Expression::EmptyExpression
        }
    };
    let comments_ = syntax_with_internal_opt(mapper, comments.as_ref())?;
    Ok(ast::jsx::ExpressionContainer {
        expression: expression_,
        comments: comments_,
    })
}

pub fn jsx_spread_child<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    child: &ast::jsx::SpreadChild<M, T>,
) -> Result<ast::jsx::SpreadChild<N, U>, E> {
    let ast::jsx::SpreadChild {
        expression: expr,
        comments,
    } = child;
    let expression_ = expression(mapper, expr)?;
    let comments_ = syntax_opt(mapper, comments.as_ref())?;
    Ok(ast::jsx::SpreadChild {
        expression: expression_,
        comments: comments_,
    })
}

pub fn jsx_element_name<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    name: &ast::jsx::Name<M, T>,
) -> Result<ast::jsx::Name<N, U>, E> {
    Ok(match name {
        ast::jsx::Name::Identifier(id) => {
            ast::jsx::Name::Identifier(jsx_element_name_identifier(mapper, id)?)
        }
        ast::jsx::Name::NamespacedName(ns) => {
            ast::jsx::Name::NamespacedName(jsx_element_name_namespaced(mapper, ns)?)
        }
        ast::jsx::Name::MemberExpression(expr) => {
            ast::jsx::Name::MemberExpression(jsx_element_name_member_expression(mapper, expr)?)
        }
    })
}

pub fn jsx_element_name_identifier<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    ident: &ast::jsx::Identifier<M, T>,
) -> Result<ast::jsx::Identifier<N, U>, E> {
    jsx_identifier(mapper, ident)
}

pub fn jsx_element_name_namespaced<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    ns: &ast::jsx::NamespacedName<M, T>,
) -> Result<ast::jsx::NamespacedName<N, U>, E> {
    jsx_namespaced_name(mapper, ns)
}

pub fn jsx_element_name_member_expression<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    expr: &ast::jsx::MemberExpression<M, T>,
) -> Result<ast::jsx::MemberExpression<N, U>, E> {
    jsx_member_expression(mapper, expr)
}

pub fn jsx_identifier<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    id: &ast::jsx::Identifier<M, T>,
) -> Result<ast::jsx::Identifier<N, U>, E> {
    let ast::jsx::Identifier {
        loc,
        name,
        comments,
    } = id;
    Ok(ast::jsx::Identifier {
        loc: mapper.on_type_annot(loc)?,
        name: name.dupe(),
        comments: syntax_opt(mapper, comments.as_ref())?,
    })
}

pub fn jsx_namespaced_name<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    ns: &ast::jsx::NamespacedName<M, T>,
) -> Result<ast::jsx::NamespacedName<N, U>, E> {
    let ast::jsx::NamespacedName {
        loc,
        namespace,
        name,
    } = ns;
    let namespace_ = jsx_identifier(mapper, namespace)?;
    let name_ = jsx_identifier(mapper, name)?;
    Ok(ast::jsx::NamespacedName {
        loc: mapper.on_loc_annot(loc)?,
        namespace: namespace_,
        name: name_,
    })
}

pub fn jsx_member_expression<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    expr: &ast::jsx::MemberExpression<M, T>,
) -> Result<ast::jsx::MemberExpression<N, U>, E> {
    let ast::jsx::MemberExpression {
        loc,
        object,
        property,
    } = expr;
    let object_ = jsx_member_expression_object(mapper, object)?;
    let property_ = jsx_member_expression_identifier(mapper, property)?;
    Ok(ast::jsx::MemberExpression {
        loc: mapper.on_loc_annot(loc)?,
        object: object_,
        property: property_,
    })
}

pub fn jsx_member_expression_object<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    obj: &ast::jsx::member_expression::Object<M, T>,
) -> Result<ast::jsx::member_expression::Object<N, U>, E> {
    Ok(match obj {
        ast::jsx::member_expression::Object::Identifier(id) => {
            ast::jsx::member_expression::Object::Identifier(jsx_element_name_identifier(
                mapper, id,
            )?)
        }
        ast::jsx::member_expression::Object::MemberExpression(expr) => {
            ast::jsx::member_expression::Object::MemberExpression(Arc::new(jsx_member_expression(
                mapper, expr,
            )?))
        }
    })
}

pub fn jsx_member_expression_identifier<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    id: &ast::jsx::Identifier<M, T>,
) -> Result<ast::jsx::Identifier<N, U>, E> {
    jsx_element_name_identifier(mapper, id)
}

pub fn type_annotation<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    annot: &ast::types::Annotation<M, T>,
) -> Result<ast::types::Annotation<N, U>, E> {
    let ast::types::Annotation { loc, annotation } = annot;
    Ok(ast::types::Annotation {
        loc: mapper.on_loc_annot(loc)?,
        annotation: type_(mapper, annotation)?,
    })
}

pub fn string_literal<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    lit: &ast::StringLiteral<M>,
) -> Result<ast::StringLiteral<N>, E> {
    let ast::StringLiteral {
        value,
        raw,
        comments,
    } = lit;
    let comments_ = syntax_opt(mapper, comments.as_ref())?;
    Ok(ast::StringLiteral {
        value: value.clone(),
        raw: raw.clone(),
        comments: comments_,
    })
}

pub fn boolean_literal<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    lit: &ast::BooleanLiteral<M>,
) -> Result<ast::BooleanLiteral<N>, E> {
    let ast::BooleanLiteral { value, comments } = lit;
    let comments_ = syntax_opt(mapper, comments.as_ref())?;
    Ok(ast::BooleanLiteral {
        value: *value,
        comments: comments_,
    })
}

pub fn number_literal<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    lit: &ast::NumberLiteral<M>,
) -> Result<ast::NumberLiteral<N>, E> {
    let ast::NumberLiteral {
        value,
        raw,
        comments,
    } = lit;
    let comments_ = syntax_opt(mapper, comments.as_ref())?;
    Ok(ast::NumberLiteral {
        value: *value,
        raw: raw.clone(),
        comments: comments_,
    })
}

pub fn bigint_literal<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    lit: &ast::BigIntLiteral<M>,
) -> Result<ast::BigIntLiteral<N>, E> {
    let ast::BigIntLiteral {
        value,
        raw,
        comments,
    } = lit;
    let comments_ = syntax_opt(mapper, comments.as_ref())?;
    Ok(ast::BigIntLiteral {
        value: *value,
        raw: raw.clone(),
        comments: comments_,
    })
}

pub fn regexp_literal<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    lit: &ast::RegExpLiteral<M>,
) -> Result<ast::RegExpLiteral<N>, E> {
    let ast::RegExpLiteral {
        pattern,
        raw,
        flags,
        comments,
    } = lit;
    let comments_ = syntax_opt(mapper, comments.as_ref())?;
    Ok(ast::RegExpLiteral {
        pattern: pattern.clone(),
        raw: raw.clone(),
        flags: flags.clone(),
        comments: comments_,
    })
}

pub fn module_ref_literal<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    lit: &ast::ModuleRefLiteral<M>,
) -> Result<ast::ModuleRefLiteral<N>, E> {
    let ast::ModuleRefLiteral {
        value,
        require_loc,
        def_loc_opt,
        prefix_len,
        raw,
        comments,
    } = lit;
    let require_loc_ = mapper.on_loc_annot(require_loc)?;
    let def_loc_opt_ = def_loc_opt
        .as_ref()
        .map(|loc| mapper.on_loc_annot(loc))
        .transpose()?;
    let comments_ = syntax_opt(mapper, comments.as_ref())?;
    Ok(ast::ModuleRefLiteral {
        value: value.clone(),
        require_loc: require_loc_,
        def_loc_opt: def_loc_opt_,
        prefix_len: *prefix_len,
        raw: raw.clone(),
        comments: comments_,
    })
}

pub fn logical<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    expr: &ast::expression::Logical<M, T>,
) -> Result<ast::expression::Logical<N, U>, E> {
    let ast::expression::Logical {
        operator,
        left,
        right,
        comments,
    } = expr;
    let left_ = expression(mapper, left)?;
    let right_ = expression(mapper, right)?;
    let comments_ = syntax_opt(mapper, comments.as_ref())?;
    Ok(ast::expression::Logical {
        operator: *operator,
        left: left_,
        right: right_,
        comments: comments_,
    })
}

pub fn match_expression<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    expr: &ast::expression::MatchExpression<M, T>,
) -> Result<ast::expression::MatchExpression<N, U>, E> {
    let ast::match_::Match {
        arg,
        cases,
        match_keyword_loc,
        comments,
    } = expr;
    let arg_ = expression(mapper, arg)?;
    let cases_ = try_map_vec(cases, |case| match_expression_case(mapper, case))?;
    let match_keyword_loc_ = mapper.on_loc_annot(match_keyword_loc)?;
    let comments_ = syntax_opt(mapper, comments.as_ref())?;
    Ok(ast::match_::Match {
        arg: arg_,
        cases: cases_.into(),
        match_keyword_loc: match_keyword_loc_,
        comments: comments_,
    })
}

pub fn match_expression_case<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    case: &ast::match_::Case<M, T, ast::expression::Expression<M, T>>,
) -> Result<ast::match_::Case<N, U, ast::expression::Expression<N, U>>, E> {
    let ast::match_::Case {
        loc,
        pattern,
        body,
        guard,
        comments,
        invalid_syntax,
        case_match_root_loc,
    } = case;
    let loc_ = mapper.on_loc_annot(loc)?;
    let pattern_ = match_pattern(mapper, pattern)?;
    let body_ = expression(mapper, body)?;
    let guard_ = guard.as_ref().map(|g| expression(mapper, g)).transpose()?;
    let comments_ = syntax_opt(mapper, comments.as_ref())?;
    let invalid_syntax_ = match_case_invalid_syntax(mapper, invalid_syntax)?;
    let case_match_root_loc_ = mapper.on_loc_annot(case_match_root_loc)?;
    Ok(ast::match_::Case {
        loc: loc_,
        pattern: pattern_,
        body: body_,
        guard: guard_,
        comments: comments_,
        invalid_syntax: invalid_syntax_,
        case_match_root_loc: case_match_root_loc_,
    })
}

pub fn match_case_invalid_syntax<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    x: &ast::match_::InvalidSyntax<M>,
) -> Result<ast::match_::InvalidSyntax<N>, E> {
    let ast::match_::InvalidSyntax {
        invalid_prefix_case,
        invalid_infix_colon,
        invalid_suffix_semicolon,
    } = x;
    let invalid_prefix_case_ = invalid_prefix_case
        .as_ref()
        .map(|l| mapper.on_loc_annot(l))
        .transpose()?;
    let invalid_infix_colon_ = invalid_infix_colon
        .as_ref()
        .map(|l| mapper.on_loc_annot(l))
        .transpose()?;
    let invalid_suffix_semicolon_ = invalid_suffix_semicolon
        .as_ref()
        .map(|l| mapper.on_loc_annot(l))
        .transpose()?;
    Ok(ast::match_::InvalidSyntax {
        invalid_prefix_case: invalid_prefix_case_,
        invalid_infix_colon: invalid_infix_colon_,
        invalid_suffix_semicolon: invalid_suffix_semicolon_,
    })
}

pub fn match_pattern<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    pattern: &ast::match_pattern::MatchPattern<M, T>,
) -> Result<ast::match_pattern::MatchPattern<N, U>, E> {
    Ok(match pattern {
        ast::match_pattern::MatchPattern::WildcardPattern { loc, inner } => {
            ast::match_pattern::MatchPattern::WildcardPattern {
                loc: mapper.on_loc_annot(loc)?,
                inner: Box::new(match_wildcard_pattern(mapper, inner)?),
            }
        }
        ast::match_pattern::MatchPattern::NumberPattern { loc, inner } => {
            ast::match_pattern::MatchPattern::NumberPattern {
                loc: mapper.on_loc_annot(loc)?,
                inner: Box::new(number_literal(mapper, inner)?),
            }
        }
        ast::match_pattern::MatchPattern::BigIntPattern { loc, inner } => {
            ast::match_pattern::MatchPattern::BigIntPattern {
                loc: mapper.on_loc_annot(loc)?,
                inner: Box::new(bigint_literal(mapper, inner)?),
            }
        }
        ast::match_pattern::MatchPattern::StringPattern { loc, inner } => {
            ast::match_pattern::MatchPattern::StringPattern {
                loc: mapper.on_loc_annot(loc)?,
                inner: Box::new(string_literal(mapper, inner)?),
            }
        }
        ast::match_pattern::MatchPattern::BooleanPattern { loc, inner } => {
            ast::match_pattern::MatchPattern::BooleanPattern {
                loc: mapper.on_loc_annot(loc)?,
                inner: Box::new(boolean_literal(mapper, inner)?),
            }
        }
        ast::match_pattern::MatchPattern::NullPattern { loc, inner } => {
            ast::match_pattern::MatchPattern::NullPattern {
                loc: mapper.on_loc_annot(loc)?,
                inner: Box::new(syntax_opt(mapper, (**inner).as_ref())?),
            }
        }
        ast::match_pattern::MatchPattern::UnaryPattern { loc, inner } => {
            ast::match_pattern::MatchPattern::UnaryPattern {
                loc: mapper.on_loc_annot(loc)?,
                inner: Arc::new(match_unary_pattern(mapper, inner)?),
            }
        }
        ast::match_pattern::MatchPattern::BindingPattern { loc, inner } => {
            ast::match_pattern::MatchPattern::BindingPattern {
                loc: mapper.on_loc_annot(loc)?,
                inner: Arc::new(match_binding_pattern(mapper, inner)?),
            }
        }
        ast::match_pattern::MatchPattern::IdentifierPattern { loc, inner } => {
            ast::match_pattern::MatchPattern::IdentifierPattern {
                loc: mapper.on_loc_annot(loc)?,
                inner: Box::new(t_identifier(mapper, inner)?),
            }
        }
        ast::match_pattern::MatchPattern::MemberPattern { loc, inner } => {
            ast::match_pattern::MatchPattern::MemberPattern {
                loc: mapper.on_loc_annot(loc)?,
                inner: Arc::new(match_member_pattern(mapper, inner)?),
            }
        }
        ast::match_pattern::MatchPattern::ObjectPattern { loc, inner } => {
            ast::match_pattern::MatchPattern::ObjectPattern {
                loc: mapper.on_loc_annot(loc)?,
                inner: Arc::new(match_object_pattern(mapper, inner)?),
            }
        }
        ast::match_pattern::MatchPattern::ArrayPattern { loc, inner } => {
            ast::match_pattern::MatchPattern::ArrayPattern {
                loc: mapper.on_loc_annot(loc)?,
                inner: Arc::new(match_array_pattern(mapper, inner)?),
            }
        }
        ast::match_pattern::MatchPattern::OrPattern { loc, inner } => {
            ast::match_pattern::MatchPattern::OrPattern {
                loc: mapper.on_loc_annot(loc)?,
                inner: Arc::new(match_or_pattern(mapper, inner)?),
            }
        }
        ast::match_pattern::MatchPattern::AsPattern { loc, inner } => {
            ast::match_pattern::MatchPattern::AsPattern {
                loc: mapper.on_loc_annot(loc)?,
                inner: Arc::new(match_as_pattern(mapper, inner)?),
            }
        }
        ast::match_pattern::MatchPattern::InstancePattern { loc, inner } => {
            ast::match_pattern::MatchPattern::InstancePattern {
                loc: mapper.on_loc_annot(loc)?,
                inner: Arc::new(match_instance_pattern(mapper, inner)?),
            }
        }
    })
}

pub fn match_wildcard_pattern<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    x: &ast::match_pattern::WildcardPattern<M>,
) -> Result<ast::match_pattern::WildcardPattern<N>, E> {
    let ast::match_pattern::WildcardPattern {
        comments,
        invalid_syntax_default_keyword,
    } = x;
    let comments_ = syntax_opt(mapper, comments.as_ref())?;
    Ok(ast::match_pattern::WildcardPattern {
        comments: comments_,
        invalid_syntax_default_keyword: *invalid_syntax_default_keyword,
    })
}

pub fn match_unary_pattern<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    x: &ast::match_pattern::UnaryPattern<M>,
) -> Result<ast::match_pattern::UnaryPattern<N>, E> {
    let ast::match_pattern::UnaryPattern {
        operator,
        argument,
        comments,
    } = x;
    let (arg_loc, arg) = argument;
    let argument_ = (
        mapper.on_loc_annot(arg_loc)?,
        match_unary_pattern_argument(mapper, arg)?,
    );
    let comments_ = syntax_opt(mapper, comments.as_ref())?;
    Ok(ast::match_pattern::UnaryPattern {
        operator: operator.clone(),
        argument: argument_,
        comments: comments_,
    })
}

pub fn match_unary_pattern_argument<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    arg: &ast::match_pattern::unary_pattern::Argument<M>,
) -> Result<ast::match_pattern::unary_pattern::Argument<N>, E> {
    Ok(match arg {
        ast::match_pattern::unary_pattern::Argument::NumberLiteral(lit) => {
            ast::match_pattern::unary_pattern::Argument::NumberLiteral(number_literal(mapper, lit)?)
        }
        ast::match_pattern::unary_pattern::Argument::BigIntLiteral(lit) => {
            ast::match_pattern::unary_pattern::Argument::BigIntLiteral(bigint_literal(mapper, lit)?)
        }
    })
}

pub fn match_binding_pattern<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    x: &ast::match_pattern::BindingPattern<M, T>,
) -> Result<ast::match_pattern::BindingPattern<N, U>, E> {
    let ast::match_pattern::BindingPattern { kind, id, comments } = x;
    let id_ = pattern_identifier(mapper, *kind, id)?;
    let comments_ = syntax_opt(mapper, comments.as_ref())?;
    Ok(ast::match_pattern::BindingPattern {
        kind: *kind,
        id: id_,
        comments: comments_,
    })
}

pub fn match_member_pattern<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    x: &ast::match_pattern::MemberPattern<M, T>,
) -> Result<ast::match_pattern::MemberPattern<N, U>, E> {
    let ast::match_pattern::MemberPattern {
        loc,
        base,
        property,
        comments,
    } = x;
    let loc_ = mapper.on_type_annot(loc)?;
    let base_ = match_member_pattern_base(mapper, base)?;
    let property_ = match_member_pattern_property(mapper, property)?;
    let comments_ = syntax_opt(mapper, comments.as_ref())?;
    Ok(ast::match_pattern::MemberPattern {
        loc: loc_,
        base: base_,
        property: property_,
        comments: comments_,
    })
}

pub fn match_member_pattern_base<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    base: &ast::match_pattern::member_pattern::Base<M, T>,
) -> Result<ast::match_pattern::member_pattern::Base<N, U>, E> {
    Ok(match base {
        ast::match_pattern::member_pattern::Base::BaseIdentifier(ident) => {
            ast::match_pattern::member_pattern::Base::BaseIdentifier(t_identifier(mapper, ident)?)
        }
        ast::match_pattern::member_pattern::Base::BaseMember(mem) => {
            ast::match_pattern::member_pattern::Base::BaseMember(Arc::new(match_member_pattern(
                mapper, mem,
            )?))
        }
    })
}

pub fn match_member_pattern_property<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    prop: &ast::match_pattern::member_pattern::Property<M, T>,
) -> Result<ast::match_pattern::member_pattern::Property<N, U>, E> {
    Ok(match prop {
        ast::match_pattern::member_pattern::Property::PropertyString { loc, literal } => {
            ast::match_pattern::member_pattern::Property::PropertyString {
                loc: mapper.on_loc_annot(loc)?,
                literal: string_literal(mapper, literal)?,
            }
        }
        ast::match_pattern::member_pattern::Property::PropertyNumber { loc, literal } => {
            ast::match_pattern::member_pattern::Property::PropertyNumber {
                loc: mapper.on_loc_annot(loc)?,
                literal: number_literal(mapper, literal)?,
            }
        }
        ast::match_pattern::member_pattern::Property::PropertyBigInt { loc, literal } => {
            ast::match_pattern::member_pattern::Property::PropertyBigInt {
                loc: mapper.on_loc_annot(loc)?,
                literal: bigint_literal(mapper, literal)?,
            }
        }
        ast::match_pattern::member_pattern::Property::PropertyIdentifier(ident) => {
            ast::match_pattern::member_pattern::Property::PropertyIdentifier(t_identifier(
                mapper, ident,
            )?)
        }
    })
}

pub fn match_object_pattern<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    x: &ast::match_pattern::ObjectPattern<M, T>,
) -> Result<ast::match_pattern::ObjectPattern<N, U>, E> {
    let ast::match_pattern::ObjectPattern {
        properties,
        rest,
        comments,
    } = x;
    let properties_ = try_map_vec(properties, |p| match_object_pattern_property(mapper, p))?;
    let rest_ = rest
        .as_ref()
        .map(|r| match_rest_pattern(mapper, r))
        .transpose()?;
    let comments_ = syntax_with_internal_opt(mapper, comments.as_ref())?;
    Ok(ast::match_pattern::ObjectPattern {
        properties: properties_.into(),
        rest: rest_,
        comments: comments_,
    })
}

pub fn match_object_pattern_property<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    prop: &ast::match_pattern::object_pattern::Property<M, T>,
) -> Result<ast::match_pattern::object_pattern::Property<N, U>, E> {
    Ok(match prop {
        ast::match_pattern::object_pattern::Property::Valid { loc, property } => {
            let ast::match_pattern::object_pattern::PropertyStruct {
                key,
                pattern,
                shorthand,
                comments,
            } = property;
            let key_ = match_object_pattern_property_key(mapper, key)?;
            let pattern_ = match_pattern(mapper, pattern)?;
            let comments_ = syntax_opt(mapper, comments.as_ref())?;
            ast::match_pattern::object_pattern::Property::Valid {
                loc: mapper.on_loc_annot(loc)?,
                property: ast::match_pattern::object_pattern::PropertyStruct {
                    key: key_,
                    pattern: pattern_,
                    shorthand: *shorthand,
                    comments: comments_,
                },
            }
        }
        ast::match_pattern::object_pattern::Property::InvalidShorthand {
            loc,
            identifier: id,
        } => ast::match_pattern::object_pattern::Property::InvalidShorthand {
            loc: mapper.on_loc_annot(loc)?,
            identifier: identifier(mapper, id)?,
        },
    })
}

pub fn match_object_pattern_property_key<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    key: &ast::match_pattern::object_pattern::Key<M, T>,
) -> Result<ast::match_pattern::object_pattern::Key<N, U>, E> {
    Ok(match key {
        ast::match_pattern::object_pattern::Key::StringLiteral((annot, lit)) => {
            ast::match_pattern::object_pattern::Key::StringLiteral((
                mapper.on_loc_annot(annot)?,
                string_literal(mapper, lit)?,
            ))
        }
        ast::match_pattern::object_pattern::Key::NumberLiteral((annot, lit)) => {
            ast::match_pattern::object_pattern::Key::NumberLiteral((
                mapper.on_loc_annot(annot)?,
                number_literal(mapper, lit)?,
            ))
        }
        ast::match_pattern::object_pattern::Key::BigIntLiteral((annot, lit)) => {
            ast::match_pattern::object_pattern::Key::BigIntLiteral((
                mapper.on_loc_annot(annot)?,
                bigint_literal(mapper, lit)?,
            ))
        }
        ast::match_pattern::object_pattern::Key::Identifier(ident) => {
            ast::match_pattern::object_pattern::Key::Identifier(t_identifier(mapper, ident)?)
        }
    })
}

pub fn match_rest_pattern<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    x: &ast::match_pattern::RestPattern<M, T>,
) -> Result<ast::match_pattern::RestPattern<N, U>, E> {
    let ast::match_pattern::RestPattern {
        loc,
        argument,
        comments,
    } = x;
    let loc_ = mapper.on_loc_annot(loc)?;
    let argument_ = argument
        .as_ref()
        .map(|(arg_loc, binding)| {
            Ok((
                mapper.on_loc_annot(arg_loc)?,
                match_binding_pattern(mapper, binding)?,
            ))
        })
        .transpose()?;

    let comments_ = syntax_opt(mapper, comments.as_ref())?;
    Ok(ast::match_pattern::RestPattern {
        loc: loc_,
        argument: argument_,
        comments: comments_,
    })
}

pub fn match_array_pattern<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    x: &ast::match_pattern::ArrayPattern<M, T>,
) -> Result<ast::match_pattern::ArrayPattern<N, U>, E> {
    let ast::match_pattern::ArrayPattern {
        elements,
        rest,
        comments,
    } = x;
    let elements_ = try_map_vec(elements, |e| match_pattern_array_element(mapper, e))?;
    let rest_ = rest
        .as_ref()
        .map(|r| match_rest_pattern(mapper, r))
        .transpose()?;
    let comments_ = syntax_with_internal_opt(mapper, comments.as_ref())?;
    Ok(ast::match_pattern::ArrayPattern {
        elements: elements_.into(),
        rest: rest_,
        comments: comments_,
    })
}

pub fn match_pattern_array_element<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    elem: &ast::match_pattern::array_pattern::Element<M, T>,
) -> Result<ast::match_pattern::array_pattern::Element<N, U>, E> {
    let ast::match_pattern::array_pattern::Element { index, pattern } = elem;
    let pattern_ = match_pattern(mapper, pattern)?;
    let index_ = mapper.on_loc_annot(index)?;
    Ok(ast::match_pattern::array_pattern::Element {
        index: index_,
        pattern: pattern_,
    })
}

pub fn match_or_pattern<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    x: &ast::match_pattern::OrPattern<M, T>,
) -> Result<ast::match_pattern::OrPattern<N, U>, E> {
    let ast::match_pattern::OrPattern { patterns, comments } = x;
    let patterns_ = try_map_vec(patterns, |p| match_pattern(mapper, p))?.into();
    let comments_ = syntax_opt(mapper, comments.as_ref())?;
    Ok(ast::match_pattern::OrPattern {
        patterns: patterns_,
        comments: comments_,
    })
}

pub fn match_as_pattern<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    x: &ast::match_pattern::AsPattern<M, T>,
) -> Result<ast::match_pattern::AsPattern<N, U>, E> {
    let ast::match_pattern::AsPattern {
        pattern,
        target,
        comments,
    } = x;
    let pattern_ = match_pattern(mapper, pattern)?;
    let target_ = match_as_pattern_target(mapper, target)?;
    let comments_ = syntax_opt(mapper, comments.as_ref())?;
    Ok(ast::match_pattern::AsPattern {
        pattern: pattern_,
        target: target_,
        comments: comments_,
    })
}

pub fn match_as_pattern_target<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    target: &ast::match_pattern::as_pattern::Target<M, T>,
) -> Result<ast::match_pattern::as_pattern::Target<N, U>, E> {
    Ok(match target {
        ast::match_pattern::as_pattern::Target::Binding { loc, pattern } => {
            ast::match_pattern::as_pattern::Target::Binding {
                loc: mapper.on_loc_annot(loc)?,
                pattern: match_binding_pattern(mapper, pattern)?,
            }
        }
        ast::match_pattern::as_pattern::Target::Identifier(id) => {
            ast::match_pattern::as_pattern::Target::Identifier(pattern_identifier(
                mapper,
                ast::VariableKind::Const,
                id,
            )?)
        }
    })
}

pub fn match_instance_pattern<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    x: &ast::match_pattern::InstancePattern<M, T>,
) -> Result<ast::match_pattern::InstancePattern<N, U>, E> {
    let ast::match_pattern::InstancePattern {
        constructor,
        properties: (properties_loc, properties),
        comments,
    } = x;
    let constructor_ = match constructor {
        ast::match_pattern::InstancePatternConstructor::IdentifierConstructor(ident) => {
            ast::match_pattern::InstancePatternConstructor::IdentifierConstructor(t_identifier(
                mapper, ident,
            )?)
        }
        ast::match_pattern::InstancePatternConstructor::MemberConstructor(member) => {
            ast::match_pattern::InstancePatternConstructor::MemberConstructor(match_member_pattern(
                mapper, member,
            )?)
        }
    };
    let properties_ = (
        mapper.on_loc_annot(properties_loc)?,
        match_object_pattern(mapper, properties)?,
    );
    let comments_ = syntax_opt(mapper, comments.as_ref())?;
    Ok(ast::match_pattern::InstancePattern {
        constructor: constructor_,
        properties: properties_,
        comments: comments_,
    })
}

pub fn member<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    _annot: &T,
    expr: &ast::expression::Member<M, T>,
) -> Result<ast::expression::Member<N, U>, E> {
    let ast::expression::Member {
        object,
        property,
        comments,
    } = expr;
    let object_ = expression(mapper, object)?;
    let property_ = member_property(mapper, property)?;
    let comments_ = syntax_opt(mapper, comments.as_ref())?;
    Ok(ast::expression::Member {
        object: object_,
        property: property_,
        comments: comments_,
    })
}

pub fn member_property<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    expr: &ast::expression::member::Property<M, T>,
) -> Result<ast::expression::member::Property<N, U>, E> {
    Ok(match expr {
        ast::expression::member::Property::PropertyIdentifier(ident) => {
            ast::expression::member::Property::PropertyIdentifier(t_identifier(mapper, ident)?)
        }
        ast::expression::member::Property::PropertyPrivateName(ident) => {
            ast::expression::member::Property::PropertyPrivateName(private_name(mapper, ident)?)
        }
        ast::expression::member::Property::PropertyExpression(e) => {
            ast::expression::member::Property::PropertyExpression(expression(mapper, e)?)
        }
    })
}

pub fn meta_property<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    expr: &ast::expression::MetaProperty<M>,
) -> Result<ast::expression::MetaProperty<N>, E> {
    let ast::expression::MetaProperty {
        meta,
        property,
        comments,
    } = expr;
    Ok(ast::expression::MetaProperty {
        meta: identifier(mapper, meta)?,
        property: identifier(mapper, property)?,
        comments: syntax_opt(mapper, comments.as_ref())?,
    })
}

pub fn new_<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    _annot: &T,
    expr: &ast::expression::New<M, T>,
) -> Result<ast::expression::New<N, U>, E> {
    let ast::expression::New {
        callee,
        targs,
        arguments,
        comments,
    } = expr;
    let callee_ = expression(mapper, callee)?;
    let targs_ = targs
        .as_ref()
        .map(|t| call_type_args(mapper, t))
        .transpose()?;
    let arguments_ = arguments
        .as_ref()
        .map(|a| arg_list(mapper, a))
        .transpose()?;
    let comments_ = syntax_opt(mapper, comments.as_ref())?;
    Ok(ast::expression::New {
        callee: callee_,
        targs: targs_,
        arguments: arguments_,
        comments: comments_,
    })
}

pub fn object_<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    expr: &ast::expression::Object<M, T>,
) -> Result<ast::expression::Object<N, U>, E> {
    let ast::expression::Object {
        properties,
        comments,
    } = expr;
    let comments_ = syntax_with_internal_opt(mapper, comments.as_ref())?;
    let properties_ = try_map_vec(properties, |p| {
        object_property_or_spread_property(mapper, p)
    })?;
    Ok(ast::expression::Object {
        properties: properties_.into(),
        comments: comments_,
    })
}

pub fn object_property_or_spread_property<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    prop: &ast::expression::object::Property<M, T>,
) -> Result<ast::expression::object::Property<N, U>, E> {
    Ok(match prop {
        ast::expression::object::Property::NormalProperty(p) => {
            ast::expression::object::Property::NormalProperty(object_property(mapper, p)?)
        }
        ast::expression::object::Property::SpreadProperty(s) => {
            ast::expression::object::Property::SpreadProperty(spread_property(mapper, s)?)
        }
    })
}

pub fn object_property<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    prop: &ast::expression::object::NormalProperty<M, T>,
) -> Result<ast::expression::object::NormalProperty<N, U>, E> {
    Ok(match prop {
        ast::expression::object::NormalProperty::Init {
            loc,
            key,
            value,
            shorthand,
        } => {
            let key_ = object_key(mapper, key)?;
            let value_ = expression(mapper, value)?;
            ast::expression::object::NormalProperty::Init {
                loc: mapper.on_loc_annot(loc)?,
                key: key_,
                value: value_,
                shorthand: *shorthand,
            }
        }
        ast::expression::object::NormalProperty::Method { loc, key, value } => {
            let (fn_annot, fn_) = value;
            let key_ = object_key(mapper, key)?;
            let fn__ = function_expression(mapper, fn_)?;
            ast::expression::object::NormalProperty::Method {
                loc: mapper.on_loc_annot(loc)?,
                key: key_,
                value: (mapper.on_loc_annot(fn_annot)?, fn__),
            }
        }
        ast::expression::object::NormalProperty::Get {
            loc,
            key,
            value,
            comments,
        } => {
            let (fn_annot, fn_) = value;
            let key_ = object_key(mapper, key)?;
            let fn__ = function_expression(mapper, fn_)?;
            let comments_ = syntax_opt(mapper, comments.as_ref())?;
            ast::expression::object::NormalProperty::Get {
                loc: mapper.on_loc_annot(loc)?,
                key: key_,
                value: (mapper.on_loc_annot(fn_annot)?, fn__),
                comments: comments_,
            }
        }
        ast::expression::object::NormalProperty::Set {
            loc,
            key,
            value,
            comments,
        } => {
            let (fn_annot, fn_) = value;
            let key_ = object_key(mapper, key)?;
            let fn__ = function_expression(mapper, fn_)?;
            let comments_ = syntax_opt(mapper, comments.as_ref())?;
            ast::expression::object::NormalProperty::Set {
                loc: mapper.on_loc_annot(loc)?,
                key: key_,
                value: (mapper.on_loc_annot(fn_annot)?, fn__),
                comments: comments_,
            }
        }
    })
}

pub fn object_key<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    key: &ast::expression::object::Key<M, T>,
) -> Result<ast::expression::object::Key<N, U>, E> {
    Ok(match key {
        ast::expression::object::Key::StringLiteral((annot, lit)) => {
            ast::expression::object::Key::StringLiteral((
                mapper.on_type_annot(annot)?,
                string_literal(mapper, lit)?,
            ))
        }
        ast::expression::object::Key::NumberLiteral((annot, lit)) => {
            ast::expression::object::Key::NumberLiteral((
                mapper.on_type_annot(annot)?,
                number_literal(mapper, lit)?,
            ))
        }
        ast::expression::object::Key::BigIntLiteral((annot, lit)) => {
            ast::expression::object::Key::BigIntLiteral((
                mapper.on_type_annot(annot)?,
                bigint_literal(mapper, lit)?,
            ))
        }
        ast::expression::object::Key::Identifier(ident) => {
            ast::expression::object::Key::Identifier(t_identifier(mapper, ident)?)
        }
        ast::expression::object::Key::PrivateName(ident) => {
            ast::expression::object::Key::PrivateName(private_name(mapper, ident)?)
        }
        ast::expression::object::Key::Computed(computed) => {
            ast::expression::object::Key::Computed(computed_key(mapper, computed)?)
        }
    })
}

pub fn spread_property<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    expr: &ast::expression::object::SpreadProperty<M, T>,
) -> Result<ast::expression::object::SpreadProperty<N, U>, E> {
    let ast::expression::object::SpreadProperty {
        loc,
        argument,
        comments,
    } = expr;
    let loc_ = mapper.on_loc_annot(loc)?;
    let argument_ = expression(mapper, argument)?;
    let comments_ = syntax_opt(mapper, comments.as_ref())?;
    Ok(ast::expression::object::SpreadProperty {
        loc: loc_,
        argument: argument_,
        comments: comments_,
    })
}

pub fn optional_call<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    annot: &T,
    expr: &ast::expression::OptionalCall<M, T>,
) -> Result<ast::expression::OptionalCall<N, U>, E> {
    let ast::expression::OptionalCall {
        call: call_,
        filtered_out,
        optional,
    } = expr;
    let call__ = call(mapper, annot, call_)?;
    let filtered_out_ = mapper.on_type_annot(filtered_out)?;
    let optional_ = match optional {
        ast::expression::OptionalCallKind::Optional => ast::expression::OptionalCallKind::Optional,
        ast::expression::OptionalCallKind::NonOptional => {
            ast::expression::OptionalCallKind::NonOptional
        }
        ast::expression::OptionalCallKind::AssertNonnull { assertion_loc } => {
            ast::expression::OptionalCallKind::AssertNonnull {
                assertion_loc: mapper.on_loc_annot(assertion_loc)?,
            }
        }
    };
    Ok(ast::expression::OptionalCall {
        call: call__,
        filtered_out: filtered_out_,
        optional: optional_,
    })
}

pub fn optional_member<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    annot: &T,
    expr: &ast::expression::OptionalMember<M, T>,
) -> Result<ast::expression::OptionalMember<N, U>, E> {
    let ast::expression::OptionalMember {
        member: member_,
        filtered_out,
        optional,
    } = expr;
    let member__ = member(mapper, annot, member_)?;
    let filtered_out_ = mapper.on_type_annot(filtered_out)?;
    let optional_ = match optional {
        ast::expression::OptionalMemberKind::Optional => {
            ast::expression::OptionalMemberKind::Optional
        }
        ast::expression::OptionalMemberKind::NonOptional => {
            ast::expression::OptionalMemberKind::NonOptional
        }
        ast::expression::OptionalMemberKind::AssertNonnull { assertion_loc } => {
            ast::expression::OptionalMemberKind::AssertNonnull {
                assertion_loc: mapper.on_loc_annot(assertion_loc)?,
            }
        }
    };
    Ok(ast::expression::OptionalMember {
        member: member__,
        filtered_out: filtered_out_,
        optional: optional_,
    })
}

pub fn record<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    expr: &ast::expression::Record<M, T>,
) -> Result<ast::expression::Record<N, U>, E> {
    let ast::expression::Record {
        constructor,
        targs,
        properties,
        comments,
    } = expr;
    let constructor_ = expression(mapper, constructor)?;
    let targs_ = targs
        .as_ref()
        .map(|t| call_type_args(mapper, t))
        .transpose()?;
    let properties_ = (
        mapper.on_loc_annot(&properties.0)?,
        object_(mapper, &properties.1)?,
    );
    let comments_ = syntax_opt(mapper, comments.as_ref())?;
    Ok(ast::expression::Record {
        constructor: constructor_,
        targs: targs_,
        properties: properties_,
        comments: comments_,
    })
}

pub fn sequence<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    expr: &ast::expression::Sequence<M, T>,
) -> Result<ast::expression::Sequence<N, U>, E> {
    let ast::expression::Sequence {
        expressions,
        comments,
    } = expr;
    let expressions_ = try_map_vec(expressions, |e| expression(mapper, e))?.into();
    let comments_ = syntax_opt(mapper, comments.as_ref())?;
    Ok(ast::expression::Sequence {
        expressions: expressions_,
        comments: comments_,
    })
}

pub fn super_expression<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    expr: &ast::expression::Super<M>,
) -> Result<ast::expression::Super<N>, E> {
    let ast::expression::Super { comments } = expr;
    let comments_ = syntax_opt(mapper, comments.as_ref())?;
    Ok(ast::expression::Super {
        comments: comments_,
    })
}

pub fn tagged_template<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    expr: &ast::expression::TaggedTemplate<M, T>,
) -> Result<ast::expression::TaggedTemplate<N, U>, E> {
    let ast::expression::TaggedTemplate {
        tag,
        targs,
        quasi,
        comments,
    } = expr;
    let tag_ = expression(mapper, tag)?;
    let targs_ = targs
        .as_ref()
        .map(|t| call_type_args(mapper, t))
        .transpose()?;
    let quasi_ = {
        let (annot, inner) = quasi;
        (
            mapper.on_loc_annot(annot)?,
            template_literal(mapper, inner)?,
        )
    };
    let comments_ = syntax_opt(mapper, comments.as_ref())?;
    Ok(ast::expression::TaggedTemplate {
        tag: tag_,
        targs: targs_,
        quasi: quasi_,
        comments: comments_,
    })
}

pub fn template_literal<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    expr: &ast::expression::TemplateLiteral<M, T>,
) -> Result<ast::expression::TemplateLiteral<N, U>, E> {
    let ast::expression::TemplateLiteral {
        quasis,
        expressions,
        comments,
    } = expr;
    let quasis_ = try_map_vec(quasis, |q| template_literal_element(mapper, q))?;
    let expressions_ = try_map_vec(expressions, |e| expression(mapper, e))?.into();
    let comments_ = syntax_opt(mapper, comments.as_ref())?;
    Ok(ast::expression::TemplateLiteral {
        quasis: quasis_.into(),
        expressions: expressions_,
        comments: comments_,
    })
}

pub fn template_literal_element<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    elem: &ast::expression::template_literal::Element<M>,
) -> Result<ast::expression::template_literal::Element<N>, E> {
    let ast::expression::template_literal::Element { loc, value, tail } = elem;
    Ok(ast::expression::template_literal::Element {
        loc: mapper.on_loc_annot(loc)?,
        value: value.clone(),
        tail: *tail,
    })
}

pub fn template_literal_type<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    t: &ast::types::TypeTemplateLiteral<M, T>,
) -> Result<ast::types::TypeTemplateLiteral<N, U>, E> {
    let ast::types::TypeTemplateLiteral {
        quasis,
        types,
        comments,
    } = t;
    let quasis_ = quasis
        .iter()
        .map(|elem| {
            let ast::types::type_template_literal::Element { loc, value, tail } = elem;
            Ok(ast::types::type_template_literal::Element {
                loc: mapper.on_loc_annot(loc)?,
                value: value.clone(),
                tail: *tail,
            })
        })
        .collect::<Result<Arc<[_]>, E>>()?;
    let types_ = types
        .iter()
        .map(|t| type_(mapper, t))
        .collect::<Result<Arc<[_]>, E>>()?;
    let comments_ = syntax_opt(mapper, comments.as_ref())?;
    Ok(ast::types::TypeTemplateLiteral {
        quasis: quasis_,
        types: types_,
        comments: comments_,
    })
}

pub fn this_expression<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    expr: &ast::expression::This<M>,
) -> Result<ast::expression::This<N>, E> {
    let ast::expression::This { comments } = expr;
    let comments_ = syntax_opt(mapper, comments.as_ref())?;
    Ok(ast::expression::This {
        comments: comments_,
    })
}

pub fn type_cast<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    expr: &ast::expression::TypeCast<M, T>,
) -> Result<ast::expression::TypeCast<N, U>, E> {
    let ast::expression::TypeCast {
        expression: expr_inner,
        annot,
        comments,
    } = expr;
    let expression_ = expression(mapper, expr_inner)?;
    let annot_ = type_annotation(mapper, annot)?;
    let comments_ = syntax_opt(mapper, comments.as_ref())?;
    Ok(ast::expression::TypeCast {
        expression: expression_,
        annot: annot_,
        comments: comments_,
    })
}

pub fn ts_satisfies<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    expr: &ast::expression::TSSatisfies<M, T>,
) -> Result<ast::expression::TSSatisfies<N, U>, E> {
    let ast::expression::TSSatisfies {
        expression: expr_inner,
        annot,
        comments,
    } = expr;
    let expression_ = expression(mapper, expr_inner)?;
    let annot_ = type_annotation(mapper, annot)?;
    let comments_ = syntax_opt(mapper, comments.as_ref())?;
    Ok(ast::expression::TSSatisfies {
        expression: expression_,
        annot: annot_,
        comments: comments_,
    })
}

pub fn unary_expression<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    expr: &ast::expression::Unary<M, T>,
) -> Result<ast::expression::Unary<N, U>, E> {
    let ast::expression::Unary {
        operator,
        argument,
        comments,
    } = expr;
    let argument_ = expression(mapper, argument)?;
    let comments_ = syntax_opt(mapper, comments.as_ref())?;
    Ok(ast::expression::Unary {
        operator: *operator,
        argument: argument_,
        comments: comments_,
    })
}

pub fn update_expression<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    expr: &ast::expression::Update<M, T>,
) -> Result<ast::expression::Update<N, U>, E> {
    let ast::expression::Update {
        operator,
        argument,
        prefix,
        comments,
    } = expr;
    let argument_ = expression(mapper, argument)?;
    let comments_ = syntax_opt(mapper, comments.as_ref())?;
    Ok(ast::expression::Update {
        operator: operator.clone(),
        argument: argument_,
        prefix: *prefix,
        comments: comments_,
    })
}

pub fn yield_<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    expr: &ast::expression::Yield<M, T>,
) -> Result<ast::expression::Yield<N, U>, E> {
    let ast::expression::Yield {
        argument,
        comments,
        delegate,
        result_out,
    } = expr;
    let argument_ = argument
        .as_ref()
        .map(|e| expression(mapper, e))
        .transpose()?;
    let comments_ = syntax_opt(mapper, comments.as_ref())?;
    let result_out_ = mapper.on_type_annot(result_out)?;
    Ok(ast::expression::Yield {
        argument: argument_,
        comments: comments_,
        delegate: *delegate,
        result_out: result_out_,
    })
}

pub fn class_declaration<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    cls: &ast::class::Class<M, T>,
) -> Result<ast::class::Class<N, U>, E> {
    class_(mapper, cls)
}

pub fn class_<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    cls: &ast::class::Class<M, T>,
) -> Result<ast::class::Class<N, U>, E> {
    let ast::class::Class {
        id,
        body,
        tparams,
        extends,
        implements,
        class_decorators,
        abstract_,
        comments,
    } = cls;
    let id_ = id
        .as_ref()
        .map(|i| class_identifier(mapper, i))
        .transpose()?;
    let comments_ = syntax_opt(mapper, comments.as_ref())?;
    let tparams_ = tparams
        .as_ref()
        .map(|tp| type_params(mapper, tp))
        .transpose()?;
    let extends_ = extends
        .as_ref()
        .map(|e| class_extends(mapper, e))
        .transpose()?;
    let body_ = class_body(mapper, body)?;
    let implements_ = implements
        .as_ref()
        .map(|i| class_implements(mapper, i))
        .transpose()?;
    let class_decorators_ = try_map_vec(class_decorators, |d| class_decorator(mapper, d))?;
    Ok(ast::class::Class {
        id: id_,
        body: body_,
        tparams: tparams_,
        extends: extends_,
        implements: implements_,
        class_decorators: class_decorators_.into(),
        abstract_: *abstract_,
        comments: comments_,
    })
}
fn class_identifier<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    ident: &ast::Identifier<M, T>,
) -> Result<ast::Identifier<N, U>, E> {
    pattern_identifier(mapper, ast::VariableKind::Let, ident)
}
fn class_extends<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    extends: &ast::class::Extends<M, T>,
) -> Result<ast::class::Extends<N, U>, E> {
    let ast::class::Extends {
        loc,
        expr,
        targs,
        comments,
    } = extends;
    let loc_ = mapper.on_loc_annot(loc)?;
    let expr_ = expression(mapper, expr)?;
    let targs_ = targs.as_ref().map(|t| type_args(mapper, t)).transpose()?;
    let comments_ = syntax_opt(mapper, comments.as_ref())?;
    Ok(ast::class::Extends {
        loc: loc_,
        expr: expr_,
        targs: targs_,
        comments: comments_,
    })
}
pub fn class_decorator<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    dec: &ast::class::Decorator<M, T>,
) -> Result<ast::class::Decorator<N, U>, E> {
    let ast::class::Decorator {
        loc,
        expression: expr,
        comments,
    } = dec;
    let loc_ = mapper.on_loc_annot(loc)?;
    let expression_ = expression(mapper, expr)?;
    let comments_ = syntax_opt(mapper, comments.as_ref())?;
    Ok(ast::class::Decorator {
        loc: loc_,
        expression: expression_,
        comments: comments_,
    })
}
fn class_body<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    body: &ast::class::Body<M, T>,
) -> Result<ast::class::Body<N, U>, E> {
    let ast::class::Body {
        loc,
        body: elements,
        comments,
    } = body;
    let loc_ = mapper.on_loc_annot(loc)?;
    let body_ = try_map_vec(elements, |e| class_element(mapper, e))?.into();
    let comments_ = syntax_opt(mapper, comments.as_ref())?;
    Ok(ast::class::Body {
        loc: loc_,
        body: body_,
        comments: comments_,
    })
}
pub fn class_element<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    elem: &ast::class::BodyElement<M, T>,
) -> Result<ast::class::BodyElement<N, U>, E> {
    use ast::class::BodyElement;
    Ok(match elem {
        BodyElement::Method(meth) => BodyElement::Method(class_method(mapper, meth)?),
        BodyElement::Property(prop) => BodyElement::Property(class_property(mapper, prop)?),
        BodyElement::PrivateField(field) => {
            BodyElement::PrivateField(class_private_field(mapper, field)?)
        }
        BodyElement::StaticBlock(block) => {
            BodyElement::StaticBlock(class_static_block(mapper, block)?)
        }
        BodyElement::DeclareMethod(decl_meth) => {
            BodyElement::DeclareMethod(class_declare_method(mapper, decl_meth)?)
        }
        BodyElement::AbstractMethod(abs_meth) => {
            BodyElement::AbstractMethod(class_abstract_method(mapper, abs_meth)?)
        }
        BodyElement::AbstractProperty(abs_prop) => {
            BodyElement::AbstractProperty(class_abstract_property(mapper, abs_prop)?)
        }
        BodyElement::IndexSignature(indexer) => {
            BodyElement::IndexSignature(object_indexer_property_type(mapper, indexer)?)
        }
    })
}
fn class_key<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    key: &ast::expression::object::Key<M, T>,
) -> Result<ast::expression::object::Key<N, U>, E> {
    object_key(mapper, key)
}
fn class_method_key<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    key: &ast::expression::object::Key<M, T>,
) -> Result<ast::expression::object::Key<N, U>, E> {
    class_key(mapper, key)
}
fn class_property_key<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    key: &ast::expression::object::Key<M, T>,
) -> Result<ast::expression::object::Key<N, U>, E> {
    class_key(mapper, key)
}
fn class_method<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    meth: &ast::class::Method<M, T>,
) -> Result<ast::class::Method<N, U>, E> {
    let ast::class::Method {
        loc,
        kind,
        key,
        value,
        static_,
        override_,
        ts_accessibility,
        decorators,
        comments,
    } = meth;
    let loc_ = mapper.on_type_annot(loc)?;
    let key_ = class_method_key(mapper, key)?;
    let value_ = {
        let (annot, func) = value;
        (
            mapper.on_loc_annot(annot)?,
            function_expression(mapper, func)?,
        )
    };
    let ts_accessibility_ = ts_accessibility
        .as_ref()
        .map(|tsa| {
            Ok(ast::class::ts_accessibility::TSAccessibility {
                loc: mapper.on_loc_annot(&tsa.loc)?,
                kind: tsa.kind,
                comments: syntax_opt(mapper, tsa.comments.as_ref())?,
            })
        })
        .transpose()?;
    let decorators_ = try_map_vec(decorators, |d| class_decorator(mapper, d))?;
    let comments_ = syntax_opt(mapper, comments.as_ref())?;
    Ok(ast::class::Method {
        loc: loc_,
        kind: *kind,
        key: key_,
        value: value_,
        static_: *static_,
        override_: *override_,
        ts_accessibility: ts_accessibility_,
        decorators: decorators_.into(),
        comments: comments_,
    })
}
fn class_declare_method<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    decl_meth: &ast::class::DeclareMethod<M, T>,
) -> Result<ast::class::DeclareMethod<N, U>, E> {
    let ast::class::DeclareMethod {
        loc,
        kind,
        key,
        annot,
        static_,
        override_,
        optional,
        ts_accessibility,
        comments,
    } = decl_meth;
    let loc_ = mapper.on_type_annot(loc)?;
    let key_ = class_method_key(mapper, key)?;
    let annot_ = type_annotation(mapper, annot)?;
    let ts_accessibility_ = ts_accessibility
        .as_ref()
        .map(|tsa| {
            Ok(ast::class::ts_accessibility::TSAccessibility {
                loc: mapper.on_loc_annot(&tsa.loc)?,
                kind: tsa.kind,
                comments: syntax_opt(mapper, tsa.comments.as_ref())?,
            })
        })
        .transpose()?;
    let comments_ = syntax_opt(mapper, comments.as_ref())?;
    Ok(ast::class::DeclareMethod {
        loc: loc_,
        kind: *kind,
        key: key_,
        annot: annot_,
        static_: *static_,
        override_: *override_,
        optional: *optional,
        ts_accessibility: ts_accessibility_,
        comments: comments_,
    })
}

fn class_abstract_method<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    abs_meth: &ast::class::AbstractMethod<M, T>,
) -> Result<ast::class::AbstractMethod<N, U>, E> {
    let ast::class::AbstractMethod {
        loc,
        key,
        annot: (annot_loc, func),
        override_,
        ts_accessibility,
        comments,
    } = abs_meth;
    let loc_ = mapper.on_type_annot(loc)?;
    let key_ = class_method_key(mapper, key)?;
    let annot_ = (
        mapper.on_loc_annot(annot_loc)?,
        function_type(mapper, func)?,
    );
    let ts_accessibility_ = ts_accessibility
        .as_ref()
        .map(|tsa| {
            Ok(ast::class::ts_accessibility::TSAccessibility {
                loc: mapper.on_loc_annot(&tsa.loc)?,
                kind: tsa.kind,
                comments: syntax_opt(mapper, tsa.comments.as_ref())?,
            })
        })
        .transpose()?;
    let comments_ = syntax_opt(mapper, comments.as_ref())?;
    Ok(ast::class::AbstractMethod {
        loc: loc_,
        key: key_,
        annot: annot_,
        override_: *override_,
        ts_accessibility: ts_accessibility_,
        comments: comments_,
    })
}

fn class_abstract_property<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    abs_prop: &ast::class::AbstractProperty<M, T>,
) -> Result<ast::class::AbstractProperty<N, U>, E> {
    let ast::class::AbstractProperty {
        loc,
        key,
        annot,
        override_,
        ts_accessibility,
        variance: var,
        comments,
    } = abs_prop;
    let loc_ = mapper.on_type_annot(loc)?;
    let key_ = class_property_key(mapper, key)?;
    let annot_ = type_annotation_hint(mapper, annot)?;
    let ts_accessibility_ = ts_accessibility
        .as_ref()
        .map(|tsa| {
            Ok(ast::class::ts_accessibility::TSAccessibility {
                loc: mapper.on_loc_annot(&tsa.loc)?,
                kind: tsa.kind,
                comments: syntax_opt(mapper, tsa.comments.as_ref())?,
            })
        })
        .transpose()?;
    let variance_ = var.as_ref().map(|v| variance(mapper, v)).transpose()?;
    let comments_ = syntax_opt(mapper, comments.as_ref())?;
    Ok(ast::class::AbstractProperty {
        loc: loc_,
        key: key_,
        annot: annot_,
        override_: *override_,
        ts_accessibility: ts_accessibility_,
        variance: variance_,
        comments: comments_,
    })
}

pub fn class_property<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    prop: &ast::class::Property<M, T>,
) -> Result<ast::class::Property<N, U>, E> {
    let ast::class::Property {
        loc,
        key,
        value,
        annot,
        static_,
        override_,
        optional,
        definite,
        variance: var,
        ts_accessibility,
        decorators,
        comments,
    } = prop;
    let loc_ = mapper.on_type_annot(loc)?;
    let key_ = class_property_key(mapper, key)?;
    let value_ = class_property_value(mapper, value)?;
    let annot_ = type_annotation_hint(mapper, annot)?;
    let decorators_ = try_map_vec(decorators, |d| class_decorator(mapper, d))?;
    let variance_ = var.as_ref().map(|v| variance(mapper, v)).transpose()?;
    let ts_accessibility_ = ts_accessibility
        .as_ref()
        .map(|tsa| {
            Ok(ast::class::ts_accessibility::TSAccessibility {
                loc: mapper.on_loc_annot(&tsa.loc)?,
                kind: tsa.kind,
                comments: syntax_opt(mapper, tsa.comments.as_ref())?,
            })
        })
        .transpose()?;
    let comments_ = syntax_opt(mapper, comments.as_ref())?;
    Ok(ast::class::Property {
        loc: loc_,
        key: key_,
        value: value_,
        annot: annot_,
        static_: *static_,
        override_: *override_,
        optional: *optional,
        definite: *definite,
        variance: variance_,
        ts_accessibility: ts_accessibility_,
        decorators: decorators_.into(),
        comments: comments_,
    })
}
fn class_property_value<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    value: &ast::class::property::Value<M, T>,
) -> Result<ast::class::property::Value<N, U>, E> {
    use ast::class::property::Value;
    Ok(match value {
        Value::Declared => Value::Declared,
        Value::Uninitialized => Value::Uninitialized,
        Value::Initialized(expr) => Value::Initialized(expression(mapper, expr)?),
    })
}
fn class_private_field<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    field: &ast::class::PrivateField<M, T>,
) -> Result<ast::class::PrivateField<N, U>, E> {
    let ast::class::PrivateField {
        loc,
        key,
        value,
        annot,
        static_,
        override_,
        optional,
        definite,
        variance: var,
        ts_accessibility,
        decorators,
        comments,
    } = field;
    let loc_ = mapper.on_type_annot(loc)?;
    let key_ = private_name(mapper, key)?;
    let value_ = class_property_value(mapper, value)?;
    let annot_ = type_annotation_hint(mapper, annot)?;
    let decorators_ = try_map_vec(decorators, |d| class_decorator(mapper, d))?;
    let variance_ = var.as_ref().map(|v| variance(mapper, v)).transpose()?;
    let ts_accessibility_ = ts_accessibility
        .as_ref()
        .map(|tsa| {
            Ok(ast::class::ts_accessibility::TSAccessibility {
                loc: mapper.on_loc_annot(&tsa.loc)?,
                kind: tsa.kind,
                comments: syntax_opt(mapper, tsa.comments.as_ref())?,
            })
        })
        .transpose()?;
    let comments_ = syntax_opt(mapper, comments.as_ref())?;
    Ok(ast::class::PrivateField {
        loc: loc_,
        key: key_,
        value: value_,
        annot: annot_,
        static_: *static_,
        override_: *override_,
        optional: *optional,
        definite: *definite,
        variance: variance_,
        ts_accessibility: ts_accessibility_,
        decorators: decorators_.into(),
        comments: comments_,
    })
}
fn class_static_block<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    block: &ast::class::StaticBlock<M, T>,
) -> Result<ast::class::StaticBlock<N, U>, E> {
    let ast::class::StaticBlock {
        loc,
        body,
        comments,
    } = block;
    let loc_ = mapper.on_loc_annot(loc)?;
    let body_ = statement_list(mapper, body)?;
    let comments_ = syntax_with_internal_opt(mapper, comments.as_ref())?;
    Ok(ast::class::StaticBlock {
        loc: loc_,
        body: body_.into(),
        comments: comments_,
    })
}
fn class_implements<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    implements: &ast::class::Implements<M, T>,
) -> Result<ast::class::Implements<N, U>, E> {
    let ast::class::Implements {
        loc,
        interfaces,
        comments,
    } = implements;
    let loc_ = mapper.on_loc_annot(loc)?;
    let interfaces_ = try_map_vec(interfaces, |i| class_implements_interface(mapper, i))?;
    let comments_ = syntax_opt(mapper, comments.as_ref())?;
    Ok(ast::class::Implements {
        loc: loc_,
        interfaces: interfaces_.into(),
        comments: comments_,
    })
}
fn class_implements_interface<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    interface: &ast::class::implements::Interface<M, T>,
) -> Result<ast::class::implements::Interface<N, U>, E> {
    let ast::class::implements::Interface { loc, id, targs } = interface;
    let loc_ = mapper.on_loc_annot(loc)?;
    let id_ = generic_identifier_type(mapper, id)?;
    let targs_ = targs.as_ref().map(|t| type_args(mapper, t)).transpose()?;
    Ok(ast::class::implements::Interface {
        loc: loc_,
        id: id_,
        targs: targs_,
    })
}

pub fn component_declaration<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    component: &ast::statement::ComponentDeclaration<M, T>,
) -> Result<ast::statement::ComponentDeclaration<N, U>, E> {
    let ast::statement::ComponentDeclaration {
        id: ident,
        params,
        body,
        renders,
        tparams,
        comments,
        sig_loc,
        async_,
    } = component;
    let ident_ = component_identifier(mapper, ident)?;
    let tparams_ = tparams
        .as_ref()
        .map(|tp| type_params(mapper, tp))
        .transpose()?;
    let params_ = component_params(mapper, params)?;
    let renders_ = component_renders_annotation(mapper, renders)?;
    let body_ = body
        .as_ref()
        .map(|b| component_body(mapper, b))
        .transpose()?;
    let comments_ = syntax_opt(mapper, comments.as_ref())?;
    let sig_loc_ = mapper.on_loc_annot(sig_loc)?;
    Ok(ast::statement::ComponentDeclaration {
        id: ident_,
        params: params_,
        renders: renders_,
        body: body_,
        tparams: tparams_,
        comments: comments_,
        sig_loc: sig_loc_,
        async_: *async_,
    })
}
fn component_identifier<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    ident: &ast::Identifier<M, T>,
) -> Result<ast::Identifier<N, U>, E> {
    pattern_identifier(mapper, ast::VariableKind::Var, ident)
}
pub fn component_params<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    params: &ast::statement::component_params::Params<M, T>,
) -> Result<ast::statement::component_params::Params<N, U>, E> {
    let ast::statement::component_params::Params {
        loc: annot,
        params: params_list,
        rest,
        comments,
    } = params;
    let params_list_ = try_map_vec(params_list, |p| component_param(mapper, p))?;
    let rest_ = rest
        .as_ref()
        .map(|r| component_rest_param(mapper, r))
        .transpose()?;
    let comments_ = syntax_with_internal_opt(mapper, comments.as_ref())?;
    Ok(ast::statement::component_params::Params {
        loc: mapper.on_type_annot(annot)?,
        params: params_list_.into(),
        rest: rest_,
        comments: comments_,
    })
}
fn component_param<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    param: &ast::statement::component_params::Param<M, T>,
) -> Result<ast::statement::component_params::Param<N, U>, E> {
    let ast::statement::component_params::Param {
        loc: annot,
        name,
        local,
        default,
        shorthand,
    } = param;
    let annot_ = mapper.on_type_annot(annot)?;
    let name_ = component_param_name(mapper, name)?;
    let local_ = component_param_pattern(mapper, local)?;
    let default_ = default
        .as_ref()
        .map(|d| expression(mapper, d))
        .transpose()?;
    Ok(ast::statement::component_params::Param {
        loc: annot_,
        name: name_,
        local: local_,
        default: default_,
        shorthand: *shorthand,
    })
}
fn component_param_pattern<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    expr: &ast::pattern::Pattern<M, T>,
) -> Result<ast::pattern::Pattern<N, U>, E> {
    binding_pattern(mapper, ast::VariableKind::Var, expr)
}
fn component_rest_param<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    param: &ast::statement::component_params::RestParam<M, T>,
) -> Result<ast::statement::component_params::RestParam<N, U>, E> {
    let ast::statement::component_params::RestParam {
        loc: annot,
        argument,
        comments,
    } = param;
    let annot_ = mapper.on_type_annot(annot)?;
    let argument_ = component_param_pattern(mapper, argument)?;
    let comments_ = syntax_opt(mapper, comments.as_ref())?;
    Ok(ast::statement::component_params::RestParam {
        loc: annot_,
        argument: argument_,
        comments: comments_,
    })
}
pub fn component_body<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    body: &(M, ast::statement::Block<M, T>),
) -> Result<(N, ast::statement::Block<N, U>), E> {
    let (annot, blk) = body;
    Ok((mapper.on_loc_annot(annot)?, block(mapper, blk)?))
}

pub fn declare_class<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    decl: &ast::statement::DeclareClass<M, T>,
) -> Result<ast::statement::DeclareClass<N, U>, E> {
    let ast::statement::DeclareClass {
        id: ident,
        tparams,
        body,
        extends,
        mixins,
        implements,
        abstract_,
        comments,
    } = decl;
    let id_ = class_identifier(mapper, ident)?;
    let tparams_ = tparams
        .as_ref()
        .map(|tp| type_params(mapper, tp))
        .transpose()?;
    let body_ = {
        let (a, b) = body;
        (mapper.on_loc_annot(a)?, object_type(mapper, b)?)
    };
    let extends_ = extends
        .as_ref()
        .map(|(ext_loc, ext)| {
            Ok((
                mapper.on_loc_annot(ext_loc)?,
                declare_class_extends(mapper, ext)?,
            ))
        })
        .transpose()?;
    let mixins_ = try_map_vec(mixins, |(a, gt)| {
        Ok((mapper.on_loc_annot(a)?, generic_type(mapper, gt)?))
    })?
    .into();
    let implements_ = implements
        .as_ref()
        .map(|i| class_implements(mapper, i))
        .transpose()?;
    let comments_ = syntax_opt(mapper, comments.as_ref())?;
    Ok(ast::statement::DeclareClass {
        id: id_,
        tparams: tparams_,
        body: body_,
        extends: extends_,
        mixins: mixins_,
        implements: implements_,
        abstract_: *abstract_,
        comments: comments_,
    })
}

pub fn declare_class_extends<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    ext: &ast::statement::DeclareClassExtends<M, T>,
) -> Result<ast::statement::DeclareClassExtends<N, U>, E> {
    match ext {
        ast::statement::DeclareClassExtends::ExtendsIdent(generic) => Ok(
            ast::statement::DeclareClassExtends::ExtendsIdent(generic_type(mapper, generic)?),
        ),
        ast::statement::DeclareClassExtends::ExtendsCall {
            callee: (callee_loc, callee),
            arg,
        } => Ok(ast::statement::DeclareClassExtends::ExtendsCall {
            callee: (
                mapper.on_loc_annot(callee_loc)?,
                generic_type(mapper, callee)?,
            ),
            arg: Box::new((
                mapper.on_loc_annot(&arg.0)?,
                declare_class_extends(mapper, &arg.1)?,
            )),
        }),
    }
}

pub fn declare_component<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    decl: &ast::statement::DeclareComponent<M, T>,
) -> Result<ast::statement::DeclareComponent<N, U>, E> {
    let ast::statement::DeclareComponent {
        id: ident,
        params,
        renders,
        tparams,
        comments,
    } = decl;
    let ident_ = t_identifier(mapper, ident)?;
    let tparams_ = tparams
        .as_ref()
        .map(|tp| type_params(mapper, tp))
        .transpose()?;
    let params_ = component_params(mapper, params)?;
    let renders_ = component_renders_annotation(mapper, renders)?;
    let comments_ = syntax_opt(mapper, comments.as_ref())?;
    Ok(ast::statement::DeclareComponent {
        id: ident_,
        tparams: tparams_,
        params: params_,
        renders: renders_,
        comments: comments_,
    })
}

pub fn declare_enum<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    decl: &ast::statement::EnumDeclaration<M, T>,
) -> Result<ast::statement::EnumDeclaration<N, U>, E> {
    enum_declaration(mapper, decl)
}

pub fn declare_export_declaration<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    _loc: &M,
    decl: &ast::statement::DeclareExportDeclaration<M, T>,
) -> Result<ast::statement::DeclareExportDeclaration<N, U>, E> {
    let ast::statement::DeclareExportDeclaration {
        default,
        source,
        specifiers,
        declaration,
        comments,
    } = decl;
    let default_ = default
        .as_ref()
        .map(|d| mapper.on_loc_annot(d))
        .transpose()?;
    let source_ = source
        .as_ref()
        .map(|(loc, lit)| Ok((mapper.on_type_annot(loc)?, string_literal(mapper, lit)?)))
        .transpose()?;
    let specifiers_ = specifiers
        .as_ref()
        .map(|s| export_named_specifier(mapper, s))
        .transpose()?;
    let declaration_ = declaration
        .as_ref()
        .map(|d| declare_export_declaration_decl(mapper, d))
        .transpose()?;
    let comments_ = syntax_opt(mapper, comments.as_ref())?;
    Ok(ast::statement::DeclareExportDeclaration {
        default: default_,
        source: source_,
        specifiers: specifiers_,
        declaration: declaration_,
        comments: comments_,
    })
}

pub fn declare_export_declaration_decl<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    decl: &ast::statement::declare_export_declaration::Declaration<M, T>,
) -> Result<ast::statement::declare_export_declaration::Declaration<N, U>, E> {
    Ok(match decl {
        ast::statement::declare_export_declaration::Declaration::Variable { loc, declaration } => {
            ast::statement::declare_export_declaration::Declaration::Variable {
                loc: mapper.on_loc_annot(loc)?,
                declaration: Arc::new(declare_variable(mapper, declaration)?),
            }
        }
        ast::statement::declare_export_declaration::Declaration::Function { loc, declaration } => {
            ast::statement::declare_export_declaration::Declaration::Function {
                loc: mapper.on_loc_annot(loc)?,
                declaration: Arc::new(declare_function(mapper, declaration)?),
            }
        }
        ast::statement::declare_export_declaration::Declaration::Class { loc, declaration } => {
            ast::statement::declare_export_declaration::Declaration::Class {
                loc: mapper.on_loc_annot(loc)?,
                declaration: Arc::new(declare_class(mapper, declaration)?),
            }
        }
        ast::statement::declare_export_declaration::Declaration::Component { loc, declaration } => {
            ast::statement::declare_export_declaration::Declaration::Component {
                loc: mapper.on_loc_annot(loc)?,
                declaration: Arc::new(declare_component(mapper, declaration)?),
            }
        }
        ast::statement::declare_export_declaration::Declaration::DefaultType { type_: t } => {
            ast::statement::declare_export_declaration::Declaration::DefaultType {
                type_: Arc::new(type_(mapper, t)?),
            }
        }
        ast::statement::declare_export_declaration::Declaration::NamedType { loc, declaration } => {
            ast::statement::declare_export_declaration::Declaration::NamedType {
                loc: mapper.on_loc_annot(loc)?,
                declaration: Arc::new(type_alias(mapper, loc, declaration)?),
            }
        }
        ast::statement::declare_export_declaration::Declaration::NamedOpaqueType {
            loc,
            declaration,
        } => ast::statement::declare_export_declaration::Declaration::NamedOpaqueType {
            loc: mapper.on_loc_annot(loc)?,
            declaration: Arc::new(opaque_type(mapper, loc, declaration)?),
        },
        ast::statement::declare_export_declaration::Declaration::Interface { loc, declaration } => {
            ast::statement::declare_export_declaration::Declaration::Interface {
                loc: mapper.on_loc_annot(loc)?,
                declaration: Arc::new(interface(mapper, loc, declaration)?),
            }
        }
        ast::statement::declare_export_declaration::Declaration::Enum { loc, declaration } => {
            ast::statement::declare_export_declaration::Declaration::Enum {
                loc: mapper.on_loc_annot(loc)?,
                declaration: Arc::new(enum_declaration(mapper, declaration)?),
            }
        }
        ast::statement::declare_export_declaration::Declaration::Namespace { loc, declaration } => {
            ast::statement::declare_export_declaration::Declaration::Namespace {
                loc: mapper.on_loc_annot(loc)?,
                declaration: Box::new(declare_namespace(mapper, loc, declaration)?),
            }
        }
    })
}

pub fn declare_function<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    decl: &ast::statement::DeclareFunction<M, T>,
) -> Result<ast::statement::DeclareFunction<N, U>, E> {
    let ast::statement::DeclareFunction {
        id: ident,
        annot,
        predicate: pred,
        comments,
        implicit_declare,
    } = decl;
    let id_ = ident
        .as_ref()
        .map(|id| function_identifier(mapper, id))
        .transpose()?;
    let annot_ = type_annotation(mapper, annot)?;
    let predicate_ = pred.as_ref().map(|p| predicate(mapper, p)).transpose()?;
    let comments_ = syntax_opt(mapper, comments.as_ref())?;
    Ok(ast::statement::DeclareFunction {
        id: id_,
        annot: annot_,
        predicate: predicate_,
        comments: comments_,
        implicit_declare: *implicit_declare,
    })
}

pub fn declare_interface<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    loc: &M,
    decl: &ast::statement::Interface<M, T>,
) -> Result<ast::statement::Interface<N, U>, E> {
    interface(mapper, loc, decl)
}

pub fn declare_module<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    _loc: &M,
    decl: &ast::statement::DeclareModule<M, T>,
) -> Result<ast::statement::DeclareModule<N, U>, E> {
    let ast::statement::DeclareModule { id, body, comments } = decl;
    let id_ = match id {
        ast::statement::declare_module::Id::Identifier(ident) => {
            ast::statement::declare_module::Id::Identifier(t_identifier(mapper, ident)?)
        }
        ast::statement::declare_module::Id::Literal((annot, name)) => {
            ast::statement::declare_module::Id::Literal((
                mapper.on_type_annot(annot)?,
                string_literal(mapper, name)?,
            ))
        }
    };
    let body_ = (mapper.on_loc_annot(&body.0)?, block(mapper, &body.1)?);
    let comments_ = syntax_opt(mapper, comments.as_ref())?;
    Ok(ast::statement::DeclareModule {
        id: id_,
        body: body_,
        comments: comments_,
    })
}

pub fn declare_namespace<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    _loc: &M,
    decl: &ast::statement::DeclareNamespace<M, T>,
) -> Result<ast::statement::DeclareNamespace<N, U>, E> {
    let ast::statement::DeclareNamespace {
        id,
        body,
        implicit_declare,
        keyword,
        comments,
    } = decl;
    let id_ = match id {
        ast::statement::declare_namespace::Id::Global(ident) => {
            ast::statement::declare_namespace::Id::Global(identifier(mapper, ident)?)
        }
        ast::statement::declare_namespace::Id::Local(ident) => {
            ast::statement::declare_namespace::Id::Local(t_identifier(mapper, ident)?)
        }
    };
    let body_ = (mapper.on_loc_annot(&body.0)?, block(mapper, &body.1)?);
    let comments_ = syntax_opt(mapper, comments.as_ref())?;
    Ok(ast::statement::DeclareNamespace {
        id: id_,
        body: body_,
        implicit_declare: *implicit_declare,
        keyword: *keyword,
        comments: comments_,
    })
}

pub fn declare_type_alias<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    loc: &M,
    decl: &ast::statement::TypeAlias<M, T>,
) -> Result<ast::statement::TypeAlias<N, U>, E> {
    type_alias(mapper, loc, decl)
}

pub fn declare_variable<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    decl: &ast::statement::DeclareVariable<M, T>,
) -> Result<ast::statement::DeclareVariable<N, U>, E> {
    let ast::statement::DeclareVariable {
        declarations,
        kind,
        comments,
    } = decl;
    let declarations_ = declarations
        .iter()
        .map(|d| declare_variable_declarator(mapper, *kind, d))
        .collect::<Result<Arc<[_]>, E>>()?;
    let comments_ = syntax_opt(mapper, comments.as_ref())?;
    Ok(ast::statement::DeclareVariable {
        declarations: declarations_,
        kind: *kind,
        comments: comments_,
    })
}

pub fn declare_variable_declarator<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    kind: ast::VariableKind,
    decl: &ast::statement::variable::Declarator<M, T>,
) -> Result<ast::statement::variable::Declarator<N, U>, E> {
    let ast::statement::variable::Declarator {
        loc,
        id,
        init,
        definite,
    } = decl;
    let loc_ = mapper.on_loc_annot(loc)?;
    let id_ = variable_declarator_pattern(mapper, kind, id)?;
    let init_ = init.as_ref().map(|i| expression(mapper, i)).transpose()?;
    Ok(ast::statement::variable::Declarator {
        loc: loc_,
        id: id_,
        init: init_,
        definite: *definite,
    })
}

pub fn declare_module_exports<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    decl: &ast::statement::DeclareModuleExports<M, T>,
) -> Result<ast::statement::DeclareModuleExports<N, U>, E> {
    let ast::statement::DeclareModuleExports { annot, comments } = decl;
    let annot_ = type_annotation(mapper, annot)?;
    let comments_ = syntax_opt(mapper, comments.as_ref())?;
    Ok(ast::statement::DeclareModuleExports {
        annot: annot_,
        comments: comments_,
    })
}

pub fn do_while<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    stmt: &ast::statement::DoWhile<M, T>,
) -> Result<ast::statement::DoWhile<N, U>, E> {
    let ast::statement::DoWhile {
        body,
        test,
        comments,
    } = stmt;
    let body_ = statement(mapper, body)?;
    let test_ = predicate_expression(mapper, test)?;
    let comments_ = syntax_opt(mapper, comments.as_ref())?;
    Ok(ast::statement::DoWhile {
        body: body_,
        test: test_,
        comments: comments_,
    })
}

pub fn predicate_expression<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    expr: &ast::expression::Expression<M, T>,
) -> Result<ast::expression::Expression<N, U>, E> {
    expression(mapper, expr)
}

pub fn enum_declaration<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    decl: &ast::statement::EnumDeclaration<M, T>,
) -> Result<ast::statement::EnumDeclaration<N, U>, E> {
    let ast::statement::EnumDeclaration {
        id,
        body,
        const_,
        comments,
    } = decl;
    let body_ = enum_body(mapper, body)?;
    let comments_ = syntax_opt(mapper, comments.as_ref())?;
    Ok(ast::statement::EnumDeclaration {
        id: t_identifier(mapper, id)?,
        body: body_,
        const_: *const_,
        comments: comments_,
    })
}

pub fn enum_body<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    body: &ast::statement::enum_declaration::Body<M>,
) -> Result<ast::statement::enum_declaration::Body<N>, E> {
    let ast::statement::enum_declaration::Body {
        loc,
        members,
        explicit_type,
        has_unknown_members,
        comments,
    } = body;
    let loc_ = mapper.on_loc_annot(loc)?;
    let members_ = try_map_vec(members, |m| enum_member(mapper, m))?;
    let explicit_type_ = explicit_type
        .as_ref()
        .map(|(loc, t)| Ok((mapper.on_loc_annot(loc)?, *t)))
        .transpose()?;
    let has_unknown_members_ = has_unknown_members
        .as_ref()
        .map(|loc| mapper.on_loc_annot(loc))
        .transpose()?;
    let comments_ = syntax_with_internal_opt(mapper, comments.as_ref())?;
    Ok(ast::statement::enum_declaration::Body {
        loc: loc_,
        members: members_.into(),
        explicit_type: explicit_type_,
        has_unknown_members: has_unknown_members_,
        comments: comments_,
    })
}

pub fn enum_member<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    member: &ast::statement::enum_declaration::Member<M>,
) -> Result<ast::statement::enum_declaration::Member<N>, E> {
    Ok(match member {
        ast::statement::enum_declaration::Member::BooleanMember(m) => {
            let ast::statement::enum_declaration::InitializedMember {
                loc,
                id,
                init: (init_annot, init_val),
            } = m;
            let init_ = (
                mapper.on_loc_annot(init_annot)?,
                boolean_literal(mapper, init_val)?,
            );
            ast::statement::enum_declaration::Member::BooleanMember(
                ast::statement::enum_declaration::InitializedMember {
                    loc: mapper.on_loc_annot(loc)?,
                    id: enum_member_name(mapper, id)?,
                    init: init_,
                },
            )
        }
        ast::statement::enum_declaration::Member::NumberMember(m) => {
            let ast::statement::enum_declaration::InitializedMember {
                loc,
                id,
                init: (init_annot, init_val),
            } = m;
            let init_ = (
                mapper.on_loc_annot(init_annot)?,
                number_literal(mapper, init_val)?,
            );
            ast::statement::enum_declaration::Member::NumberMember(
                ast::statement::enum_declaration::InitializedMember {
                    loc: mapper.on_loc_annot(loc)?,
                    id: enum_member_name(mapper, id)?,
                    init: init_,
                },
            )
        }
        ast::statement::enum_declaration::Member::StringMember(m) => {
            let ast::statement::enum_declaration::InitializedMember {
                loc,
                id,
                init: (init_annot, init_val),
            } = m;
            let init_ = (
                mapper.on_loc_annot(init_annot)?,
                string_literal(mapper, init_val)?,
            );
            ast::statement::enum_declaration::Member::StringMember(
                ast::statement::enum_declaration::InitializedMember {
                    loc: mapper.on_loc_annot(loc)?,
                    id: enum_member_name(mapper, id)?,
                    init: init_,
                },
            )
        }
        ast::statement::enum_declaration::Member::BigIntMember(m) => {
            let ast::statement::enum_declaration::InitializedMember {
                loc,
                id,
                init: (init_annot, init_val),
            } = m;
            let init_ = (
                mapper.on_loc_annot(init_annot)?,
                bigint_literal(mapper, init_val)?,
            );
            ast::statement::enum_declaration::Member::BigIntMember(
                ast::statement::enum_declaration::InitializedMember {
                    loc: mapper.on_loc_annot(loc)?,
                    id: enum_member_name(mapper, id)?,
                    init: init_,
                },
            )
        }
        ast::statement::enum_declaration::Member::DefaultedMember(m) => {
            let ast::statement::enum_declaration::DefaultedMember { loc, id } = m;
            ast::statement::enum_declaration::Member::DefaultedMember(
                ast::statement::enum_declaration::DefaultedMember {
                    loc: mapper.on_loc_annot(loc)?,
                    id: enum_member_name(mapper, id)?,
                },
            )
        }
    })
}

pub fn enum_member_name<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    id: &ast::statement::enum_declaration::MemberName<M>,
) -> Result<ast::statement::enum_declaration::MemberName<N>, E> {
    Ok(match id {
        ast::statement::enum_declaration::MemberName::Identifier(ident) => {
            ast::statement::enum_declaration::MemberName::Identifier(enum_member_identifier(
                mapper, ident,
            )?)
        }
        ast::statement::enum_declaration::MemberName::StringLiteral(annot, lit) => {
            ast::statement::enum_declaration::MemberName::StringLiteral(
                mapper.on_loc_annot(annot)?,
                string_literal(mapper, lit)?,
            )
        }
    })
}

pub fn enum_member_identifier<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    ident: &ast::Identifier<M, M>,
) -> Result<ast::Identifier<N, N>, E> {
    identifier(mapper, ident)
}

pub fn export_default_declaration<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    _loc: &M,
    decl: &ast::statement::ExportDefaultDeclaration<M, T>,
) -> Result<ast::statement::ExportDefaultDeclaration<N, U>, E> {
    let ast::statement::ExportDefaultDeclaration {
        default,
        declaration,
        comments,
    } = decl;
    let default_ = mapper.on_type_annot(default)?;
    let declaration_ = export_default_declaration_decl(mapper, declaration)?;
    let comments_ = syntax_opt(mapper, comments.as_ref())?;
    Ok(ast::statement::ExportDefaultDeclaration {
        default: default_,
        declaration: declaration_,
        comments: comments_,
    })
}

pub fn export_default_declaration_decl<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    decl: &ast::statement::export_default_declaration::Declaration<M, T>,
) -> Result<ast::statement::export_default_declaration::Declaration<N, U>, E> {
    Ok(match decl {
        ast::statement::export_default_declaration::Declaration::Declaration(stmt) => {
            ast::statement::export_default_declaration::Declaration::Declaration(statement(
                mapper, stmt,
            )?)
        }
        ast::statement::export_default_declaration::Declaration::Expression(expr) => {
            ast::statement::export_default_declaration::Declaration::Expression(expression(
                mapper, expr,
            )?)
        }
    })
}

pub fn export_named_declaration<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    _loc: &M,
    decl: &ast::statement::ExportNamedDeclaration<M, T>,
) -> Result<ast::statement::ExportNamedDeclaration<N, U>, E> {
    let ast::statement::ExportNamedDeclaration {
        export_kind,
        source,
        specifiers,
        declaration,
        attributes,
        comments,
    } = decl;
    let source_ = source
        .as_ref()
        .map(|(loc, lit)| Ok((mapper.on_type_annot(loc)?, string_literal(mapper, lit)?)))
        .transpose()?;
    let specifiers_ = specifiers
        .as_ref()
        .map(|s| export_named_specifier(mapper, s))
        .transpose()?;
    let declaration_ = declaration
        .as_ref()
        .map(|d| statement(mapper, d))
        .transpose()?;
    let attributes_ = attributes
        .as_ref()
        .map(|(annot, attrs)| {
            let annot_ = mapper.on_loc_annot(annot)?;
            let attrs_ = try_map_vec(attrs, |attr| import_attribute(mapper, attr))?;
            Ok((annot_, attrs_))
        })
        .transpose()?;
    let comments_ = syntax_opt(mapper, comments.as_ref())?;
    Ok(ast::statement::ExportNamedDeclaration {
        export_kind: *export_kind,
        source: source_,
        specifiers: specifiers_,
        declaration: declaration_,
        attributes: attributes_,
        comments: comments_,
    })
}

fn export_assignment<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    assign: &ast::statement::ExportAssignment<M, T>,
) -> Result<ast::statement::ExportAssignment<N, U>, E> {
    let ast::statement::ExportAssignment { rhs, comments } = assign;
    let rhs_ = match rhs {
        ast::statement::ExportAssignmentRhs::Expression(expr) => {
            ast::statement::ExportAssignmentRhs::Expression(expression(mapper, expr)?)
        }
        ast::statement::ExportAssignmentRhs::DeclareFunction(loc, decl) => {
            ast::statement::ExportAssignmentRhs::DeclareFunction(
                mapper.on_loc_annot(loc)?,
                declare_function(mapper, decl)?,
            )
        }
    };
    let comments_ = syntax_opt(mapper, comments.as_ref())?;
    Ok(ast::statement::ExportAssignment {
        rhs: rhs_,
        comments: comments_,
    })
}

fn namespace_export_declaration<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    decl: &ast::statement::NamespaceExportDeclaration<M, T>,
) -> Result<ast::statement::NamespaceExportDeclaration<N, U>, E> {
    let ast::statement::NamespaceExportDeclaration { id, comments } = decl;
    let id_ = t_identifier(mapper, id)?;
    let comments_ = syntax_opt(mapper, comments.as_ref())?;
    Ok(ast::statement::NamespaceExportDeclaration {
        id: id_,
        comments: comments_,
    })
}

pub fn export_named_specifier<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    spec: &ast::statement::export_named_declaration::Specifier<M, T>,
) -> Result<ast::statement::export_named_declaration::Specifier<N, U>, E> {
    Ok(match spec {
        ast::statement::export_named_declaration::Specifier::ExportSpecifiers(specs) => {
            ast::statement::export_named_declaration::Specifier::ExportSpecifiers(try_map_vec(
                specs,
                |s| export_named_declaration_specifier(mapper, s),
            )?)
        }
        ast::statement::export_named_declaration::Specifier::ExportBatchSpecifier(batch) => {
            ast::statement::export_named_declaration::Specifier::ExportBatchSpecifier(
                export_batch_specifier(mapper, batch)?,
            )
        }
    })
}

pub fn export_named_declaration_specifier<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    spec: &ast::statement::export_named_declaration::ExportSpecifier<M, T>,
) -> Result<ast::statement::export_named_declaration::ExportSpecifier<N, U>, E> {
    let ast::statement::export_named_declaration::ExportSpecifier {
        loc,
        local,
        exported,
        export_kind,
        from_remote,
        imported_name_def_loc,
    } = spec;
    let local_ = t_identifier(mapper, local)?;
    let exported_ = exported
        .as_ref()
        .map(|e| t_identifier(mapper, e))
        .transpose()?;
    let imported_name_def_loc_ = imported_name_def_loc
        .as_ref()
        .map(|loc| mapper.on_loc_annot(loc))
        .transpose()?;
    Ok(ast::statement::export_named_declaration::ExportSpecifier {
        loc: mapper.on_loc_annot(loc)?,
        local: local_,
        exported: exported_,
        export_kind: *export_kind,
        from_remote: *from_remote,
        imported_name_def_loc: imported_name_def_loc_,
    })
}

pub fn export_batch_specifier<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    spec: &ast::statement::export_named_declaration::ExportBatchSpecifier<M, T>,
) -> Result<ast::statement::export_named_declaration::ExportBatchSpecifier<N, U>, E> {
    let ast::statement::export_named_declaration::ExportBatchSpecifier { loc, specifier } = spec;
    let loc_ = mapper.on_loc_annot(loc)?;
    let specifier_ = specifier
        .as_ref()
        .map(|s| t_identifier(mapper, s))
        .transpose()?;
    Ok(
        ast::statement::export_named_declaration::ExportBatchSpecifier {
            loc: loc_,
            specifier: specifier_,
        },
    )
}

pub fn expression_statement<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    stmt: &ast::statement::Expression<M, T>,
) -> Result<ast::statement::Expression<N, U>, E> {
    let ast::statement::Expression {
        expression: expr,
        directive,
        comments,
    } = stmt;
    let expression_ = expression(mapper, expr)?;
    let comments_ = syntax_opt(mapper, comments.as_ref())?;
    Ok(ast::statement::Expression {
        expression: expression_,
        directive: directive.clone(),
        comments: comments_,
    })
}

pub fn for_statement<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    stmt: &ast::statement::For<M, T>,
) -> Result<ast::statement::For<N, U>, E> {
    let ast::statement::For {
        init,
        test,
        update,
        body,
        comments,
    } = stmt;
    let init_ = init
        .as_ref()
        .map(|i| for_statement_init(mapper, i))
        .transpose()?;
    let test_ = test
        .as_ref()
        .map(|t| predicate_expression(mapper, t))
        .transpose()?;
    let update_ = update.as_ref().map(|u| expression(mapper, u)).transpose()?;
    let body_ = statement(mapper, body)?;
    let comments_ = syntax_opt(mapper, comments.as_ref())?;
    Ok(ast::statement::For {
        init: init_,
        test: test_,
        update: update_,
        body: body_,
        comments: comments_,
    })
}

pub fn for_statement_init<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    init: &ast::statement::for_::Init<M, T>,
) -> Result<ast::statement::for_::Init<N, U>, E> {
    Ok(match init {
        ast::statement::for_::Init::InitDeclaration(decl) => {
            ast::statement::for_::Init::InitDeclaration(for_init_declaration(mapper, decl)?)
        }
        ast::statement::for_::Init::InitExpression(expr) => {
            ast::statement::for_::Init::InitExpression(expression(mapper, expr)?)
        }
    })
}

pub fn for_init_declaration<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    decl: &(M, ast::statement::VariableDeclaration<M, T>),
) -> Result<(N, ast::statement::VariableDeclaration<N, U>), E> {
    let (annot, d) = decl;
    Ok((
        mapper.on_loc_annot(annot)?,
        variable_declaration(mapper, d)?,
    ))
}

pub fn for_in_statement<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    stmt: &ast::statement::ForIn<M, T>,
) -> Result<ast::statement::ForIn<N, U>, E> {
    let ast::statement::ForIn {
        left,
        right,
        body,
        comments,
    } = stmt;
    let left_ = for_in_statement_lhs(mapper, left)?;
    let right_ = expression(mapper, right)?;
    let body_ = statement(mapper, body)?;
    let comments_ = syntax_opt(mapper, comments.as_ref())?;
    Ok(ast::statement::ForIn {
        left: left_,
        right: right_,
        body: body_,
        comments: comments_,
    })
}

pub fn for_in_statement_lhs<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    left: &ast::statement::for_in::Left<M, T>,
) -> Result<ast::statement::for_in::Left<N, U>, E> {
    Ok(match left {
        ast::statement::for_in::Left::LeftDeclaration(decl) => {
            ast::statement::for_in::Left::LeftDeclaration(for_in_left_declaration(mapper, decl)?)
        }
        ast::statement::for_in::Left::LeftPattern(patt) => {
            ast::statement::for_in::Left::LeftPattern(for_in_assignment_pattern(mapper, patt)?)
        }
    })
}

pub fn for_in_left_declaration<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    decl: &(M, ast::statement::VariableDeclaration<M, T>),
) -> Result<(N, ast::statement::VariableDeclaration<N, U>), E> {
    let (annot, d) = decl;
    Ok((
        mapper.on_loc_annot(annot)?,
        variable_declaration(mapper, d)?,
    ))
}

pub fn assignment_pattern<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    patt: &ast::pattern::Pattern<M, T>,
) -> Result<ast::pattern::Pattern<N, U>, E> {
    pattern(mapper, None, patt)
}

pub fn pattern<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    kind: Option<ast::VariableKind>,
    patt: &ast::pattern::Pattern<M, T>,
) -> Result<ast::pattern::Pattern<N, U>, E> {
    let _ = kind;
    Ok(match patt {
        ast::pattern::Pattern::Object { loc, inner } => ast::pattern::Pattern::Object {
            loc: mapper.on_type_annot(loc)?,
            inner: Arc::new(pattern_object(mapper, kind, inner)?),
        },
        ast::pattern::Pattern::Array { loc, inner } => ast::pattern::Pattern::Array {
            loc: mapper.on_type_annot(loc)?,
            inner: Arc::new(pattern_array(mapper, kind, inner)?),
        },
        ast::pattern::Pattern::Identifier { loc, inner } => ast::pattern::Pattern::Identifier {
            loc: mapper.on_type_annot(loc)?,
            inner: Arc::new(pattern_identifier_inner(mapper, kind, inner)?),
        },
        ast::pattern::Pattern::Expression { loc, inner } => ast::pattern::Pattern::Expression {
            loc: mapper.on_type_annot(loc)?,
            inner: Arc::new(pattern_expression(mapper, inner)?),
        },
    })
}
fn pattern_object<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    kind: Option<ast::VariableKind>,
    obj: &ast::pattern::Object<M, T>,
) -> Result<ast::pattern::Object<N, U>, E> {
    let ast::pattern::Object {
        properties,
        annot,
        optional,
        comments,
    } = obj;
    let properties_ = try_map_vec(properties, |p| pattern_object_p(mapper, kind, p))?;
    let annot_ = type_annotation_hint(mapper, annot)?;
    let comments_ = syntax_with_internal_opt(mapper, comments.as_ref())?;
    Ok(ast::pattern::Object {
        properties: properties_.into(),
        annot: annot_,
        optional: *optional,
        comments: comments_,
    })
}
fn pattern_object_p<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    kind: Option<ast::VariableKind>,
    p: &ast::pattern::object::Property<M, T>,
) -> Result<ast::pattern::object::Property<N, U>, E> {
    use ast::pattern::object::Property;
    Ok(match p {
        Property::NormalProperty(prop) => {
            Property::NormalProperty(pattern_object_property(mapper, kind, prop)?)
        }
        Property::RestElement(rest) => {
            Property::RestElement(pattern_object_rest_property(mapper, kind, rest)?)
        }
    })
}
fn pattern_object_property<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    kind: Option<ast::VariableKind>,
    prop: &ast::pattern::object::NormalProperty<M, T>,
) -> Result<ast::pattern::object::NormalProperty<N, U>, E> {
    let ast::pattern::object::NormalProperty {
        loc,
        key,
        pattern: patt,
        default,
        shorthand,
    } = prop;
    let loc_ = mapper.on_loc_annot(loc)?;
    let key_ = pattern_object_property_key(mapper, kind, key)?;
    let pattern_ = pattern_object_property_pattern(mapper, kind, patt)?;
    let default_ = default
        .as_ref()
        .map(|d| expression(mapper, d))
        .transpose()?;
    Ok(ast::pattern::object::NormalProperty {
        loc: loc_,
        key: key_,
        pattern: pattern_,
        default: default_,
        shorthand: *shorthand,
    })
}
fn pattern_object_property_key<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    kind: Option<ast::VariableKind>,
    key: &ast::pattern::object::Key<M, T>,
) -> Result<ast::pattern::object::Key<N, U>, E> {
    use ast::pattern::object::Key;
    Ok(match key {
        Key::StringLiteral((annot, lit)) => Key::StringLiteral((
            mapper.on_loc_annot(annot)?,
            pattern_object_property_string_literal_key(mapper, kind, lit)?,
        )),
        Key::NumberLiteral((annot, lit)) => Key::NumberLiteral((
            mapper.on_loc_annot(annot)?,
            pattern_object_property_number_literal_key(mapper, kind, lit)?,
        )),
        Key::BigIntLiteral((annot, lit)) => Key::BigIntLiteral((
            mapper.on_loc_annot(annot)?,
            pattern_object_property_bigint_literal_key(mapper, kind, lit)?,
        )),
        Key::Identifier(identifier) => Key::Identifier(pattern_object_property_identifier_key(
            mapper, kind, identifier,
        )?),
        Key::Computed(expr) => {
            Key::Computed(pattern_object_property_computed_key(mapper, kind, expr)?)
        }
    })
}
fn pattern_object_property_string_literal_key<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    kind: Option<ast::VariableKind>,
    key: &ast::StringLiteral<M>,
) -> Result<ast::StringLiteral<N>, E> {
    pattern_string_literal(mapper, kind, key)
}
fn pattern_object_property_number_literal_key<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    kind: Option<ast::VariableKind>,
    key: &ast::NumberLiteral<M>,
) -> Result<ast::NumberLiteral<N>, E> {
    pattern_number_literal(mapper, kind, key)
}
fn pattern_object_property_bigint_literal_key<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    kind: Option<ast::VariableKind>,
    key: &ast::BigIntLiteral<M>,
) -> Result<ast::BigIntLiteral<N>, E> {
    pattern_bigint_literal(mapper, kind, key)
}
fn pattern_object_property_identifier_key<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    kind: Option<ast::VariableKind>,
    key: &ast::Identifier<M, T>,
) -> Result<ast::Identifier<N, U>, E> {
    pattern_identifier(mapper, kind.unwrap_or(ast::VariableKind::Var), key)
}
fn pattern_object_property_computed_key<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    kind: Option<ast::VariableKind>,
    key: &ast::ComputedKey<M, T>,
) -> Result<ast::ComputedKey<N, U>, E> {
    let _ = kind;
    computed_key(mapper, key)
}
fn pattern_object_rest_property<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    kind: Option<ast::VariableKind>,
    rest: &ast::pattern::RestElement<M, T>,
) -> Result<ast::pattern::RestElement<N, U>, E> {
    let ast::pattern::RestElement {
        loc,
        argument,
        comments,
    } = rest;
    let loc_ = mapper.on_loc_annot(loc)?;
    let argument_ = pattern_object_rest_property_pattern(mapper, kind, argument)?;
    let comments_ = syntax_opt(mapper, comments.as_ref())?;
    Ok(ast::pattern::RestElement {
        loc: loc_,
        argument: argument_,
        comments: comments_,
    })
}
fn pattern_object_property_pattern<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    kind: Option<ast::VariableKind>,
    patt: &ast::pattern::Pattern<M, T>,
) -> Result<ast::pattern::Pattern<N, U>, E> {
    pattern(mapper, kind, patt)
}
fn pattern_object_rest_property_pattern<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    kind: Option<ast::VariableKind>,
    patt: &ast::pattern::Pattern<M, T>,
) -> Result<ast::pattern::Pattern<N, U>, E> {
    pattern(mapper, kind, patt)
}
fn pattern_string_literal<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    kind: Option<ast::VariableKind>,
    lit: &ast::StringLiteral<M>,
) -> Result<ast::StringLiteral<N>, E> {
    let _ = kind;
    string_literal(mapper, lit)
}
fn pattern_number_literal<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    kind: Option<ast::VariableKind>,
    lit: &ast::NumberLiteral<M>,
) -> Result<ast::NumberLiteral<N>, E> {
    let _ = kind;
    number_literal(mapper, lit)
}
fn pattern_bigint_literal<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    kind: Option<ast::VariableKind>,
    lit: &ast::BigIntLiteral<M>,
) -> Result<ast::BigIntLiteral<N>, E> {
    let _ = kind;
    bigint_literal(mapper, lit)
}
fn pattern_array<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    kind: Option<ast::VariableKind>,
    arr: &ast::pattern::Array<M, T>,
) -> Result<ast::pattern::Array<N, U>, E> {
    let ast::pattern::Array {
        elements,
        annot,
        optional,
        comments,
    } = arr;
    let elements_ = try_map_vec(elements, |e| pattern_array_e(mapper, kind, e))?;
    let annot_ = type_annotation_hint(mapper, annot)?;
    let comments_ = syntax_with_internal_opt(mapper, comments.as_ref())?;
    Ok(ast::pattern::Array {
        elements: elements_.into(),
        annot: annot_,
        optional: *optional,
        comments: comments_,
    })
}
fn pattern_array_e<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    kind: Option<ast::VariableKind>,
    e: &ast::pattern::array::Element<M, T>,
) -> Result<ast::pattern::array::Element<N, U>, E> {
    use ast::pattern::array::Element;
    Ok(match e {
        Element::Hole(loc) => Element::Hole(mapper.on_loc_annot(loc)?),
        Element::NormalElement(elem) => {
            Element::NormalElement(pattern_array_element(mapper, kind, elem)?)
        }
        Element::RestElement(elem) => {
            Element::RestElement(pattern_array_rest_element(mapper, kind, elem)?)
        }
    })
}
fn pattern_array_element<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    kind: Option<ast::VariableKind>,
    elem: &ast::pattern::array::NormalElement<M, T>,
) -> Result<ast::pattern::array::NormalElement<N, U>, E> {
    let ast::pattern::array::NormalElement {
        loc,
        argument,
        default,
    } = elem;
    let loc_ = mapper.on_loc_annot(loc)?;
    let argument_ = pattern_array_element_pattern(mapper, kind, argument)?;
    let default_ = default
        .as_ref()
        .map(|d| expression(mapper, d))
        .transpose()?;
    Ok(ast::pattern::array::NormalElement {
        loc: loc_,
        argument: argument_,
        default: default_,
    })
}
fn pattern_array_element_pattern<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    kind: Option<ast::VariableKind>,
    patt: &ast::pattern::Pattern<M, T>,
) -> Result<ast::pattern::Pattern<N, U>, E> {
    pattern(mapper, kind, patt)
}
fn pattern_array_rest_element<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    kind: Option<ast::VariableKind>,
    elem: &ast::pattern::RestElement<M, T>,
) -> Result<ast::pattern::RestElement<N, U>, E> {
    let ast::pattern::RestElement {
        loc,
        argument,
        comments,
    } = elem;
    let loc_ = mapper.on_loc_annot(loc)?;
    let argument_ = pattern_array_rest_element_pattern(mapper, kind, argument)?;
    let comments_ = syntax_opt(mapper, comments.as_ref())?;
    Ok(ast::pattern::RestElement {
        loc: loc_,
        argument: argument_,
        comments: comments_,
    })
}
fn pattern_array_rest_element_pattern<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    kind: Option<ast::VariableKind>,
    patt: &ast::pattern::Pattern<M, T>,
) -> Result<ast::pattern::Pattern<N, U>, E> {
    pattern(mapper, kind, patt)
}
fn pattern_identifier_inner<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    kind: Option<ast::VariableKind>,
    ident: &ast::pattern::Identifier<M, T>,
) -> Result<ast::pattern::Identifier<N, U>, E> {
    let ast::pattern::Identifier {
        name,
        annot,
        optional,
    } = ident;
    let name_ = pattern_identifier(mapper, kind.unwrap_or(ast::VariableKind::Var), name)?;
    let annot_ = type_annotation_hint(mapper, annot)?;
    Ok(ast::pattern::Identifier {
        name: name_,
        annot: annot_,
        optional: *optional,
    })
}
fn pattern_expression<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    expr: &ast::expression::Expression<M, T>,
) -> Result<ast::expression::Expression<N, U>, E> {
    expression(mapper, expr)
}

pub fn for_in_assignment_pattern<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    patt: &ast::pattern::Pattern<M, T>,
) -> Result<ast::pattern::Pattern<N, U>, E> {
    assignment_pattern(mapper, patt)
}

pub fn for_of_statement<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    stmt: &ast::statement::ForOf<M, T>,
) -> Result<ast::statement::ForOf<N, U>, E> {
    let ast::statement::ForOf {
        left,
        right,
        body,
        await_,
        comments,
    } = stmt;
    let left_ = for_of_statement_lhs(mapper, left)?;
    let right_ = expression(mapper, right)?;
    let body_ = statement(mapper, body)?;
    let comments_ = syntax_opt(mapper, comments.as_ref())?;
    Ok(ast::statement::ForOf {
        left: left_,
        right: right_,
        body: body_,
        await_: *await_,
        comments: comments_,
    })
}
fn for_of_statement_lhs<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    left: &ast::statement::for_of::Left<M, T>,
) -> Result<ast::statement::for_of::Left<N, U>, E> {
    Ok(match left {
        ast::statement::for_of::Left::LeftDeclaration(decl) => {
            ast::statement::for_of::Left::LeftDeclaration(for_of_left_declaration(mapper, decl)?)
        }
        ast::statement::for_of::Left::LeftPattern(patt) => {
            ast::statement::for_of::Left::LeftPattern(for_of_assignment_pattern(mapper, patt)?)
        }
    })
}
fn for_of_left_declaration<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    (annot, decl): &(M, ast::statement::VariableDeclaration<M, T>),
) -> Result<(N, ast::statement::VariableDeclaration<N, U>), E> {
    Ok((
        mapper.on_loc_annot(annot)?,
        variable_declaration(mapper, decl)?,
    ))
}
fn for_of_assignment_pattern<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    patt: &ast::pattern::Pattern<M, T>,
) -> Result<ast::pattern::Pattern<N, U>, E> {
    assignment_pattern(mapper, patt)
}

pub fn function_declaration<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    func: &ast::function::Function<M, T>,
) -> Result<ast::function::Function<N, U>, E> {
    function_(mapper, func)
}
pub fn function_return_annotation<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    return_: &ast::function::ReturnAnnot<M, T>,
) -> Result<ast::function::ReturnAnnot<N, U>, E> {
    Ok(match return_ {
        ast::function::ReturnAnnot::Missing(loc) => {
            ast::function::ReturnAnnot::Missing(mapper.on_type_annot(loc)?)
        }
        ast::function::ReturnAnnot::Available(annot) => {
            ast::function::ReturnAnnot::Available(type_annotation(mapper, annot)?)
        }
        ast::function::ReturnAnnot::TypeGuard(guard) => {
            ast::function::ReturnAnnot::TypeGuard(type_guard_annotation(mapper, guard)?)
        }
    })
}
pub fn type_guard_annotation<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    guard_annot: &ast::types::TypeGuardAnnotation<M, T>,
) -> Result<ast::types::TypeGuardAnnotation<N, U>, E> {
    let ast::types::TypeGuardAnnotation { loc: annot, guard } = guard_annot;
    let annot_ = mapper.on_loc_annot(annot)?;
    let guard_ = type_guard(mapper, guard)?;
    Ok(ast::types::TypeGuardAnnotation {
        loc: annot_,
        guard: guard_,
    })
}

pub fn function_<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    func: &ast::function::Function<M, T>,
) -> Result<ast::function::Function<N, U>, E> {
    let ast::function::Function {
        id: ident,
        params,
        body,
        async_,
        generator,
        effect_,
        predicate: pred,
        return_,
        tparams,
        sig_loc,
        comments,
    } = func;
    let ident_ = ident
        .as_ref()
        .map(|i| function_identifier(mapper, i))
        .transpose()?;
    let tparams_ = tparams
        .as_ref()
        .map(|tp| type_params(mapper, tp))
        .transpose()?;
    let params_ = function_params(mapper, params)?;
    let return__ = function_return_annotation(mapper, return_)?;
    let body_ = function_body_any(mapper, body)?;
    let predicate_ = pred.as_ref().map(|p| predicate(mapper, p)).transpose()?;
    let sig_loc_ = mapper.on_loc_annot(sig_loc)?;
    let comments_ = syntax_opt(mapper, comments.as_ref())?;
    Ok(ast::function::Function {
        id: ident_,
        params: params_,
        body: body_,
        async_: *async_,
        generator: *generator,
        effect_: *effect_,
        predicate: predicate_,
        return_: return__,
        tparams: tparams_,
        sig_loc: sig_loc_,
        comments: comments_,
    })
}
pub fn function_params<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    params: &ast::function::Params<M, T>,
) -> Result<ast::function::Params<N, U>, E> {
    let ast::function::Params {
        loc: annot,
        params: params_list,
        rest,
        this_,
        comments,
    } = params;
    let params_list_ = try_map_vec(params_list, |p| function_param(mapper, p))?;
    let rest_ = rest
        .as_ref()
        .map(|r| function_rest_param(mapper, r))
        .transpose()?;
    let this__ = this_
        .as_ref()
        .map(|t| function_this_param(mapper, t))
        .transpose()?;
    let comments_ = syntax_with_internal_opt(mapper, comments.as_ref())?;
    Ok(ast::function::Params {
        loc: mapper.on_loc_annot(annot)?,
        params: params_list_.into(),
        rest: rest_,
        this_: this__,
        comments: comments_,
    })
}
fn function_param<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    param: &ast::function::Param<M, T>,
) -> Result<ast::function::Param<N, U>, E> {
    Ok(match param {
        ast::function::Param::RegularParam {
            loc,
            argument,
            default,
        } => {
            let loc_ = mapper.on_loc_annot(loc)?;
            let argument_ = function_param_pattern(mapper, argument)?;
            let default_ = default
                .as_ref()
                .map(|d| expression(mapper, d))
                .transpose()?;
            ast::function::Param::RegularParam {
                loc: loc_,
                argument: argument_,
                default: default_,
            }
        }
        ast::function::Param::ParamProperty { loc, property } => {
            let loc_ = mapper.on_loc_annot(loc)?;
            let property_ = class_property(mapper, property)?;
            ast::function::Param::ParamProperty {
                loc: loc_,
                property: property_,
            }
        }
    })
}
fn function_rest_param<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    param: &ast::function::RestParam<M, T>,
) -> Result<ast::function::RestParam<N, U>, E> {
    let ast::function::RestParam {
        loc: annot,
        argument,
        comments,
    } = param;
    let annot_ = mapper.on_loc_annot(annot)?;
    let argument_ = function_param_pattern(mapper, argument)?;
    let comments_ = syntax_opt(mapper, comments.as_ref())?;
    Ok(ast::function::RestParam {
        loc: annot_,
        argument: argument_,
        comments: comments_,
    })
}
fn function_this_param<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    this_param: &ast::function::ThisParam<M, T>,
) -> Result<ast::function::ThisParam<N, U>, E> {
    let ast::function::ThisParam {
        loc,
        annot,
        comments,
    } = this_param;
    let loc_ = mapper.on_loc_annot(loc)?;
    let annot_ = type_annotation(mapper, annot)?;
    let comments_ = syntax_opt(mapper, comments.as_ref())?;
    Ok(ast::function::ThisParam {
        loc: loc_,
        annot: annot_,
        comments: comments_,
    })
}
pub fn function_body_any<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    body: &ast::function::Body<M, T>,
) -> Result<ast::function::Body<N, U>, E> {
    Ok(match body {
        ast::function::Body::BodyBlock((loc, blk)) => {
            ast::function::Body::BodyBlock((mapper.on_loc_annot(loc)?, block(mapper, blk)?))
        }
        ast::function::Body::BodyExpression(expr) => {
            ast::function::Body::BodyExpression(expression(mapper, expr)?)
        }
    })
}
fn function_param_pattern<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    patt: &ast::pattern::Pattern<M, T>,
) -> Result<ast::pattern::Pattern<N, U>, E> {
    binding_pattern(mapper, ast::VariableKind::Var, patt)
}

pub fn if_statement<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    stmt: &ast::statement::If<M, T>,
) -> Result<ast::statement::If<N, U>, E> {
    let ast::statement::If {
        test,
        consequent,
        alternate,
        comments,
    } = stmt;
    let test_ = predicate_expression(mapper, test)?;
    let consequent_ = if_consequent_statement(mapper, alternate.is_some(), consequent)?;
    let alternate_ = alternate
        .as_ref()
        .map(|alt| if_alternate_statement(mapper, alt))
        .transpose()?;
    let comments_ = syntax_opt(mapper, comments.as_ref())?;
    Ok(ast::statement::If {
        test: test_,
        consequent: consequent_,
        alternate: alternate_,
        comments: comments_,
    })
}
fn if_consequent_statement<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    _has_else: bool,
    stmt: &ast::statement::Statement<M, T>,
) -> Result<ast::statement::Statement<N, U>, E> {
    statement(mapper, stmt)
}
fn if_alternate_statement<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    altern: &ast::statement::if_::Alternate<M, T>,
) -> Result<ast::statement::if_::Alternate<N, U>, E> {
    let ast::statement::if_::Alternate {
        loc,
        body,
        comments,
    } = altern;
    let loc_ = mapper.on_loc_annot(loc)?;
    let body_ = statement(mapper, body)?;
    let comments_ = syntax_opt(mapper, comments.as_ref())?;
    Ok(ast::statement::if_::Alternate {
        loc: loc_,
        body: body_,
        comments: comments_,
    })
}

pub fn import_declaration<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    _loc: &M,
    decl: &ast::statement::ImportDeclaration<M, T>,
) -> Result<ast::statement::ImportDeclaration<N, U>, E> {
    let ast::statement::ImportDeclaration {
        import_kind,
        phase,
        source,
        default,
        specifiers,
        attributes,
        comments,
    } = decl;
    let source_ = {
        let (annot, lit) = source;
        (
            mapper.on_type_annot(annot)?,
            import_source(mapper, annot, lit)?,
        )
    };
    let specifiers_ = specifiers
        .as_ref()
        .map(|s| import_specifier(mapper, *import_kind, s))
        .transpose()?;
    let default_ = default
        .as_ref()
        .map(
            |ast::statement::import_declaration::DefaultIdentifier {
                 identifier,
                 remote_default_name_def_loc,
             }| {
                Ok(ast::statement::import_declaration::DefaultIdentifier {
                    identifier: import_default_specifier(mapper, *import_kind, identifier)?,
                    remote_default_name_def_loc: remote_default_name_def_loc
                        .as_ref()
                        .map(|loc| mapper.on_loc_annot(loc))
                        .transpose()?,
                })
            },
        )
        .transpose()?;
    let attributes_ = attributes
        .as_ref()
        .map(|(annot, attrs)| {
            let annot_ = mapper.on_loc_annot(annot)?;
            let attrs_ = try_map_vec(attrs, |attr| import_attribute(mapper, attr))?;
            Ok((annot_, attrs_))
        })
        .transpose()?;
    let comments_ = syntax_opt(mapper, comments.as_ref())?;
    Ok(ast::statement::ImportDeclaration {
        import_kind: *import_kind,
        phase: *phase,
        source: source_,
        default: default_,
        specifiers: specifiers_,
        attributes: attributes_,
        comments: comments_,
    })
}

fn import_equals_declaration<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    decl: &ast::statement::ImportEqualsDeclaration<M, T>,
) -> Result<ast::statement::ImportEqualsDeclaration<N, U>, E> {
    let ast::statement::ImportEqualsDeclaration {
        id,
        module_reference,
        import_kind,
        is_export,
        comments,
    } = decl;
    let id_ = t_identifier(mapper, id)?;
    let module_reference_ = import_equals_module_reference(mapper, module_reference)?;
    let comments_ = syntax_opt(mapper, comments.as_ref())?;
    Ok(ast::statement::ImportEqualsDeclaration {
        id: id_,
        module_reference: module_reference_,
        import_kind: *import_kind,
        is_export: *is_export,
        comments: comments_,
    })
}

fn import_equals_module_reference<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    module_ref: &ast::statement::import_equals_declaration::ModuleReference<M, T>,
) -> Result<ast::statement::import_equals_declaration::ModuleReference<N, U>, E> {
    Ok(match module_ref {
        ast::statement::import_equals_declaration::ModuleReference::ExternalModuleReference(
            annot,
            lit,
        ) => ast::statement::import_equals_declaration::ModuleReference::ExternalModuleReference(
            mapper.on_type_annot(annot)?,
            string_literal(mapper, lit)?,
        ),
        ast::statement::import_equals_declaration::ModuleReference::Identifier(ident) => {
            ast::statement::import_equals_declaration::ModuleReference::Identifier(
                generic_identifier_type(mapper, ident)?,
            )
        }
    })
}
fn import_source<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    _loc: &T,
    source: &ast::StringLiteral<M>,
) -> Result<ast::StringLiteral<N>, E> {
    string_literal(mapper, source)
}
fn import_attribute_key<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    key: &ast::statement::import_declaration::ImportAttributeKey<M, T>,
) -> Result<ast::statement::import_declaration::ImportAttributeKey<N, U>, E> {
    Ok(match key {
        ast::statement::import_declaration::ImportAttributeKey::Identifier(id) => {
            ast::statement::import_declaration::ImportAttributeKey::Identifier(t_identifier(
                mapper, id,
            )?)
        }
        ast::statement::import_declaration::ImportAttributeKey::StringLiteral(annot, lit) => {
            ast::statement::import_declaration::ImportAttributeKey::StringLiteral(
                mapper.on_loc_annot(annot)?,
                string_literal(mapper, lit)?,
            )
        }
    })
}
fn import_attribute<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    attr: &ast::statement::import_declaration::ImportAttribute<M, T>,
) -> Result<ast::statement::import_declaration::ImportAttribute<N, U>, E> {
    let ast::statement::import_declaration::ImportAttribute { loc, key, value } = attr;
    let loc_ = mapper.on_loc_annot(loc)?;
    let key_ = import_attribute_key(mapper, key)?;
    let value_ = {
        let (annot, lit) = value;
        (mapper.on_type_annot(annot)?, string_literal(mapper, lit)?)
    };
    Ok(ast::statement::import_declaration::ImportAttribute {
        loc: loc_,
        key: key_,
        value: value_,
    })
}
fn import_specifier<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    import_kind: ast::statement::ImportKind,
    specifier: &ast::statement::import_declaration::Specifier<M, T>,
) -> Result<ast::statement::import_declaration::Specifier<N, U>, E> {
    Ok(match specifier {
        ast::statement::import_declaration::Specifier::ImportNamedSpecifiers(named_specifiers) => {
            let named_specifiers_ = try_map_vec(named_specifiers, |s| {
                import_named_specifier(mapper, import_kind, s)
            })?;
            ast::statement::import_declaration::Specifier::ImportNamedSpecifiers(named_specifiers_)
        }
        ast::statement::import_declaration::Specifier::ImportNamespaceSpecifier((annot, ident)) => {
            let ident_ = import_namespace_specifier(mapper, import_kind, annot, ident)?;
            ast::statement::import_declaration::Specifier::ImportNamespaceSpecifier((
                mapper.on_loc_annot(annot)?,
                ident_,
            ))
        }
    })
}
fn remote_identifier<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    id: &ast::Identifier<M, T>,
) -> Result<ast::Identifier<N, U>, E> {
    t_identifier(mapper, id)
}
fn import_named_specifier<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    import_kind: ast::statement::ImportKind,
    specifier: &ast::statement::import_declaration::NamedSpecifier<M, T>,
) -> Result<ast::statement::import_declaration::NamedSpecifier<N, U>, E> {
    let ast::statement::import_declaration::NamedSpecifier {
        kind,
        kind_loc,
        local,
        remote,
        remote_name_def_loc,
    } = specifier;
    let (is_type_remote, is_type_local) = match (import_kind, kind) {
        (ast::statement::ImportKind::ImportType, _)
        | (_, Some(ast::statement::ImportKind::ImportType)) => (true, true),
        (ast::statement::ImportKind::ImportTypeof, _)
        | (_, Some(ast::statement::ImportKind::ImportTypeof)) => (false, true),
        _ => (false, false),
    };
    let remote_ = match local {
        None => {
            if is_type_remote {
                binding_type_identifier(mapper, remote)?
            } else {
                pattern_identifier(mapper, ast::VariableKind::Let, remote)?
            }
        }
        Some(_) => remote_identifier(mapper, remote)?,
    };
    let local_ = match local {
        None => None,
        Some(ident) => {
            if is_type_local {
                Some(binding_type_identifier(mapper, ident)?)
            } else {
                Some(pattern_identifier(mapper, ast::VariableKind::Let, ident)?)
            }
        }
    };
    let kind_loc_ = kind_loc
        .as_ref()
        .map(|loc| mapper.on_loc_annot(loc))
        .transpose()?;
    let remote_name_def_loc_ = remote_name_def_loc
        .as_ref()
        .map(|loc| mapper.on_loc_annot(loc))
        .transpose()?;
    Ok(ast::statement::import_declaration::NamedSpecifier {
        kind: *kind,
        kind_loc: kind_loc_,
        local: local_,
        remote: remote_,
        remote_name_def_loc: remote_name_def_loc_,
    })
}
fn import_default_specifier<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    _import_kind: ast::statement::ImportKind,
    id: &ast::Identifier<M, T>,
) -> Result<ast::Identifier<N, U>, E> {
    pattern_identifier(mapper, ast::VariableKind::Let, id)
}
fn import_namespace_specifier<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    _import_kind: ast::statement::ImportKind,
    _loc: &M,
    id: &ast::Identifier<M, T>,
) -> Result<ast::Identifier<N, U>, E> {
    pattern_identifier(mapper, ast::VariableKind::Let, id)
}
fn interface<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    _loc: &M,
    iface: &ast::statement::Interface<M, T>,
) -> Result<ast::statement::Interface<N, U>, E> {
    let ast::statement::Interface {
        id: ident,
        tparams,
        extends,
        body,
        comments,
    } = iface;
    let id_ = binding_type_identifier(mapper, ident)?;
    let tparams_ = tparams
        .as_ref()
        .map(|tp| type_params(mapper, tp))
        .transpose()?;
    let extends_ = try_map_vec(extends, |(loc, gt)| {
        Ok((mapper.on_loc_annot(loc)?, generic_type(mapper, gt)?))
    })?
    .into();
    let body_ = {
        let (loc, obj) = body;
        (mapper.on_loc_annot(loc)?, object_type(mapper, obj)?)
    };
    let comments_ = syntax_opt(mapper, comments.as_ref())?;
    Ok(ast::statement::Interface {
        id: id_,
        tparams: tparams_,
        extends: extends_,
        body: body_,
        comments: comments_,
    })
}

pub fn interface_declaration<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    loc: &M,
    decl: &ast::statement::Interface<M, T>,
) -> Result<ast::statement::Interface<N, U>, E> {
    interface(mapper, loc, decl)
}

pub fn labeled_statement<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    stmt: &ast::statement::Labeled<M, T>,
) -> Result<ast::statement::Labeled<N, U>, E> {
    let ast::statement::Labeled {
        label,
        body,
        comments,
    } = stmt;
    let label_ = label_identifier(mapper, label)?;
    let body_ = statement(mapper, body)?;
    let comments_ = syntax_opt(mapper, comments.as_ref())?;
    Ok(ast::statement::Labeled {
        label: label_,
        body: body_,
        comments: comments_,
    })
}

pub fn match_statement<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    stmt: &ast::statement::MatchStatement<M, T>,
) -> Result<ast::statement::MatchStatement<N, U>, E> {
    let ast::match_::Match {
        arg,
        cases,
        match_keyword_loc,
        comments,
    } = stmt;
    let arg_ = expression(mapper, arg)?;
    let cases_ = try_map_vec(cases, |case| match_statement_case(mapper, case))?;
    let match_keyword_loc_ = mapper.on_loc_annot(match_keyword_loc)?;
    let comments_ = syntax_opt(mapper, comments.as_ref())?;
    Ok(ast::match_::Match {
        arg: arg_,
        cases: cases_.into(),
        match_keyword_loc: match_keyword_loc_,
        comments: comments_,
    })
}

pub fn match_statement_case<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    case: &ast::match_::Case<M, T, ast::statement::Statement<M, T>>,
) -> Result<ast::match_::Case<N, U, ast::statement::Statement<N, U>>, E> {
    let ast::match_::Case {
        loc,
        pattern,
        body,
        guard,
        comments,
        invalid_syntax,
        case_match_root_loc,
    } = case;
    let loc_ = mapper.on_loc_annot(loc)?;
    let pattern_ = match_pattern(mapper, pattern)?;
    let body_ = statement(mapper, body)?;
    let guard_ = guard.as_ref().map(|g| expression(mapper, g)).transpose()?;
    let comments_ = syntax_opt(mapper, comments.as_ref())?;
    let invalid_syntax_ = match_case_invalid_syntax(mapper, invalid_syntax)?;
    let case_match_root_loc_ = mapper.on_loc_annot(case_match_root_loc)?;
    Ok(ast::match_::Case {
        loc: loc_,
        pattern: pattern_,
        body: body_,
        guard: guard_,
        comments: comments_,
        invalid_syntax: invalid_syntax_,
        case_match_root_loc: case_match_root_loc_,
    })
}

pub fn opaque_type<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    _loc: &M,
    otype: &ast::statement::OpaqueType<M, T>,
) -> Result<ast::statement::OpaqueType<N, U>, E> {
    let ast::statement::OpaqueType {
        id,
        tparams,
        impl_type,
        lower_bound,
        upper_bound,
        legacy_upper_bound,
        comments,
    } = otype;
    let id_ = binding_type_identifier(mapper, id)?;
    let tparams_ = tparams
        .as_ref()
        .map(|tp| type_params(mapper, tp))
        .transpose()?;
    let impl_type_ = impl_type.as_ref().map(|t| type_(mapper, t)).transpose()?;
    let lower_bound_ = lower_bound.as_ref().map(|t| type_(mapper, t)).transpose()?;
    let upper_bound_ = upper_bound.as_ref().map(|t| type_(mapper, t)).transpose()?;
    let legacy_upper_bound_ = legacy_upper_bound
        .as_ref()
        .map(|t| type_(mapper, t))
        .transpose()?;
    let comments_ = syntax_opt(mapper, comments.as_ref())?;
    Ok(ast::statement::OpaqueType {
        id: id_,
        tparams: tparams_,
        impl_type: impl_type_,
        lower_bound: lower_bound_,
        upper_bound: upper_bound_,
        legacy_upper_bound: legacy_upper_bound_,
        comments: comments_,
    })
}

pub fn record_declaration<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    decl: &ast::statement::RecordDeclaration<M, T>,
) -> Result<ast::statement::RecordDeclaration<N, U>, E> {
    let ast::statement::RecordDeclaration {
        id,
        tparams,
        implements,
        body,
        comments,
        invalid_syntax,
    } = decl;
    let id_ = pattern_identifier(mapper, ast::VariableKind::Const, id)?;
    let comments_ = syntax_opt(mapper, comments.as_ref())?;
    let tparams_ = tparams
        .as_ref()
        .map(|tp| type_params(mapper, tp))
        .transpose()?;
    let implements_ = implements
        .as_ref()
        .map(|i| class_implements(mapper, i))
        .transpose()?;
    let body_ = record_body(mapper, body)?;
    let invalid_syntax_ = invalid_syntax
        .as_ref()
        .map(
            |ast::statement::record_declaration::InvalidSyntax {
                 invalid_infix_equals,
             }| {
                Ok(ast::statement::record_declaration::InvalidSyntax {
                    invalid_infix_equals: invalid_infix_equals
                        .as_ref()
                        .map(|l| mapper.on_loc_annot(l))
                        .transpose()?,
                })
            },
        )
        .transpose()?;
    Ok(ast::statement::RecordDeclaration {
        id: id_,
        tparams: tparams_,
        implements: implements_,
        body: body_,
        comments: comments_,
        invalid_syntax: invalid_syntax_,
    })
}
fn record_body<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    body: &ast::statement::record_declaration::Body<M, T>,
) -> Result<ast::statement::record_declaration::Body<N, U>, E> {
    let ast::statement::record_declaration::Body {
        loc,
        body,
        comments,
    } = body;
    let loc_ = mapper.on_loc_annot(loc)?;
    let body_ = try_map_vec(body, |e| record_element(mapper, e))?.into();
    let comments_ = syntax_opt(mapper, comments.as_ref())?;
    Ok(ast::statement::record_declaration::Body {
        loc: loc_,
        body: body_,
        comments: comments_,
    })
}
fn record_element<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    element: &ast::statement::record_declaration::BodyElement<M, T>,
) -> Result<ast::statement::record_declaration::BodyElement<N, U>, E> {
    Ok(match element {
        ast::statement::record_declaration::BodyElement::Method(meth) => {
            ast::statement::record_declaration::BodyElement::Method(class_method(mapper, meth)?)
        }
        ast::statement::record_declaration::BodyElement::Property(prop) => {
            ast::statement::record_declaration::BodyElement::Property(record_property(
                mapper, prop,
            )?)
        }
        ast::statement::record_declaration::BodyElement::StaticProperty(prop) => {
            ast::statement::record_declaration::BodyElement::StaticProperty(record_static_property(
                mapper, prop,
            )?)
        }
    })
}
fn record_invalid_property_syntax<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    invalid_syntax: &ast::statement::record_declaration::InvalidPropertySyntax<M>,
) -> Result<ast::statement::record_declaration::InvalidPropertySyntax<N>, E> {
    let ast::statement::record_declaration::InvalidPropertySyntax {
        invalid_variance,
        invalid_optional,
        invalid_suffix_semicolon,
    } = invalid_syntax;
    let invalid_suffix_semicolon_ = invalid_suffix_semicolon
        .as_ref()
        .map(|l| mapper.on_loc_annot(l))
        .transpose()?;
    let invalid_optional_ = invalid_optional
        .as_ref()
        .map(|l| mapper.on_loc_annot(l))
        .transpose()?;
    let invalid_variance_ = invalid_variance
        .as_ref()
        .map(|v| variance(mapper, v))
        .transpose()?;
    Ok(ast::statement::record_declaration::InvalidPropertySyntax {
        invalid_variance: invalid_variance_,
        invalid_optional: invalid_optional_,
        invalid_suffix_semicolon: invalid_suffix_semicolon_,
    })
}
fn record_property<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    prop: &ast::statement::record_declaration::Property<M, T>,
) -> Result<ast::statement::record_declaration::Property<N, U>, E> {
    let ast::statement::record_declaration::Property {
        loc,
        key,
        annot,
        default_value,
        comments,
        invalid_syntax,
    } = prop;
    let loc_ = mapper.on_type_annot(loc)?;
    let key_ = object_key(mapper, key)?;
    let annot_ = type_annotation(mapper, annot)?;
    let default_value_ = default_value
        .as_ref()
        .map(|e| expression(mapper, e))
        .transpose()?;
    let invalid_syntax_ = invalid_syntax
        .as_ref()
        .map(|is| record_invalid_property_syntax(mapper, is))
        .transpose()?;
    let comments_ = syntax_opt(mapper, comments.as_ref())?;
    Ok(ast::statement::record_declaration::Property {
        loc: loc_,
        key: key_,
        annot: annot_,
        default_value: default_value_,
        comments: comments_,
        invalid_syntax: invalid_syntax_,
    })
}
fn record_static_property<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    prop: &ast::statement::record_declaration::StaticProperty<M, T>,
) -> Result<ast::statement::record_declaration::StaticProperty<N, U>, E> {
    let ast::statement::record_declaration::StaticProperty {
        loc,
        key,
        annot,
        value,
        comments,
        invalid_syntax,
    } = prop;
    let loc_ = mapper.on_type_annot(loc)?;
    let key_ = object_key(mapper, key)?;
    let annot_ = type_annotation(mapper, annot)?;
    let value_ = expression(mapper, value)?;
    let invalid_syntax_ = invalid_syntax
        .as_ref()
        .map(|is| record_invalid_property_syntax(mapper, is))
        .transpose()?;
    let comments_ = syntax_opt(mapper, comments.as_ref())?;
    Ok(ast::statement::record_declaration::StaticProperty {
        loc: loc_,
        key: key_,
        annot: annot_,
        value: value_,
        comments: comments_,
        invalid_syntax: invalid_syntax_,
    })
}

pub fn return_<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    stmt: &ast::statement::Return<M, T>,
) -> Result<ast::statement::Return<N, U>, E> {
    let ast::statement::Return {
        argument,
        comments,
        return_out,
    } = stmt;
    let return_out_ = mapper.on_type_annot(return_out)?;
    let argument_ = argument
        .as_ref()
        .map(|arg| expression(mapper, arg))
        .transpose()?;
    let comments_ = syntax_opt(mapper, comments.as_ref())?;
    Ok(ast::statement::Return {
        argument: argument_,
        comments: comments_,
        return_out: return_out_,
    })
}

pub fn switch<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    stmt: &ast::statement::Switch<M, T>,
) -> Result<ast::statement::Switch<N, U>, E> {
    let ast::statement::Switch {
        discriminant,
        cases,
        comments,
        exhaustive_out,
    } = stmt;
    let exhaustive_out_ = mapper.on_type_annot(exhaustive_out)?;
    let discriminant_ = expression(mapper, discriminant)?;
    let cases_ = try_map_vec(cases, |case| switch_case(mapper, case))?.into();
    let comments_ = syntax_opt(mapper, comments.as_ref())?;
    Ok(ast::statement::Switch {
        discriminant: discriminant_,
        cases: cases_,
        comments: comments_,
        exhaustive_out: exhaustive_out_,
    })
}
fn switch_case<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    case: &ast::statement::switch::Case<M, T>,
) -> Result<ast::statement::switch::Case<N, U>, E> {
    let ast::statement::switch::Case {
        loc,
        test,
        case_test_loc,
        consequent,
        comments,
    } = case;
    let loc_ = mapper.on_loc_annot(loc)?;
    let test_ = test.as_ref().map(|t| expression(mapper, t)).transpose()?;
    let case_test_loc_ = case_test_loc
        .as_ref()
        .map(|l| mapper.on_loc_annot(l))
        .transpose()?;
    let consequent_ = statement_list(mapper, consequent)?;
    let comments_ = syntax_opt(mapper, comments.as_ref())?;
    Ok(ast::statement::switch::Case {
        loc: loc_,
        test: test_,
        case_test_loc: case_test_loc_,
        consequent: consequent_.into(),
        comments: comments_,
    })
}

pub fn throw<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    stmt: &ast::statement::Throw<M, T>,
) -> Result<ast::statement::Throw<N, U>, E> {
    let ast::statement::Throw { argument, comments } = stmt;
    let argument_ = expression(mapper, argument)?;
    let comments_ = syntax_opt(mapper, comments.as_ref())?;
    Ok(ast::statement::Throw {
        argument: argument_,
        comments: comments_,
    })
}

pub fn try_catch<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    stmt: &ast::statement::Try<M, T>,
) -> Result<ast::statement::Try<N, U>, E> {
    let ast::statement::Try {
        block,
        handler,
        finalizer,
        comments,
    } = stmt;
    let block_ = {
        let (annot, blk) = block;
        (mapper.on_loc_annot(annot)?, self::block(mapper, blk)?)
    };
    let handler_ = handler
        .as_ref()
        .map(|h| catch_clause(mapper, h))
        .transpose()?;
    let finalizer_ = finalizer
        .as_ref()
        .map(|(annot, blk)| Ok((mapper.on_loc_annot(annot)?, self::block(mapper, blk)?)))
        .transpose()?;
    let comments_ = syntax_opt(mapper, comments.as_ref())?;
    Ok(ast::statement::Try {
        block: block_,
        handler: handler_,
        finalizer: finalizer_,
        comments: comments_,
    })
}
fn catch_clause<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    clause: &ast::statement::try_::CatchClause<M, T>,
) -> Result<ast::statement::try_::CatchClause<N, U>, E> {
    let ast::statement::try_::CatchClause {
        loc,
        param,
        body,
        comments,
    } = clause;
    let loc_ = mapper.on_loc_annot(loc)?;
    let param_ = param
        .as_ref()
        .map(|p| catch_clause_pattern(mapper, p))
        .transpose()?;
    let body_ = {
        let (annot, blk) = body;
        (mapper.on_loc_annot(annot)?, catch_body(mapper, blk)?)
    };
    let comments_ = syntax_opt(mapper, comments.as_ref())?;
    Ok(ast::statement::try_::CatchClause {
        loc: loc_,
        param: param_,
        body: body_,
        comments: comments_,
    })
}
fn catch_body<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    body: &ast::statement::Block<M, T>,
) -> Result<ast::statement::Block<N, U>, E> {
    block(mapper, body)
}

pub fn binding_pattern<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    kind: ast::VariableKind,
    expr: &ast::pattern::Pattern<M, T>,
) -> Result<ast::pattern::Pattern<N, U>, E> {
    pattern(mapper, Some(kind), expr)
}
fn catch_clause_pattern<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    expr: &ast::pattern::Pattern<M, T>,
) -> Result<ast::pattern::Pattern<N, U>, E> {
    binding_pattern(mapper, ast::VariableKind::Let, expr)
}

pub fn variable_declaration<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    decl: &ast::statement::VariableDeclaration<M, T>,
) -> Result<ast::statement::VariableDeclaration<N, U>, E> {
    let ast::statement::VariableDeclaration {
        declarations,
        kind,
        comments,
    } = decl;
    let decls_ = try_map_vec(declarations, |d| variable_declarator(mapper, *kind, d))?;
    let comments_ = syntax_opt(mapper, comments.as_ref())?;
    Ok(ast::statement::VariableDeclaration {
        declarations: decls_.into(),
        kind: *kind,
        comments: comments_,
    })
}
fn variable_declarator<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    kind: ast::VariableKind,
    decl: &ast::statement::variable::Declarator<M, T>,
) -> Result<ast::statement::variable::Declarator<N, U>, E> {
    let ast::statement::variable::Declarator {
        loc,
        id,
        init,
        definite,
    } = decl;
    let loc_ = mapper.on_loc_annot(loc)?;
    let id_ = variable_declarator_pattern(mapper, kind, id)?;
    let init_ = init.as_ref().map(|i| expression(mapper, i)).transpose()?;
    Ok(ast::statement::variable::Declarator {
        loc: loc_,
        id: id_,
        init: init_,
        definite: *definite,
    })
}
fn variable_declarator_pattern<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    kind: ast::VariableKind,
    expr: &ast::pattern::Pattern<M, T>,
) -> Result<ast::pattern::Pattern<N, U>, E> {
    binding_pattern(mapper, kind, expr)
}

pub fn while_<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    stmt: &ast::statement::While<M, T>,
) -> Result<ast::statement::While<N, U>, E> {
    let ast::statement::While {
        test,
        body,
        comments,
    } = stmt;
    let test_ = predicate_expression(mapper, test)?;
    let body_ = statement(mapper, body)?;
    let comments_ = syntax_opt(mapper, comments.as_ref())?;
    Ok(ast::statement::While {
        test: test_,
        body: body_,
        comments: comments_,
    })
}

pub fn with_<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    stmt: &ast::statement::With<M, T>,
) -> Result<ast::statement::With<N, U>, E> {
    let ast::statement::With {
        object,
        body,
        comments,
    } = stmt;
    let object_ = expression(mapper, object)?;
    let body_ = statement(mapper, body)?;
    let comments_ = syntax_opt(mapper, comments.as_ref())?;
    Ok(ast::statement::With {
        object: object_,
        body: body_,
        comments: comments_,
    })
}

pub fn type_params<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    tparams: &ast::types::TypeParams<M, T>,
) -> Result<ast::types::TypeParams<N, U>, E> {
    let ast::types::TypeParams {
        loc,
        params,
        comments,
    } = tparams;
    let loc_ = mapper.on_loc_annot(loc)?;
    let params_ = try_map_vec(params, |tp| type_param(mapper, tp))?.into();
    let comments_ = syntax_with_internal_opt(mapper, comments.as_ref())?;
    Ok(ast::types::TypeParams {
        loc: loc_,
        params: params_,
        comments: comments_,
    })
}

pub fn type_args<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    targs: &ast::types::TypeArgs<M, T>,
) -> Result<ast::types::TypeArgs<N, U>, E> {
    let ast::types::TypeArgs {
        loc,
        arguments,
        comments,
    } = targs;
    let loc_ = mapper.on_loc_annot(loc)?;
    let arguments_ = try_map_vec(arguments, |t| type_(mapper, t))?.into();
    let comments_ = syntax_with_internal_opt(mapper, comments.as_ref())?;
    Ok(ast::types::TypeArgs {
        loc: loc_,
        arguments: arguments_,
        comments: comments_,
    })
}

pub fn type_param<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    tparam: &ast::types::TypeParam<M, T>,
) -> Result<ast::types::TypeParam<N, U>, E> {
    let ast::types::TypeParam {
        loc,
        name,
        bound,
        bound_kind,
        variance: var,
        default,
        const_,
    } = tparam;
    let name_ = type_param_identifier(mapper, name)?;
    let bound_ = type_annotation_hint(mapper, bound)?;
    let variance_ = var.as_ref().map(|v| variance(mapper, v)).transpose()?;
    let default_ = default.as_ref().map(|d| type_(mapper, d)).transpose()?;
    let const__ = const_
        .as_ref()
        .map(|c| tparam_const_modifier(mapper, c))
        .transpose()?;
    Ok(ast::types::TypeParam {
        loc: mapper.on_loc_annot(loc)?,
        name: name_,
        bound: bound_,
        bound_kind: bound_kind.clone(),
        variance: variance_,
        default: default_,
        const_: const__,
    })
}
fn type_param_identifier<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    id: &ast::Identifier<M, T>,
) -> Result<ast::Identifier<N, U>, E> {
    t_identifier(mapper, id)
}

pub fn type_annotation_hint<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    return_: &ast::types::AnnotationOrHint<M, T>,
) -> Result<ast::types::AnnotationOrHint<N, U>, E> {
    use ast::types::AnnotationOrHint;
    Ok(match return_ {
        AnnotationOrHint::Available(annot) => {
            AnnotationOrHint::Available(type_annotation(mapper, annot)?)
        }
        AnnotationOrHint::Missing(loc) => AnnotationOrHint::Missing(mapper.on_type_annot(loc)?),
    })
}
fn tparam_const_modifier<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    c: &ast::types::type_param::ConstModifier<M>,
) -> Result<ast::types::type_param::ConstModifier<N>, E> {
    let ast::types::type_param::ConstModifier { loc, comments } = c;
    let loc_ = mapper.on_loc_annot(loc)?;
    let comments_ = syntax_opt(mapper, comments.as_ref())?;
    Ok(ast::types::type_param::ConstModifier {
        loc: loc_,
        comments: comments_,
    })
}

pub fn variance<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    variance: &ast::Variance<M>,
) -> Result<ast::Variance<N>, E> {
    let ast::Variance {
        loc,
        kind,
        comments,
    } = variance;
    let loc_ = mapper.on_loc_annot(loc)?;
    let comments_ = syntax_opt(mapper, comments.as_ref())?;
    Ok(ast::Variance {
        loc: loc_,
        kind: kind.clone(),
        comments: comments_,
    })
}
fn type_guard<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    guard: &ast::types::TypeGuard<M, T>,
) -> Result<ast::types::TypeGuard<N, U>, E> {
    let ast::types::TypeGuard {
        loc,
        kind,
        guard: (x, t),
        comments,
    } = guard;
    let loc_ = mapper.on_loc_annot(loc)?;
    let x_ = identifier(mapper, x)?;
    let t_ = t.as_ref().map(|ty| type_(mapper, ty)).transpose()?;
    let comments_ = syntax_with_internal_opt(mapper, comments.as_ref())?;
    Ok(ast::types::TypeGuard {
        loc: loc_,
        kind: kind.clone(),
        guard: (x_, t_),
        comments: comments_,
    })
}
fn function_param_type<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    param: &ast::types::function::Param<M, T>,
) -> Result<ast::types::function::Param<N, U>, E> {
    let loc_ = mapper.on_loc_annot(&param.loc)?;
    let param_ = match &param.param {
        ast::types::function::ParamKind::Anonymous(annot) => {
            let annot_ = type_(mapper, annot)?;
            ast::types::function::ParamKind::Anonymous(annot_)
        }
        ast::types::function::ParamKind::Labeled {
            name,
            annot,
            optional,
        } => {
            let name_ = function_param_type_identifier(mapper, name)?;
            let annot_ = type_(mapper, annot)?;
            ast::types::function::ParamKind::Labeled {
                name: name_,
                annot: annot_,
                optional: *optional,
            }
        }
        ast::types::function::ParamKind::Destructuring(patt) => {
            let patt_ = function_param_type_pattern(mapper, patt)?;
            ast::types::function::ParamKind::Destructuring(patt_)
        }
    };
    Ok(ast::types::function::Param {
        loc: loc_,
        param: param_,
    })
}

fn function_param_type_identifier<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    id: &ast::Identifier<M, T>,
) -> Result<ast::Identifier<N, U>, E> {
    t_identifier(mapper, id)
}

fn function_param_type_pattern<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    patt: &ast::pattern::Pattern<M, T>,
) -> Result<ast::pattern::Pattern<N, U>, E> {
    pattern(mapper, None, patt)
}
fn function_rest_param_type<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    param: &ast::types::function::RestParam<M, T>,
) -> Result<ast::types::function::RestParam<N, U>, E> {
    let ast::types::function::RestParam {
        loc,
        argument,
        comments,
    } = param;
    let loc_ = mapper.on_loc_annot(loc)?;
    let argument_ = function_param_type(mapper, argument)?;
    let comments_ = syntax_opt(mapper, comments.as_ref())?;
    Ok(ast::types::function::RestParam {
        loc: loc_,
        argument: argument_,
        comments: comments_,
    })
}
fn function_this_param_type<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    param: &ast::types::function::ThisParam<M, T>,
) -> Result<ast::types::function::ThisParam<N, U>, E> {
    let ast::types::function::ThisParam {
        loc,
        annot,
        comments,
    } = param;
    let loc_ = mapper.on_loc_annot(loc)?;
    let annot_ = type_annotation(mapper, annot)?;
    let comments_ = syntax_opt(mapper, comments.as_ref())?;
    Ok(ast::types::function::ThisParam {
        loc: loc_,
        annot: annot_,
        comments: comments_,
    })
}
pub fn function_type_return_annotation<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    ret_annot: &ast::types::function::ReturnAnnotation<M, T>,
) -> Result<ast::types::function::ReturnAnnotation<N, U>, E> {
    use ast::types::function::ReturnAnnotation;
    Ok(match ret_annot {
        ReturnAnnotation::Missing(loc) => ReturnAnnotation::Missing(mapper.on_loc_annot(loc)?),
        ReturnAnnotation::Available(t) => ReturnAnnotation::Available(type_annotation(mapper, t)?),
        ReturnAnnotation::TypeGuard(g) => ReturnAnnotation::TypeGuard(type_guard(mapper, g)?),
    })
}
fn function_type<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    ft: &ast::types::Function<M, T>,
) -> Result<ast::types::Function<N, U>, E> {
    let ast::types::Function {
        tparams,
        params,
        return_,
        comments: func_comments,
        effect,
    } = ft;
    let ast::types::function::Params {
        loc: params_annot,
        this: this_,
        params: ps,
        rest: rpo,
        comments: params_comments,
    } = params;
    let tparams_ = tparams
        .as_ref()
        .map(|tp| type_params(mapper, tp))
        .transpose()?;
    let this__ = this_
        .as_ref()
        .map(|t| function_this_param_type(mapper, t))
        .transpose()?;
    let ps_ = try_map_vec(ps, |p| function_param_type(mapper, p))?.into();
    let rpo_ = rpo
        .as_ref()
        .map(|r| function_rest_param_type(mapper, r))
        .transpose()?;
    let return__ = function_type_return_annotation(mapper, return_)?;
    let func_comments_ = syntax_opt(mapper, func_comments.as_ref())?;
    let params_comments_ = syntax_with_internal_opt(mapper, params_comments.as_ref())?;
    Ok(ast::types::Function {
        tparams: tparams_,
        params: ast::types::function::Params {
            loc: mapper.on_loc_annot(params_annot)?,
            this: this__,
            params: ps_,
            rest: rpo_,
            comments: params_comments_,
        },
        return_: return__,
        comments: func_comments_,
        effect: effect.clone(),
    })
}
fn component_param_name<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    name: &ast::statement::component_params::ParamName<M, T>,
) -> Result<ast::statement::component_params::ParamName<N, U>, E> {
    use ast::statement::component_params::ParamName;
    Ok(match name {
        ParamName::Identifier(ident) => ParamName::Identifier(t_identifier(mapper, ident)?),
        ParamName::StringLiteral((annot, str)) => {
            ParamName::StringLiteral((mapper.on_loc_annot(annot)?, string_literal(mapper, str)?))
        }
    })
}
fn component_renders_annotation<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    renders: &ast::types::ComponentRendersAnnotation<M, T>,
) -> Result<ast::types::ComponentRendersAnnotation<N, U>, E> {
    use ast::types::ComponentRendersAnnotation;
    Ok(match renders {
        ComponentRendersAnnotation::AvailableRenders(loc, r) => {
            ComponentRendersAnnotation::AvailableRenders(
                mapper.on_loc_annot(loc)?,
                renders_type(mapper, r)?,
            )
        }
        ComponentRendersAnnotation::MissingRenders(loc) => {
            ComponentRendersAnnotation::MissingRenders(mapper.on_type_annot(loc)?)
        }
    })
}
fn component_type_param<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    param: &ast::types::component_params::Param<M, T>,
) -> Result<ast::types::component_params::Param<N, U>, E> {
    let ast::types::component_params::Param {
        loc,
        name,
        annot,
        optional,
    } = param;
    let name_ = component_param_name(mapper, name)?;
    let annot_ = type_annotation(mapper, annot)?;
    Ok(ast::types::component_params::Param {
        loc: mapper.on_type_annot(loc)?,
        name: name_,
        annot: annot_,
        optional: *optional,
    })
}
fn component_type_rest_param<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    param: &ast::types::component_params::RestParam<M, T>,
) -> Result<ast::types::component_params::RestParam<N, U>, E> {
    let ast::types::component_params::RestParam {
        loc,
        argument,
        annot,
        optional,
        comments,
    } = param;
    let argument_ = argument
        .as_ref()
        .map(|a| t_identifier(mapper, a))
        .transpose()?;
    let annot_ = type_(mapper, annot)?;
    let comments_ = syntax_opt(mapper, comments.as_ref())?;
    Ok(ast::types::component_params::RestParam {
        loc: mapper.on_type_annot(loc)?,
        argument: argument_,
        annot: annot_,
        optional: *optional,
        comments: comments_,
    })
}
fn component_type_params<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    params: &ast::types::component_params::Params<M, T>,
) -> Result<ast::types::component_params::Params<N, U>, E> {
    let ast::types::component_params::Params {
        loc,
        params: params_list,
        rest,
        comments,
    } = params;
    let params_list_ = try_map_vec(params_list, |p| component_type_param(mapper, p))?;
    let rest_ = rest
        .as_ref()
        .map(|r| component_type_rest_param(mapper, r))
        .transpose()?;
    let comments_ = syntax_with_internal_opt(mapper, comments.as_ref())?;
    Ok(ast::types::component_params::Params {
        loc: mapper.on_type_annot(loc)?,
        params: params_list_.into(),
        rest: rest_,
        comments: comments_,
    })
}
fn component_type<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    t: &ast::types::Component<M, T>,
) -> Result<ast::types::Component<N, U>, E> {
    let ast::types::Component {
        tparams,
        params,
        renders,
        comments,
    } = t;
    let tparams_ = tparams
        .as_ref()
        .map(|tp| type_params(mapper, tp))
        .transpose()?;
    let params_ = component_type_params(mapper, params)?;
    let renders_ = component_renders_annotation(mapper, renders)?;
    let comments_ = syntax_opt(mapper, comments.as_ref())?;
    Ok(ast::types::Component {
        tparams: tparams_,
        params: params_,
        renders: renders_,
        comments: comments_,
    })
}
fn object_property_value_type<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    opvt: &ast::types::object::PropertyValue<M, T>,
) -> Result<ast::types::object::PropertyValue<N, U>, E> {
    use ast::types::object::PropertyValue;
    Ok(match opvt {
        PropertyValue::Init(Some(t)) => PropertyValue::Init(Some(type_(mapper, t)?)),
        PropertyValue::Init(None) => PropertyValue::Init(None),
        PropertyValue::Get(annot, ft) => {
            PropertyValue::Get(mapper.on_loc_annot(annot)?, function_type(mapper, ft)?)
        }
        PropertyValue::Set(annot, ft) => {
            PropertyValue::Set(mapper.on_loc_annot(annot)?, function_type(mapper, ft)?)
        }
    })
}
pub fn object_property_type<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    prop: &ast::types::object::NormalProperty<M, T>,
) -> Result<ast::types::object::NormalProperty<N, U>, E> {
    let ast::types::object::NormalProperty {
        loc,
        key,
        value,
        optional,
        static_,
        proto,
        method,
        abstract_,
        override_,
        variance: var,
        ts_accessibility,
        init,
        comments,
    } = prop;
    let key_ = object_key(mapper, key)?;
    let value_ = object_property_value_type(mapper, value)?;
    let variance_ = var.as_ref().map(|v| variance(mapper, v)).transpose()?;
    let ts_accessibility_ = ts_accessibility
        .as_ref()
        .map(|tsa| {
            Ok(ast::class::ts_accessibility::TSAccessibility {
                loc: mapper.on_loc_annot(&tsa.loc)?,
                kind: tsa.kind,
                comments: syntax_opt(mapper, tsa.comments.as_ref())?,
            })
        })
        .transpose()?;
    let init_ = init.as_ref().map(|i| expression(mapper, i)).transpose()?;
    let comments_ = syntax_opt(mapper, comments.as_ref())?;
    Ok(ast::types::object::NormalProperty {
        loc: mapper.on_loc_annot(loc)?,
        key: key_,
        value: value_,
        optional: *optional,
        static_: *static_,
        proto: *proto,
        method: *method,
        abstract_: *abstract_,
        override_: *override_,
        variance: variance_,
        ts_accessibility: ts_accessibility_,
        init: init_,
        comments: comments_,
    })
}
fn object_indexer_property_type<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    indexer: &ast::types::object::Indexer<M, T>,
) -> Result<ast::types::object::Indexer<N, U>, E> {
    let ast::types::object::Indexer {
        loc,
        id,
        key,
        value,
        static_,
        variance: var,
        optional,
        comments,
    } = indexer;
    let id_ = id.as_ref().map(|i| t_identifier(mapper, i)).transpose()?;
    let key_ = type_(mapper, key)?;
    let value_ = type_(mapper, value)?;
    let variance_ = var.as_ref().map(|v| variance(mapper, v)).transpose()?;
    let comments_ = syntax_opt(mapper, comments.as_ref())?;
    Ok(ast::types::object::Indexer {
        loc: mapper.on_loc_annot(loc)?,
        id: id_,
        key: key_,
        value: value_,
        static_: *static_,
        variance: variance_,
        optional: *optional,
        comments: comments_,
    })
}
fn object_mapped_type<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    mapped: &ast::types::object::MappedType<M, T>,
) -> Result<ast::types::object::MappedType<N, U>, E> {
    let ast::types::object::MappedType {
        loc,
        key_tparam,
        prop_type,
        source_type,
        name_type,
        variance: var,
        variance_op,
        optional,
        comments,
    } = mapped;
    let source_type_ = type_(mapper, source_type)?;
    let key_tparam_ = type_param(mapper, key_tparam)?;
    let prop_type_ = type_(mapper, prop_type)?;
    let name_type_ = name_type.as_ref().map(|t| type_(mapper, t)).transpose()?;
    let variance_ = var.as_ref().map(|v| variance(mapper, v)).transpose()?;
    let comments_ = comments.as_ref().map(|c| syntax(mapper, c)).transpose()?;
    Ok(ast::types::object::MappedType {
        loc: mapper.on_loc_annot(loc)?,
        key_tparam: key_tparam_,
        prop_type: prop_type_,
        source_type: source_type_,
        name_type: name_type_,
        variance: variance_,
        variance_op: *variance_op,
        optional: optional.clone(),
        comments: comments_,
    })
}
fn object_private_field_type<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    pf: &ast::types::object::PrivateField<M>,
) -> Result<ast::types::object::PrivateField<N>, E> {
    let ast::types::object::PrivateField { loc, key, comments } = pf;
    let loc_ = mapper.on_loc_annot(loc)?;
    let key_ = private_name(mapper, key)?;
    let comments_ = syntax_opt(mapper, comments.as_ref())?;
    Ok(ast::types::object::PrivateField {
        loc: loc_,
        key: key_,
        comments: comments_,
    })
}

fn object_internal_slot_property_type<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    slot: &ast::types::object::InternalSlot<M, T>,
) -> Result<ast::types::object::InternalSlot<N, U>, E> {
    let ast::types::object::InternalSlot {
        loc,
        id,
        value,
        optional,
        static_,
        method,
        comments,
    } = slot;
    let id_ = identifier(mapper, id)?;
    let value_ = type_(mapper, value)?;
    let comments_ = syntax_opt(mapper, comments.as_ref())?;
    Ok(ast::types::object::InternalSlot {
        loc: mapper.on_loc_annot(loc)?,
        id: id_,
        value: value_,
        optional: *optional,
        static_: *static_,
        method: *method,
        comments: comments_,
    })
}
fn object_type<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    ot: &ast::types::Object<M, T>,
) -> Result<ast::types::Object<N, U>, E> {
    let ast::types::Object {
        exact,
        inexact,
        properties,
        comments,
    } = ot;
    let properties_ = try_map_vec(properties, |p| object_type_property(mapper, p))?;
    let comments_ = syntax_with_internal_opt(mapper, comments.as_ref())?;
    Ok(ast::types::Object {
        exact: *exact,
        inexact: *inexact,
        properties: properties_.into(),
        comments: comments_,
    })
}
pub fn object_type_property<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    prop: &ast::types::object::Property<M, T>,
) -> Result<ast::types::object::Property<N, U>, E> {
    use ast::types::object::Property;
    Ok(match prop {
        Property::NormalProperty(p) => Property::NormalProperty(object_property_type(mapper, p)?),
        Property::SpreadProperty(p) => {
            Property::SpreadProperty(object_spread_property_type(mapper, p)?)
        }
        Property::Indexer(i) => Property::Indexer(object_indexer_property_type(mapper, i)?),
        Property::CallProperty(c) => Property::CallProperty(object_call_property_type(mapper, c)?),
        Property::MappedType(m) => Property::MappedType(object_mapped_type(mapper, m)?),
        Property::InternalSlot(s) => {
            Property::InternalSlot(object_internal_slot_property_type(mapper, s)?)
        }
        Property::PrivateField(pf) => {
            Property::PrivateField(object_private_field_type(mapper, pf)?)
        }
    })
}
fn object_spread_property_type<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    prop: &ast::types::object::SpreadProperty<M, T>,
) -> Result<ast::types::object::SpreadProperty<N, U>, E> {
    let ast::types::object::SpreadProperty {
        loc,
        argument,
        comments,
    } = prop;
    let loc_ = mapper.on_loc_annot(loc)?;
    let argument_ = type_(mapper, argument)?;
    let comments_ = syntax_opt(mapper, comments.as_ref())?;
    Ok(ast::types::object::SpreadProperty {
        loc: loc_,
        argument: argument_,
        comments: comments_,
    })
}
fn object_call_property_type<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    call: &ast::types::object::CallProperty<M, T>,
) -> Result<ast::types::object::CallProperty<N, U>, E> {
    let ast::types::object::CallProperty {
        loc,
        value: (value_annot, value_ft),
        static_,
        comments,
    } = call;
    let loc_ = mapper.on_loc_annot(loc)?;
    let value_ = (
        mapper.on_loc_annot(value_annot)?,
        function_type(mapper, value_ft)?,
    );
    let comments_ = syntax_opt(mapper, comments.as_ref())?;
    Ok(ast::types::object::CallProperty {
        loc: loc_,
        value: value_,
        static_: *static_,
        comments: comments_,
    })
}
fn interface_type<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    i: &ast::types::Interface<M, T>,
) -> Result<ast::types::Interface<N, U>, E> {
    let ast::types::Interface {
        body: (body_annot, body_obj),
        extends,
        comments,
    } = i;
    let extends_ = try_map_vec(extends, |(annot, gt)| {
        Ok((mapper.on_loc_annot(annot)?, generic_type(mapper, gt)?))
    })?
    .into();
    let body_ = (
        mapper.on_loc_annot(body_annot)?,
        object_type(mapper, body_obj)?,
    );
    let comments_ = syntax_opt(mapper, comments.as_ref())?;
    Ok(ast::types::Interface {
        body: body_,
        extends: extends_,
        comments: comments_,
    })
}

pub fn nullable_type<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    t: &ast::types::Nullable<M, T>,
) -> Result<ast::types::Nullable<N, U>, E> {
    let ast::types::Nullable { argument, comments } = t;
    let argument_ = type_(mapper, argument)?;
    let comments_ = syntax_opt(mapper, comments.as_ref())?;
    Ok(ast::types::Nullable {
        argument: argument_,
        comments: comments_,
    })
}

pub fn typeof_type<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    t: &ast::types::Typeof<M, T>,
) -> Result<ast::types::Typeof<N, U>, E> {
    let ast::types::Typeof {
        argument,
        targs,
        comments,
    } = t;
    let argument_ = typeof_expression(mapper, argument)?;
    let targs_ = targs.as_ref().map(|ta| type_args(mapper, ta)).transpose()?;
    let comments_ = syntax_opt(mapper, comments.as_ref())?;
    Ok(ast::types::Typeof {
        argument: argument_,
        targs: targs_,
        comments: comments_,
    })
}
pub fn typeof_expression<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    target: &ast::types::typeof_::Target<M, T>,
) -> Result<ast::types::typeof_::Target<N, U>, E> {
    use ast::types::typeof_::Target;
    Ok(match target {
        Target::Unqualified(i) => Target::Unqualified(typeof_identifier(mapper, i)?),
        Target::Qualified(q) => {
            Target::Qualified(Arc::new(typeof_qualified_identifier(mapper, q)?))
        }
        Target::Import(it) => {
            let loc_ = mapper.on_type_annot(&it.loc)?;
            let source_ = {
                let (loc, lit) = &it.source;
                (mapper.on_loc_annot(loc)?, string_literal(mapper, lit)?)
            };
            let comments_ = syntax_opt(mapper, it.comments.as_ref())?;
            Target::Import(Arc::new(ast::types::generic::ImportType {
                loc: loc_,
                source: source_,
                comments: comments_,
            }))
        }
    })
}
fn typeof_identifier<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    id: &ast::Identifier<M, T>,
) -> Result<ast::Identifier<N, U>, E> {
    t_identifier(mapper, id)
}
fn typeof_member_identifier<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    id: &ast::Identifier<M, T>,
) -> Result<ast::Identifier<N, U>, E> {
    t_identifier(mapper, id)
}
fn typeof_qualified_identifier<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    qual: &ast::types::typeof_::Qualified<M, T>,
) -> Result<ast::types::typeof_::Qualified<N, U>, E> {
    let ast::types::typeof_::Qualified {
        loc,
        qualification,
        id,
    } = qual;
    let qualification_ = typeof_expression(mapper, qualification)?;
    let id_ = typeof_member_identifier(mapper, id)?;
    Ok(ast::types::typeof_::Qualified {
        loc: mapper.on_type_annot(loc)?,
        qualification: qualification_,
        id: id_,
    })
}

pub fn keyof_type<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    t: &ast::types::Keyof<M, T>,
) -> Result<ast::types::Keyof<N, U>, E> {
    let ast::types::Keyof { argument, comments } = t;
    let argument_ = type_(mapper, argument)?;
    let comments_ = syntax_opt(mapper, comments.as_ref())?;
    Ok(ast::types::Keyof {
        argument: argument_,
        comments: comments_,
    })
}

pub fn renders_type<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    t: &ast::types::Renders<M, T>,
) -> Result<ast::types::Renders<N, U>, E> {
    let ast::types::Renders {
        operator_loc,
        argument,
        comments,
        variant,
    } = t;
    let argument_ = type_(mapper, argument)?;
    let comments_ = syntax_opt(mapper, comments.as_ref())?;
    Ok(ast::types::Renders {
        operator_loc: mapper.on_loc_annot(operator_loc)?,
        argument: argument_,
        comments: comments_,
        variant: *variant,
    })
}

pub fn readonly_type<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    t: &ast::types::ReadOnly<M, T>,
) -> Result<ast::types::ReadOnly<N, U>, E> {
    let ast::types::ReadOnly { argument, comments } = t;
    let argument_ = type_(mapper, argument)?;
    let comments_ = syntax_opt(mapper, comments.as_ref())?;
    Ok(ast::types::ReadOnly {
        argument: argument_,
        comments: comments_,
    })
}

pub fn array_type<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    t: &ast::types::Array<M, T>,
) -> Result<ast::types::Array<N, U>, E> {
    let ast::types::Array { argument, comments } = t;
    let argument_ = type_(mapper, argument)?;
    let comments_ = syntax_opt(mapper, comments.as_ref())?;
    Ok(ast::types::Array {
        argument: argument_,
        comments: comments_,
    })
}

pub fn conditional_type<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    t: &ast::types::Conditional<M, T>,
) -> Result<ast::types::Conditional<N, U>, E> {
    let ast::types::Conditional {
        check_type,
        extends_type,
        true_type,
        false_type,
        comments,
    } = t;
    let check_type_ = type_(mapper, check_type)?;
    let extends_type_ = type_(mapper, extends_type)?;
    let true_type_ = type_(mapper, true_type)?;
    let false_type_ = type_(mapper, false_type)?;
    let comments_ = comments.as_ref().map(|c| syntax(mapper, c)).transpose()?;
    Ok(ast::types::Conditional {
        check_type: check_type_,
        extends_type: extends_type_,
        true_type: true_type_,
        false_type: false_type_,
        comments: comments_,
    })
}

pub fn infer_type<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    t: &ast::types::Infer<M, T>,
) -> Result<ast::types::Infer<N, U>, E> {
    let ast::types::Infer { tparam, comments } = t;
    let tparam_ = type_param(mapper, tparam)?;
    let comments_ = comments.as_ref().map(|c| syntax(mapper, c)).transpose()?;
    Ok(ast::types::Infer {
        tparam: tparam_,
        comments: comments_,
    })
}

pub fn union_type<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    t: &ast::types::Union<M, T>,
) -> Result<ast::types::Union<N, U>, E> {
    let ast::types::Union { types, comments } = t;
    let (t0, t1, ts) = types;
    let t0_ = type_(mapper, t0)?;
    let t1_ = type_(mapper, t1)?;
    let ts_ = try_map_vec(ts, |ty| type_(mapper, ty))?;
    let comments_ = syntax_opt(mapper, comments.as_ref())?;
    Ok(ast::types::Union {
        types: (t0_, t1_, ts_),
        comments: comments_,
    })
}

pub fn intersection_type<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    t: &ast::types::Intersection<M, T>,
) -> Result<ast::types::Intersection<N, U>, E> {
    let ast::types::Intersection { types, comments } = t;
    let (t0, t1, ts) = types;
    let t0_ = type_(mapper, t0)?;
    let t1_ = type_(mapper, t1)?;
    let ts_ = try_map_vec(ts, |ty| type_(mapper, ty))?;
    let comments_ = syntax_opt(mapper, comments.as_ref())?;
    Ok(ast::types::Intersection {
        types: (t0_, t1_, ts_),
        comments: comments_,
    })
}

pub fn tuple_type<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    t: &ast::types::Tuple<M, T>,
) -> Result<ast::types::Tuple<N, U>, E> {
    let ast::types::Tuple {
        elements,
        inexact,
        comments,
    } = t;
    let elements_ = try_map_vec(elements, |e| tuple_element(mapper, e))?.into();
    let comments_ = syntax_opt(mapper, comments.as_ref())?;
    Ok(ast::types::Tuple {
        elements: elements_,
        inexact: *inexact,
        comments: comments_,
    })
}
pub fn tuple_element<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    element: &ast::types::tuple::Element<M, T>,
) -> Result<ast::types::tuple::Element<N, U>, E> {
    use ast::types::tuple::Element;
    Ok(match element {
        Element::UnlabeledElement {
            loc,
            annot: ty,
            optional,
        } => Element::UnlabeledElement {
            loc: mapper.on_loc_annot(loc)?,
            annot: type_(mapper, ty)?,
            optional: *optional,
        },
        Element::LabeledElement { loc, element } => Element::LabeledElement {
            loc: mapper.on_loc_annot(loc)?,
            element: tuple_labeled_element(mapper, element)?,
        },
        Element::SpreadElement { loc, element } => Element::SpreadElement {
            loc: mapper.on_loc_annot(loc)?,
            element: tuple_spread_element(mapper, element)?,
        },
    })
}
fn tuple_labeled_element<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    t: &ast::types::tuple::LabeledElement<M, T>,
) -> Result<ast::types::tuple::LabeledElement<N, U>, E> {
    let ast::types::tuple::LabeledElement {
        annot,
        name,
        variance: var,
        optional,
    } = t;
    let annot_ = type_(mapper, annot)?;
    let name_ = t_identifier(mapper, name)?;
    let variance_ = var.as_ref().map(|v| variance(mapper, v)).transpose()?;
    Ok(ast::types::tuple::LabeledElement {
        annot: annot_,
        name: name_,
        variance: variance_,
        optional: *optional,
    })
}
fn tuple_spread_element<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    t: &ast::types::tuple::SpreadElement<M, T>,
) -> Result<ast::types::tuple::SpreadElement<N, U>, E> {
    let ast::types::tuple::SpreadElement { annot, name } = t;
    let annot_ = type_(mapper, annot)?;
    let name_ = name.as_ref().map(|n| t_identifier(mapper, n)).transpose()?;
    Ok(ast::types::tuple::SpreadElement {
        annot: annot_,
        name: name_,
    })
}

pub fn generic_type<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    gt: &ast::types::Generic<M, T>,
) -> Result<ast::types::Generic<N, U>, E> {
    let ast::types::Generic {
        id,
        targs,
        comments,
    } = gt;
    let id_ = generic_identifier_type(mapper, id)?;
    let targs_ = targs.as_ref().map(|ta| type_args(mapper, ta)).transpose()?;
    let comments_ = syntax_opt(mapper, comments.as_ref())?;
    Ok(ast::types::Generic {
        id: id_,
        targs: targs_,
        comments: comments_,
    })
}
fn generic_identifier_type<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    git: &ast::types::generic::Identifier<M, T>,
) -> Result<ast::types::generic::Identifier<N, U>, E> {
    use ast::types::generic::Identifier;
    Ok(match git {
        Identifier::Unqualified(i) => {
            Identifier::Unqualified(type_identifier_reference(mapper, i)?)
        }
        Identifier::Qualified(q) => {
            Identifier::Qualified(Arc::new(generic_qualified_identifier_type(mapper, q)?))
        }
        Identifier::ImportTypeAnnot(import_type) => {
            let loc_ = mapper.on_type_annot(&import_type.loc)?;
            let source_ = {
                let (loc, lit) = &import_type.source;
                (mapper.on_loc_annot(loc)?, string_literal(mapper, lit)?)
            };
            let comments_ = syntax_opt(mapper, import_type.comments.as_ref())?;
            Identifier::ImportTypeAnnot(Arc::new(ast::types::generic::ImportType {
                loc: loc_,
                source: source_,
                comments: comments_,
            }))
        }
    })
}
fn generic_qualified_identifier_type<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    qual: &ast::types::generic::Qualified<M, T>,
) -> Result<ast::types::generic::Qualified<N, U>, E> {
    let ast::types::generic::Qualified {
        loc,
        qualification,
        id,
    } = qual;
    let qualification_ = generic_identifier_type(mapper, qualification)?;
    let id_ = member_type_identifier(mapper, id)?;
    Ok(ast::types::generic::Qualified {
        loc: mapper.on_loc_annot(loc)?,
        qualification: qualification_,
        id: id_,
    })
}
fn member_type_identifier<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    id: &ast::Identifier<M, T>,
) -> Result<ast::Identifier<N, U>, E> {
    t_identifier(mapper, id)
}

pub fn indexed_access_type<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    ia: &ast::types::IndexedAccess<M, T>,
) -> Result<ast::types::IndexedAccess<N, U>, E> {
    let ast::types::IndexedAccess {
        object,
        index,
        comments,
    } = ia;
    let object_ = type_(mapper, object)?;
    let index_ = type_(mapper, index)?;
    let comments_ = syntax_opt(mapper, comments.as_ref())?;
    Ok(ast::types::IndexedAccess {
        object: object_,
        index: index_,
        comments: comments_,
    })
}

pub fn optional_indexed_access_type<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    ia: &ast::types::OptionalIndexedAccess<M, T>,
) -> Result<ast::types::OptionalIndexedAccess<N, U>, E> {
    let ast::types::OptionalIndexedAccess {
        indexed_access,
        optional,
    } = ia;
    let indexed_access_ = indexed_access_type(mapper, indexed_access)?;
    Ok(ast::types::OptionalIndexedAccess {
        indexed_access: indexed_access_,
        optional: *optional,
    })
}

pub fn predicate<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    pred: &ast::types::Predicate<M, T>,
) -> Result<ast::types::Predicate<N, U>, E> {
    let ast::types::Predicate {
        loc: annot,
        kind,
        comments,
    } = pred;
    let annot_ = mapper.on_loc_annot(annot)?;
    let kind_ = match kind {
        ast::types::PredicateKind::Declared(e) => {
            ast::types::PredicateKind::Declared(Arc::new(expression(mapper, e)?))
        }
        ast::types::PredicateKind::Inferred => ast::types::PredicateKind::Inferred,
    };
    let comments_ = syntax_opt(mapper, comments.as_ref())?;
    Ok(ast::types::Predicate {
        loc: annot_,
        kind: kind_,
        comments: comments_,
    })
}

pub fn type_<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    ty: &ast::types::Type<M, T>,
) -> Result<ast::types::Type<N, U>, E> {
    use ast::types::TypeInner;
    Ok(ast::types::Type::new(match ty.deref() {
        TypeInner::Any { loc, comments } => TypeInner::Any {
            loc: mapper.on_type_annot(loc)?,
            comments: syntax_opt(mapper, comments.as_ref())?,
        },
        TypeInner::Mixed { loc, comments } => TypeInner::Mixed {
            loc: mapper.on_type_annot(loc)?,
            comments: syntax_opt(mapper, comments.as_ref())?,
        },
        TypeInner::Empty { loc, comments } => TypeInner::Empty {
            loc: mapper.on_type_annot(loc)?,
            comments: syntax_opt(mapper, comments.as_ref())?,
        },
        TypeInner::Void { loc, comments } => TypeInner::Void {
            loc: mapper.on_type_annot(loc)?,
            comments: syntax_opt(mapper, comments.as_ref())?,
        },
        TypeInner::Null { loc, comments } => TypeInner::Null {
            loc: mapper.on_type_annot(loc)?,
            comments: syntax_opt(mapper, comments.as_ref())?,
        },
        TypeInner::Symbol { loc, comments } => TypeInner::Symbol {
            loc: mapper.on_type_annot(loc)?,
            comments: syntax_opt(mapper, comments.as_ref())?,
        },
        TypeInner::Number { loc, comments } => TypeInner::Number {
            loc: mapper.on_type_annot(loc)?,
            comments: syntax_opt(mapper, comments.as_ref())?,
        },
        TypeInner::BigInt { loc, comments } => TypeInner::BigInt {
            loc: mapper.on_type_annot(loc)?,
            comments: syntax_opt(mapper, comments.as_ref())?,
        },
        TypeInner::String { loc, comments } => TypeInner::String {
            loc: mapper.on_type_annot(loc)?,
            comments: syntax_opt(mapper, comments.as_ref())?,
        },
        TypeInner::Boolean { loc, raw, comments } => TypeInner::Boolean {
            loc: mapper.on_type_annot(loc)?,
            raw: raw.clone(),
            comments: syntax_opt(mapper, comments.as_ref())?,
        },
        TypeInner::Exists { loc, comments } => TypeInner::Exists {
            loc: mapper.on_type_annot(loc)?,
            comments: syntax_opt(mapper, comments.as_ref())?,
        },
        TypeInner::Unknown { loc, comments } => TypeInner::Unknown {
            loc: mapper.on_type_annot(loc)?,
            comments: syntax_opt(mapper, comments.as_ref())?,
        },
        TypeInner::Never { loc, comments } => TypeInner::Never {
            loc: mapper.on_type_annot(loc)?,
            comments: syntax_opt(mapper, comments.as_ref())?,
        },
        TypeInner::Undefined { loc, comments } => TypeInner::Undefined {
            loc: mapper.on_type_annot(loc)?,
            comments: syntax_opt(mapper, comments.as_ref())?,
        },
        TypeInner::UniqueSymbol {
            loc,
            symbol_loc,
            comments,
        } => TypeInner::UniqueSymbol {
            loc: mapper.on_type_annot(loc)?,
            symbol_loc: mapper.on_loc_annot(symbol_loc)?,
            comments: syntax_opt(mapper, comments.as_ref())?,
        },
        TypeInner::Nullable { loc, inner } => TypeInner::Nullable {
            loc: mapper.on_type_annot(loc)?,
            inner: Arc::new(nullable_type(mapper, inner)?),
        },
        TypeInner::Array { loc, inner } => TypeInner::Array {
            loc: mapper.on_type_annot(loc)?,
            inner: Arc::new(array_type(mapper, inner)?),
        },
        TypeInner::Conditional { loc, inner } => TypeInner::Conditional {
            loc: mapper.on_type_annot(loc)?,
            inner: Arc::new(conditional_type(mapper, inner)?),
        },
        TypeInner::Infer { loc, inner } => TypeInner::Infer {
            loc: mapper.on_type_annot(loc)?,
            inner: Arc::new(infer_type(mapper, inner)?),
        },
        TypeInner::Typeof { loc, inner } => TypeInner::Typeof {
            loc: mapper.on_type_annot(loc)?,
            inner: Arc::new(typeof_type(mapper, inner)?),
        },
        TypeInner::Keyof { loc, inner } => TypeInner::Keyof {
            loc: mapper.on_type_annot(loc)?,
            inner: Arc::new(keyof_type(mapper, inner)?),
        },
        TypeInner::Renders { loc, inner } => TypeInner::Renders {
            loc: mapper.on_type_annot(loc)?,
            inner: Arc::new(renders_type(mapper, inner)?),
        },
        TypeInner::ReadOnly { loc, inner } => TypeInner::ReadOnly {
            loc: mapper.on_type_annot(loc)?,
            inner: Arc::new(readonly_type(mapper, inner)?),
        },
        TypeInner::Function { loc, inner } => TypeInner::Function {
            loc: mapper.on_type_annot(loc)?,
            inner: Arc::new(function_type(mapper, inner)?),
        },
        TypeInner::ConstructorType {
            loc,
            abstract_,
            inner,
        } => TypeInner::ConstructorType {
            loc: mapper.on_type_annot(loc)?,
            abstract_: *abstract_,
            inner: Arc::new(function_type(mapper, inner)?),
        },
        TypeInner::Component { loc, inner } => TypeInner::Component {
            loc: mapper.on_type_annot(loc)?,
            inner: Arc::new(component_type(mapper, inner)?),
        },
        TypeInner::Object { loc, inner } => TypeInner::Object {
            loc: mapper.on_type_annot(loc)?,
            inner: Arc::new(object_type(mapper, inner)?),
        },
        TypeInner::Interface { loc, inner } => TypeInner::Interface {
            loc: mapper.on_type_annot(loc)?,
            inner: Arc::new(interface_type(mapper, inner)?),
        },
        TypeInner::Generic { loc, inner } => TypeInner::Generic {
            loc: mapper.on_type_annot(loc)?,
            inner: Arc::new(generic_type(mapper, inner)?),
        },
        TypeInner::IndexedAccess { loc, inner } => TypeInner::IndexedAccess {
            loc: mapper.on_type_annot(loc)?,
            inner: Arc::new(indexed_access_type(mapper, inner)?),
        },
        TypeInner::OptionalIndexedAccess { loc, inner } => TypeInner::OptionalIndexedAccess {
            loc: mapper.on_type_annot(loc)?,
            inner: Arc::new(optional_indexed_access_type(mapper, inner)?),
        },
        TypeInner::Union { loc, inner } => TypeInner::Union {
            loc: mapper.on_type_annot(loc)?,
            inner: Arc::new(union_type(mapper, inner)?),
        },
        TypeInner::Intersection { loc, inner } => TypeInner::Intersection {
            loc: mapper.on_type_annot(loc)?,
            inner: Arc::new(intersection_type(mapper, inner)?),
        },
        TypeInner::Tuple { loc, inner } => TypeInner::Tuple {
            loc: mapper.on_type_annot(loc)?,
            inner: Arc::new(tuple_type(mapper, inner)?),
        },
        TypeInner::StringLiteral { loc, literal } => TypeInner::StringLiteral {
            loc: mapper.on_type_annot(loc)?,
            literal: string_literal(mapper, literal)?,
        },
        TypeInner::NumberLiteral { loc, literal } => TypeInner::NumberLiteral {
            loc: mapper.on_type_annot(loc)?,
            literal: number_literal(mapper, literal)?,
        },
        TypeInner::BigIntLiteral { loc, literal } => TypeInner::BigIntLiteral {
            loc: mapper.on_type_annot(loc)?,
            literal: bigint_literal(mapper, literal)?,
        },
        TypeInner::BooleanLiteral { loc, literal } => TypeInner::BooleanLiteral {
            loc: mapper.on_type_annot(loc)?,
            literal: boolean_literal(mapper, literal)?,
        },
        TypeInner::TemplateLiteral { loc, inner } => TypeInner::TemplateLiteral {
            loc: mapper.on_type_annot(loc)?,
            inner: Arc::new(template_literal_type(mapper, inner)?),
        },
    }))
}

pub fn type_alias<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    _loc: &M,
    decl: &ast::statement::TypeAlias<M, T>,
) -> Result<ast::statement::TypeAlias<N, U>, E> {
    let ast::statement::TypeAlias {
        id,
        tparams,
        right,
        comments,
    } = decl;
    let id_ = binding_type_identifier(mapper, id)?;
    let tparams_ = tparams
        .as_ref()
        .map(|tp| type_params(mapper, tp))
        .transpose()?;
    let right_ = type_(mapper, right)?;
    let comments_ = syntax_opt(mapper, comments.as_ref())?;
    Ok(ast::statement::TypeAlias {
        id: id_,
        tparams: tparams_,
        right: right_,
        comments: comments_,
    })
}

pub fn declare_opaque_type<M: Dupe, T: Dupe, N: Dupe, U: Dupe, E>(
    mapper: &mut impl LocMapper<M, T, N, U, E>,
    loc: &M,
    otype: &ast::statement::OpaqueType<M, T>,
) -> Result<ast::statement::OpaqueType<N, U>, E> {
    opaque_type(mapper, loc, otype)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::expression;
    use crate::ast_visitor::AstVisitor;
    use crate::loc::Loc;
    use crate::loc::Position;

    fn span(start: i32, end: i32) -> Loc {
        Loc {
            source: None,
            start: Position {
                line: 1,
                column: start,
            },
            end: Position {
                line: 1,
                column: end,
            },
        }
    }

    fn operand() -> expression::Expression<Loc, Loc> {
        expression::Expression::new(expression::ExpressionInner::This {
            loc: span(2, 6),
            inner: Arc::new(expression::This { comments: None }),
        })
    }

    fn asserted_call() -> expression::OptionalCall<Loc, Loc> {
        expression::OptionalCall {
            call: expression::Call {
                callee: operand(),
                targs: None,
                arguments: expression::ArgList {
                    loc: span(8, 10),
                    arguments: Arc::from([]),
                    comments: None,
                },
                comments: None,
            },
            optional: expression::OptionalCallKind::AssertNonnull {
                assertion_loc: span(0, 8),
            },
            filtered_out: span(0, 10),
        }
    }

    fn asserted_member() -> expression::OptionalMember<Loc, Loc> {
        expression::OptionalMember {
            member: expression::Member {
                object: operand(),
                property: expression::member::Property::PropertyExpression(operand()),
                comments: None,
            },
            optional: expression::OptionalMemberKind::AssertNonnull {
                assertion_loc: span(0, 8),
            },
            filtered_out: span(0, 16),
        }
    }

    struct ToTypedAnnotations;

    impl LocMapper<Loc, Loc, Arc<Loc>, (Loc, u8)> for ToTypedAnnotations {
        fn on_loc_annot(&mut self, loc: &Loc) -> Result<Arc<Loc>, !> {
            Ok(Arc::new(loc.dupe()))
        }

        fn on_type_annot(&mut self, loc: &Loc) -> Result<(Loc, u8), !> {
            Ok((loc.dupe(), 1))
        }
    }

    #[test]
    fn optional_call_assertion_loc_maps_as_syntax() {
        let call = asserted_call();
        let Ok(mapped) = optional_call(&mut ToTypedAnnotations, &span(0, 10), &call);
        assert_eq!(
            mapped.optional,
            expression::OptionalCallKind::AssertNonnull {
                assertion_loc: Arc::new(span(0, 8)),
            }
        );
        assert_eq!(mapped.call.callee.loc(), &(span(2, 6), 1));
        assert_eq!(mapped.filtered_out, (span(0, 10), 1));
    }

    #[test]
    fn optional_member_assertion_loc_maps_as_syntax() {
        let member = asserted_member();
        let Ok(mapped) = optional_member(&mut ToTypedAnnotations, &span(0, 16), &member);
        assert_eq!(
            mapped.optional,
            expression::OptionalMemberKind::AssertNonnull {
                assertion_loc: Arc::new(span(0, 8)),
            }
        );
        assert_eq!(mapped.member.object.loc(), &(span(2, 6), 1));
        assert_eq!(mapped.filtered_out, (span(0, 16), 1));
    }

    struct ShiftAssertionLoc;

    impl AstVisitor<'_, Loc> for ShiftAssertionLoc {
        fn normalize_loc(loc: &Loc) -> &Loc {
            loc
        }

        fn normalize_type(loc: &Loc) -> &Loc {
            loc
        }

        fn map_loc_annot(&mut self, loc: &Loc) -> Loc {
            span(loc.start.column + 20, loc.end.column + 20)
        }
    }

    #[test]
    fn homogeneous_mapping_maps_assertion_loc_and_preserves_filtered_out() {
        let call = asserted_call();
        let member = asserted_member();
        let loc = span(0, 16);
        let mapped_call = ShiftAssertionLoc.map_optional_call(&loc, &call);
        let mapped_member = ShiftAssertionLoc.map_optional_member(&loc, &member);
        assert_eq!(
            mapped_call.optional,
            expression::OptionalCallKind::AssertNonnull {
                assertion_loc: span(20, 28),
            }
        );
        assert_eq!(
            mapped_member.optional,
            expression::OptionalMemberKind::AssertNonnull {
                assertion_loc: span(20, 28),
            }
        );
        assert_eq!(mapped_call.filtered_out, call.filtered_out);
        assert_eq!(mapped_member.filtered_out, member.filtered_out);
    }
}
