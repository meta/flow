// (c) Meta Platforms, Inc. and affiliates. Confidential and proprietary.

use dupe::Dupe;
use flow_parser::ast::class;
use flow_parser::ast::expression::object;
use flow_parser::ast::function;
use flow_parser::ast::statement;
use flow_parser::ast::statement::StatementInner;
use flow_parser::ast::{self};
use flow_parser::loc::Loc;

use super::PrintError;
use super::Printed;
use super::Stmt;
use super::Writer;

impl Writer {
    pub(super) fn statements(&mut self, statements: &[Stmt]) -> Printed {
        for statement in statements {
            self.statement(statement)?;
            if !self.line_start {
                self.newline();
            }
        }
        Ok(())
    }

    fn statement(&mut self, stmt: &Stmt) -> Printed {
        self.at(stmt.loc(), |writer| match &**stmt {
            StatementInner::Block { inner, .. } => writer.block(inner),
            StatementInner::Expression { inner, .. } => {
                writer.commented(&inner.comments, |writer| {
                    if inner.directive.is_some() {
                        let ast::expression::ExpressionInner::StringLiteral {
                            inner: literal, ..
                        } = &*inner.expression
                        else {
                            return Err(PrintError::DirectiveShape(stmt.loc().dupe()));
                        };
                        writer.at(inner.expression.loc(), |writer| {
                            writer.leading(&literal.comments)?;
                            writer.token(&literal.raw)?;
                            writer.trailing(&literal.comments)
                        })?;
                    } else {
                        writer.fenced_expression(&inner.expression)?;
                    }
                    writer.token(";")
                })
            }
            StatementInner::VariableDeclaration { inner, .. } => {
                writer.variable(inner)?;
                writer.token(";")
            }
            StatementInner::FunctionDeclaration { inner, .. } => writer.function(inner),
            StatementInner::ClassDeclaration { inner, .. } => writer.class(inner),
            StatementInner::Empty { inner, .. } => {
                writer.commented(&inner.comments, |writer| writer.token(";"))
            }
            StatementInner::Debugger { inner, .. } => {
                writer.commented(&inner.comments, |writer| writer.token("debugger;"))
            }
            StatementInner::Break { inner, .. } => writer.commented(&inner.comments, |writer| {
                writer.jump("break", inner.label.as_ref())
            }),
            StatementInner::Continue { inner, .. } => writer.commented(&inner.comments, |writer| {
                writer.jump("continue", inner.label.as_ref())
            }),
            StatementInner::Return { inner, .. } => writer.commented(&inner.comments, |writer| {
                writer.token("return")?;
                if let Some(argument) = &inner.argument {
                    writer.token(" ")?;
                    writer.fenced_expression(argument)?;
                }
                writer.token(";")
            }),
            StatementInner::Throw { inner, .. } => writer.commented(&inner.comments, |writer| {
                writer.token("throw ")?;
                writer.fenced_expression(&inner.argument)?;
                writer.token(";")
            }),
            StatementInner::If { inner, .. } => writer.commented(&inner.comments, |writer| {
                writer.token("if (")?;
                writer.expression(&inner.test)?;
                writer.token(")")?;
                if inner.alternate.is_some() && Self::unmatched_if(&inner.consequent) {
                    writer.token(" {")?;
                    writer.newline();
                    writer.indent += 1;
                    writer.statement(&inner.consequent)?;
                    if !writer.line_start {
                        writer.newline();
                    }
                    writer.indent -= 1;
                    writer.token("}")?;
                } else {
                    writer.body(&inner.consequent)?;
                }
                if let Some(alternate) = &inner.alternate {
                    writer.body_separator(&inner.consequent)?;
                    writer.at(&alternate.loc, |writer| {
                        writer.commented(&alternate.comments, |writer| {
                            writer.token("else")?;
                            writer.body(&alternate.body)
                        })
                    })?;
                }
                Ok(())
            }),
            StatementInner::While { inner, .. } => writer.commented(&inner.comments, |writer| {
                writer.token("while (")?;
                writer.expression(&inner.test)?;
                writer.token(")")?;
                writer.body(&inner.body)
            }),
            StatementInner::DoWhile { inner, .. } => writer.commented(&inner.comments, |writer| {
                writer.token("do")?;
                writer.body(&inner.body)?;
                writer.body_separator(&inner.body)?;
                writer.token("while (")?;
                writer.expression(&inner.test)?;
                writer.token(");")
            }),
            StatementInner::For { inner, .. } => writer.commented(&inner.comments, |writer| {
                writer.token("for (")?;
                if let Some(init) = &inner.init {
                    match init {
                        statement::for_::Init::InitDeclaration((loc, declaration)) => {
                            writer.at(loc, |writer| writer.variable(declaration))?
                        }
                        statement::for_::Init::InitExpression(expr) => writer.expression(expr)?,
                    }
                }
                writer.token("; ")?;
                if let Some(test) = &inner.test {
                    writer.expression(test)?;
                }
                writer.token("; ")?;
                if let Some(update) = &inner.update {
                    writer.expression(update)?;
                }
                writer.token(")")?;
                writer.body(&inner.body)
            }),
            StatementInner::ForIn { inner, .. } => writer.commented(&inner.comments, |writer| {
                writer.token("for (")?;
                match &inner.left {
                    statement::for_in::Left::LeftDeclaration((loc, declaration)) => {
                        writer.at(loc, |writer| writer.variable(declaration))?
                    }
                    statement::for_in::Left::LeftPattern(pattern) => writer.pattern(pattern)?,
                }
                writer.token(" in ")?;
                writer.expression(&inner.right)?;
                writer.token(")")?;
                writer.body(&inner.body)
            }),
            StatementInner::ForOf { inner, .. } => writer.commented(&inner.comments, |writer| {
                writer.token(if inner.await_ { "for await (" } else { "for (" })?;
                match &inner.left {
                    statement::for_of::Left::LeftDeclaration((loc, declaration)) => {
                        writer.at(loc, |writer| writer.variable(declaration))?
                    }
                    statement::for_of::Left::LeftPattern(pattern) => writer.pattern(pattern)?,
                }
                writer.token(" of ")?;
                writer.expression(&inner.right)?;
                writer.token(")")?;
                writer.body(&inner.body)
            }),
            StatementInner::Labeled { inner, .. } => writer.commented(&inner.comments, |writer| {
                writer.identifier(&inner.label)?;
                writer.token(":")?;
                writer.body(&inner.body)
            }),
            StatementInner::With { inner, .. } => writer.commented(&inner.comments, |writer| {
                writer.token("with (")?;
                writer.expression(&inner.object)?;
                writer.token(")")?;
                writer.body(&inner.body)
            }),
            StatementInner::Switch { inner, .. } => writer.commented(&inner.comments, |writer| {
                writer.token("switch (")?;
                writer.expression(&inner.discriminant)?;
                writer.token(") {")?;
                writer.newline();
                writer.indent += 1;
                for case in inner.cases.iter() {
                    writer.at(&case.loc, |writer| {
                        writer.commented(&case.comments, |writer| {
                            match &case.test {
                                Some(test) => {
                                    writer.token("case ")?;
                                    writer.expression(test)?;
                                }
                                None => writer.token("default")?,
                            }
                            writer.token(":")?;
                            writer.newline();
                            writer.indent += 1;
                            writer.statements(&case.consequent)?;
                            writer.indent -= 1;
                            Ok(())
                        })
                    })?;
                }
                writer.indent -= 1;
                writer.token("}")
            }),
            StatementInner::Try { inner, .. } => writer.commented(&inner.comments, |writer| {
                writer.token("try ")?;
                writer.at(&inner.block.0, |writer| writer.block(&inner.block.1))?;
                if let Some(handler) = &inner.handler {
                    writer.at(&handler.loc, |writer| {
                        writer.commented(&handler.comments, |writer| {
                            writer.token(" catch")?;
                            if let Some(param) = &handler.param {
                                writer.token(" (")?;
                                writer.pattern(param)?;
                                writer.token(")")?;
                            }
                            writer.token(" ")?;
                            writer.at(&handler.body.0, |writer| writer.block(&handler.body.1))
                        })
                    })?;
                }
                if let Some((loc, block)) = &inner.finalizer {
                    writer.token(" finally ")?;
                    writer.at(loc, |writer| writer.block(block))?;
                }
                Ok(())
            }),
            StatementInner::ImportDeclaration { inner, .. } => writer.import_declaration(inner),
            StatementInner::ExportNamedDeclaration { inner, .. } => writer.export_named(inner),
            StatementInner::ExportDefaultDeclaration { inner, .. } => {
                writer.commented(&inner.comments, |writer| {
                    writer.token("export default ")?;
                    match &inner.declaration {
                        statement::export_default_declaration::Declaration::Declaration(
                            declaration,
                        ) => writer.statement(declaration),
                        statement::export_default_declaration::Declaration::Expression(expr) => {
                            writer.fenced_expression(expr)?;
                            writer.token(";")
                        }
                    }
                })
            }
            StatementInner::ComponentDeclaration { .. }
            | StatementInner::DeclareClass { .. }
            | StatementInner::DeclareComponent { .. }
            | StatementInner::DeclareEnum { .. }
            | StatementInner::DeclareExportDeclaration { .. }
            | StatementInner::DeclareFunction { .. }
            | StatementInner::DeclareInterface { .. }
            | StatementInner::DeclareModule { .. }
            | StatementInner::DeclareModuleExports { .. }
            | StatementInner::DeclareNamespace { .. }
            | StatementInner::DeclareTypeAlias { .. }
            | StatementInner::DeclareOpaqueType { .. }
            | StatementInner::DeclareVariable { .. }
            | StatementInner::EnumDeclaration { .. }
            | StatementInner::ExportAssignment { .. }
            | StatementInner::NamespaceExportDeclaration { .. }
            | StatementInner::ImportEqualsDeclaration { .. }
            | StatementInner::InterfaceDeclaration { .. }
            | StatementInner::Match { .. }
            | StatementInner::RecordDeclaration { .. }
            | StatementInner::TypeAlias { .. }
            | StatementInner::OpaqueType { .. } => {
                Err(PrintError::UnloweredStatement(stmt.loc().dupe()))
            }
        })
    }

    fn jump(&mut self, keyword: &str, label: Option<&ast::Identifier<Loc, Loc>>) -> Printed {
        if let Some(label) = label {
            self.leading(&label.comments)?;
        }
        self.token(keyword)?;
        if let Some(label) = label {
            self.token(" ")?;
            self.at(&label.loc, |writer| {
                writer.write(&label.name, Some(&label.name))
            })?;
        }
        self.token(";")?;
        if let Some(label) = label {
            self.trailing(&label.comments)?;
        }
        Ok(())
    }

    fn body(&mut self, body: &Stmt) -> Printed {
        if matches!(&**body, StatementInner::Block { .. }) {
            self.token(" ")?;
            self.statement(body)
        } else {
            self.newline();
            self.indent += 1;
            self.statement(body)?;
            self.indent -= 1;
            Ok(())
        }
    }

    fn unmatched_if(statement: &Stmt) -> bool {
        match &**statement {
            StatementInner::If { inner, .. } => inner
                .alternate
                .as_ref()
                .is_none_or(|alternate| Self::unmatched_if(&alternate.body)),
            StatementInner::While { inner, .. } => Self::unmatched_if(&inner.body),
            StatementInner::For { inner, .. } => Self::unmatched_if(&inner.body),
            StatementInner::ForIn { inner, .. } => Self::unmatched_if(&inner.body),
            StatementInner::ForOf { inner, .. } => Self::unmatched_if(&inner.body),
            StatementInner::With { inner, .. } => Self::unmatched_if(&inner.body),
            StatementInner::Labeled { inner, .. } => Self::unmatched_if(&inner.body),
            _ => false,
        }
    }

    fn body_separator(&mut self, body: &Stmt) -> Printed {
        if matches!(&**body, StatementInner::Block { .. }) && !self.line_start {
            self.token(" ")
        } else {
            if !self.line_start {
                self.newline();
            }
            Ok(())
        }
    }

    pub(super) fn block(&mut self, block: &statement::Block<Loc, Loc>) -> Printed {
        self.leading(&block.comments)?;
        self.token("{")?;
        self.newline();
        self.indent += 1;
        self.internal(&block.comments)?;
        self.statements(&block.body)?;
        if !self.line_start {
            self.newline();
        }
        self.indent -= 1;
        self.token("}")?;
        self.trailing(&block.comments)
    }

    fn variable(&mut self, declaration: &statement::VariableDeclaration<Loc, Loc>) -> Printed {
        self.commented(&declaration.comments, |writer| {
            writer.token(match declaration.kind {
                ast::VariableKind::Var => "var ",
                ast::VariableKind::Let => "let ",
                ast::VariableKind::Const => "const ",
            })?;
            for (index, declarator) in declaration.declarations.iter().enumerate() {
                if index != 0 {
                    writer.token(", ")?;
                }
                writer.at(&declarator.loc, |writer| {
                    if declarator.definite {
                        return Err(PrintError::TypeAnnotation(declarator.loc.dupe()));
                    }
                    writer.pattern(&declarator.id)?;
                    if let Some(init) = &declarator.init {
                        writer.token(" = ")?;
                        writer.expression(init)?;
                    }
                    Ok(())
                })?;
            }
            Ok(())
        })
    }

    pub(super) fn function_annotations(&self, function: &function::Function<Loc, Loc>) -> Printed {
        if function.tparams.is_some()
            || function.predicate.is_some()
            || matches!(function.effect_, function::Effect::Hook)
        {
            return Err(PrintError::TypeAnnotation(function.sig_loc.dupe()));
        }
        match &function.return_ {
            function::ReturnAnnot::Missing(_) => Ok(()),
            function::ReturnAnnot::Available(annotation) => {
                Err(PrintError::TypeAnnotation(annotation.loc.dupe()))
            }
            function::ReturnAnnot::TypeGuard(annotation) => {
                Err(PrintError::TypeAnnotation(annotation.loc.dupe()))
            }
        }
    }

    pub(super) fn function(&mut self, function: &function::Function<Loc, Loc>) -> Printed {
        self.function_annotations(function)?;
        self.commented(&function.comments, |writer| {
            if function.async_ {
                writer.token("async ")?;
            }
            writer.token("function")?;
            if function.generator {
                writer.token("*")?;
            }
            if let Some(id) = &function.id {
                writer.token(" ")?;
                writer.identifier(id)?;
            }
            writer.params(&function.params)?;
            writer.token(" ")?;
            writer.function_body(function, false)
        })
    }

    pub(super) fn function_body(
        &mut self,
        function: &function::Function<Loc, Loc>,
        arrow: bool,
    ) -> Printed {
        match &function.body {
            function::Body::BodyBlock((loc, block)) => self.at(loc, |writer| writer.block(block)),
            function::Body::BodyExpression(expr) if arrow => self.fenced_expression(expr),
            function::Body::BodyExpression(expr) => {
                Err(PrintError::FunctionBody(expr.loc().dupe()))
            }
        }
    }

    pub(super) fn method(
        &mut self,
        key: &object::Key<Loc, Loc>,
        function: &function::Function<Loc, Loc>,
        accessor: Option<&str>,
    ) -> Printed {
        self.function_annotations(function)?;
        self.commented(&function.comments, |writer| {
            if let Some(accessor) = accessor {
                writer.token(accessor)?;
                writer.token(" ")?;
            }
            if function.async_ {
                writer.token("async ")?;
            }
            if function.generator {
                writer.token("*")?;
            }
            writer.key(key)?;
            writer.params(&function.params)?;
            writer.token(" ")?;
            writer.function_body(function, false)
        })
    }

    pub(super) fn class(&mut self, class: &class::Class<Loc, Loc>) -> Printed {
        if class.tparams.is_some()
            || class.implements.is_some()
            || class.abstract_
            || !class.class_decorators.is_empty()
        {
            return Err(PrintError::TypeAnnotation(self.location.dupe()));
        }
        self.commented(&class.comments, |writer| {
            writer.token("class")?;
            if let Some(id) = &class.id {
                writer.token(" ")?;
                writer.identifier(id)?;
            }
            if let Some(extends) = &class.extends {
                if extends.targs.is_some() {
                    return Err(PrintError::TypeAnnotation(extends.loc.dupe()));
                }
                writer.at(&extends.loc, |writer| {
                    writer.commented(&extends.comments, |writer| {
                        writer.token(" extends ")?;
                        writer.fenced_expression(&extends.expr)
                    })
                })?;
            }
            writer.token(" ")?;
            writer.at(&class.body.loc, |writer| {
                writer.commented(&class.body.comments, |writer| {
                    writer.token("{")?;
                    writer.newline();
                    writer.indent += 1;
                    for element in class.body.body.iter() {
                        writer.class_element(element)?;
                        if !writer.line_start {
                            writer.newline();
                        }
                    }
                    writer.indent -= 1;
                    writer.token("}")
                })
            })
        })
    }

    fn class_element(&mut self, element: &class::BodyElement<Loc, Loc>) -> Printed {
        match element {
            class::BodyElement::Method(method) => self.at(&method.loc, |writer| {
                if method.override_
                    || method.ts_accessibility.is_some()
                    || !method.decorators.is_empty()
                {
                    return Err(PrintError::UnloweredClassElement(method.loc.dupe()));
                }
                writer.commented(&method.comments, |writer| {
                    if method.static_ {
                        writer.token("static ")?;
                    }
                    let accessor = match method.kind {
                        class::MethodKind::Get => Some("get"),
                        class::MethodKind::Set => Some("set"),
                        class::MethodKind::Constructor | class::MethodKind::Method => None,
                    };
                    writer.method(&method.key, &method.value.1, accessor)
                })
            }),
            class::BodyElement::Property(property) => self.at(&property.loc, |writer| {
                writer.annotation(&property.annot)?;
                if property.override_
                    || property.optional
                    || property.definite
                    || property.variance.is_some()
                    || property.ts_accessibility.is_some()
                    || !property.decorators.is_empty()
                {
                    return Err(PrintError::UnloweredClassElement(property.loc.dupe()));
                }
                writer.commented(&property.comments, |writer| {
                    if property.static_ {
                        writer.token("static ")?;
                    }
                    writer.key(&property.key)?;
                    writer.field_value(&property.value)
                })
            }),
            class::BodyElement::PrivateField(property) => self.at(&property.loc, |writer| {
                writer.annotation(&property.annot)?;
                if property.override_
                    || property.optional
                    || property.definite
                    || property.variance.is_some()
                    || property.ts_accessibility.is_some()
                    || !property.decorators.is_empty()
                {
                    return Err(PrintError::UnloweredClassElement(property.loc.dupe()));
                }
                writer.commented(&property.comments, |writer| {
                    if property.static_ {
                        writer.token("static ")?;
                    }
                    writer.private_name(&property.key)?;
                    writer.field_value(&property.value)
                })
            }),
            class::BodyElement::StaticBlock(block) => self.at(&block.loc, |writer| {
                writer.leading(&block.comments)?;
                writer.token("static {")?;
                writer.newline();
                writer.indent += 1;
                writer.internal(&block.comments)?;
                writer.statements(&block.body)?;
                if !writer.line_start {
                    writer.newline();
                }
                writer.indent -= 1;
                writer.token("}")?;
                writer.trailing(&block.comments)
            }),
            class::BodyElement::DeclareMethod(method) => {
                Err(PrintError::UnloweredClassElement(method.loc.dupe()))
            }
            class::BodyElement::AbstractMethod(method) => {
                Err(PrintError::UnloweredClassElement(method.loc.dupe()))
            }
            class::BodyElement::AbstractProperty(property) => {
                Err(PrintError::UnloweredClassElement(property.loc.dupe()))
            }
            class::BodyElement::IndexSignature(indexer) => {
                Err(PrintError::UnloweredClassElement(indexer.loc.dupe()))
            }
        }
    }

    fn field_value(&mut self, value: &class::property::Value<Loc, Loc>) -> Printed {
        match value {
            class::property::Value::Declared => {
                return Err(PrintError::UnloweredClassElement(self.location.dupe()));
            }
            class::property::Value::Uninitialized => {}
            class::property::Value::Initialized(expr) => {
                self.token(" = ")?;
                self.expression(expr)?;
            }
        }
        self.token(";")
    }

    fn module_name(&mut self, id: &ast::Identifier<Loc, Loc>) -> Printed {
        let mut chars = id.name.chars();
        let identifier = chars
            .next()
            .is_some_and(|ch| ch.is_ascii_alphabetic() || ch == '_' || ch == '$')
            && chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '$');
        if identifier {
            self.identifier(id)
        } else {
            self.at(&id.loc, |writer| {
                writer.commented(&id.comments, |writer| writer.quoted(&id.name))
            })
        }
    }

    fn import_declaration(&mut self, import: &statement::ImportDeclaration<Loc, Loc>) -> Printed {
        use statement::import_declaration::Specifier;
        if !matches!(import.import_kind, statement::ImportKind::ImportValue) {
            return Err(PrintError::UnloweredStatement(self.location.dupe()));
        }
        self.commented(&import.comments, |writer| {
            writer.token("import ")?;
            if let Some(statement::ImportPhase::Defer) = import.phase {
                writer.token("defer ")?;
            }
            if let Some(default) = &import.default {
                writer.identifier(&default.identifier)?;
                if import.specifiers.is_some() {
                    writer.token(", ")?;
                }
            }
            if let Some(specifiers) = &import.specifiers {
                match specifiers {
                    Specifier::ImportNamespaceSpecifier((loc, id)) => writer.at(loc, |writer| {
                        writer.token("* as ")?;
                        writer.identifier(id)
                    })?,
                    Specifier::ImportNamedSpecifiers(specifiers) => {
                        writer.token("{")?;
                        for (index, specifier) in specifiers.iter().enumerate() {
                            if !matches!(
                                specifier.kind,
                                None | Some(statement::ImportKind::ImportValue)
                            ) {
                                return Err(PrintError::UnloweredStatement(
                                    specifier.remote.loc.dupe(),
                                ));
                            }
                            if index != 0 {
                                writer.token(", ")?;
                            }
                            writer.module_name(&specifier.remote)?;
                            if let Some(local) = &specifier.local {
                                writer.token(" as ")?;
                                writer.identifier(local)?;
                            }
                        }
                        writer.token("}")?;
                    }
                }
            }
            if import.default.is_some() || import.specifiers.is_some() {
                writer.token(" from ")?;
            }
            writer.at(&import.source.0, |writer| writer.string(&import.source.1))?;
            if let Some((loc, attributes)) = &import.attributes {
                writer.import_attributes(loc, attributes)?;
            }
            writer.token(";")
        })
    }

    fn import_attributes(
        &mut self,
        loc: &Loc,
        attributes: &[statement::import_declaration::ImportAttribute<Loc, Loc>],
    ) -> Printed {
        use statement::import_declaration::ImportAttributeKey;
        self.at(loc, |writer| {
            writer.token(" with {")?;
            for (index, attribute) in attributes.iter().enumerate() {
                if index != 0 {
                    writer.token(", ")?;
                }
                writer.at(&attribute.loc, |writer| {
                    match &attribute.key {
                        ImportAttributeKey::Identifier(id) => writer.identifier(id)?,
                        ImportAttributeKey::StringLiteral(loc, literal) => {
                            writer.at(loc, |writer| writer.string(literal))?
                        }
                    }
                    writer.token(": ")?;
                    writer.at(&attribute.value.0, |writer| {
                        writer.string(&attribute.value.1)
                    })
                })?;
            }
            writer.token("}")
        })
    }

    fn export_named(&mut self, export: &statement::ExportNamedDeclaration<Loc, Loc>) -> Printed {
        use statement::export_named_declaration::Specifier;
        if !matches!(export.export_kind, statement::ExportKind::ExportValue) {
            return Err(PrintError::UnloweredStatement(self.location.dupe()));
        }
        self.commented(&export.comments, |writer| {
            writer.token("export ")?;
            if let Some(declaration) = &export.declaration {
                return writer.statement(declaration);
            }
            if let Some(specifiers) = &export.specifiers {
                match specifiers {
                    Specifier::ExportBatchSpecifier(batch) => writer.at(&batch.loc, |writer| {
                        writer.token("*")?;
                        if let Some(id) = &batch.specifier {
                            writer.token(" as ")?;
                            writer.module_name(id)?;
                        }
                        Ok(())
                    })?,
                    Specifier::ExportSpecifiers(specifiers) => {
                        writer.token("{")?;
                        for (index, specifier) in specifiers.iter().enumerate() {
                            if !matches!(specifier.export_kind, statement::ExportKind::ExportValue)
                            {
                                return Err(PrintError::UnloweredStatement(specifier.loc.dupe()));
                            }
                            if index != 0 {
                                writer.token(", ")?;
                            }
                            writer.at(&specifier.loc, |writer| {
                                writer.module_name(&specifier.local)?;
                                if let Some(exported) = &specifier.exported {
                                    writer.token(" as ")?;
                                    writer.module_name(exported)?;
                                }
                                Ok(())
                            })?;
                        }
                        writer.token("}")?;
                    }
                }
            }
            if let Some((loc, source)) = &export.source {
                writer.token(" from ")?;
                writer.at(loc, |writer| writer.string(source))?;
            }
            if let Some((loc, attributes)) = &export.attributes {
                writer.import_attributes(loc, attributes)?;
            }
            writer.token(";")
        })
    }
}
