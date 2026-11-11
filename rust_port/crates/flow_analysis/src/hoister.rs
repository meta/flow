/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::ops::Deref;

use dupe::Dupe;
use flow_common::js_number::numeric_property_name;
use flow_data_structure_wrapper::smol_str::FlowSmolStr;
use flow_parser::ast;
use flow_parser::ast::statement::StatementInner;
use flow_parser::ast_visitor;
use flow_parser::ast_visitor::AstVisitor;

use crate::bindings::Bindings;
use crate::bindings::Entry;
use crate::bindings::Kind;
use crate::bindings::SyntheticVar;
use crate::bindings::VarKey;

// Hoister class. Does a shallow visit of statements, looking for binding
// declarations (currently, variable declarations, parameters, function
// declarations, and component declarations) and recording the corresponding
// bindings in a list. The list can have duplicates, which are handled
// elsewhere.
//
// TODO: Ideally implemented as a fold, not a map.
// TODO: It should be possible to vastly simplify hoisting by overriding the
// general method `pattern_identifier ?kind` for kind = Var | Let | Const that
// was recently introduced to distinguish bindings and assignments from other
// occurrences (`identifier`).
//
// Instead, it is implemented below by overriding several specific methods that
// are known to introduce bindings. The logic here is sufficiently tricky that
// we probably should not change it without extensive testing.

pub struct HoisterBase<Loc> {
    enable_enums: bool,
    with_types: bool,
    bindings: Bindings<Loc>,
    let_kind: Kind,
    lexical_only: bool,
    lexical: bool,
}

impl<Loc: Dupe> HoisterBase<Loc> {
    fn add_var_binding(&mut self, entry: &ast::Identifier<Loc, Loc>) {
        if !self.lexical_only {
            self.bindings.add(Entry {
                loc: entry.loc.dupe(),
                key: VarKey::Name(entry.name.dupe()),
                kind: Kind::Var,
            });
        }
    }

    fn add_let_binding(&mut self, kind: Option<Kind>, entry: &ast::Identifier<Loc, Loc>) {
        if self.lexical_only || self.lexical {
            let kind = kind.unwrap_or(self.let_kind);
            self.bindings.add(Entry {
                loc: entry.loc.dupe(),
                key: VarKey::Name(entry.name.dupe()),
                kind,
            });
        }
    }

    fn add_const_binding(&mut self, kind: Option<Kind>, entry: &ast::Identifier<Loc, Loc>) {
        if self.lexical_only || self.lexical {
            let kind = kind.unwrap_or(Kind::Const);
            self.bindings.add(Entry {
                loc: entry.loc.dupe(),
                key: VarKey::Name(entry.name.dupe()),
                kind,
            });
        }
    }

    fn add_type_binding(&mut self, imported: bool, entry: &ast::Identifier<Loc, Loc>) {
        if !self.lexical_only {
            self.bindings.add(Entry {
                loc: entry.loc.dupe(),
                key: VarKey::Name(entry.name.dupe()),
                kind: Kind::Type {
                    imported,
                    type_only_namespace: false,
                },
            });
        }
    }

    fn add_interface_binding(&mut self, imported: bool, entry: &ast::Identifier<Loc, Loc>) {
        if !self.lexical_only {
            self.bindings.add(Entry {
                loc: entry.loc.dupe(),
                key: VarKey::Name(entry.name.dupe()),
                kind: Kind::Interface {
                    imported,
                    type_only_namespace: false,
                },
            });
        }
    }

    fn add_function_binding(&mut self, entry: &ast::Identifier<Loc, Loc>) {
        if self.lexical_only || self.lexical {
            self.bindings.add(Entry {
                loc: entry.loc.dupe(),
                key: VarKey::Name(entry.name.dupe()),
                kind: Kind::Function,
            });
        }
    }

    fn add_component_binding(&mut self, entry: &ast::Identifier<Loc, Loc>) {
        if self.lexical_only || self.lexical {
            self.bindings.add(Entry {
                loc: entry.loc.dupe(),
                key: VarKey::Name(entry.name.dupe()),
                kind: Kind::Component,
            });
        }
    }

    fn add_declared_function_binding(&mut self, entry: &ast::Identifier<Loc, Loc>) {
        if self.lexical_only || self.lexical {
            self.bindings.add(Entry {
                loc: entry.loc.dupe(),
                key: VarKey::Name(entry.name.dupe()),
                kind: Kind::DeclaredFunction,
            });
        }
    }

    fn add_declared_class_binding(&mut self, entry: &ast::Identifier<Loc, Loc>) {
        if self.lexical_only || self.lexical {
            self.bindings.add(Entry {
                loc: entry.loc.dupe(),
                key: VarKey::Name(entry.name.dupe()),
                kind: Kind::DeclaredClass,
            });
        }
    }

    fn add_declared_var_binding(&mut self, entry: &ast::Identifier<Loc, Loc>) {
        if !self.lexical_only {
            self.bindings.add(Entry {
                loc: entry.loc.dupe(),
                key: VarKey::Name(entry.name.dupe()),
                kind: Kind::DeclaredVar,
            });
        }
    }

    fn add_declared_let_binding(&mut self, entry: &ast::Identifier<Loc, Loc>) {
        if self.lexical_only || self.lexical {
            self.bindings.add(Entry {
                loc: entry.loc.dupe(),
                key: VarKey::Name(entry.name.dupe()),
                kind: Kind::DeclaredLet,
            });
        }
    }

    fn add_declared_const_binding(&mut self, entry: &ast::Identifier<Loc, Loc>) {
        if self.lexical_only || self.lexical {
            self.bindings.add(Entry {
                loc: entry.loc.dupe(),
                key: VarKey::Name(entry.name.dupe()),
                kind: Kind::DeclaredConst,
            });
        }
    }

    fn non_lexical_statement(
        &mut self,
        stmt: &ast::statement::Statement<Loc, Loc>,
    ) -> Result<(), !> {
        if self.lexical_only {
            return Ok(());
        }
        let current_lex = self.lexical;
        self.lexical = false;
        let Ok(()) = ast_visitor::statement_default(self, stmt);
        self.lexical = current_lex;
        Ok(())
    }
}

impl<Loc: Dupe> AstVisitor<'_, Loc> for HoisterBase<Loc> {
    fn normalize_loc(loc: &Loc) -> &Loc {
        loc
    }

    fn normalize_type(type_: &Loc) -> &Loc {
        type_
    }

    // Ignore all statements except variable declarations, class declarations, and
    // import declarations. The ignored statements cannot contain lexical
    // bindings in the current scope.
    fn statement(&mut self, stmt: &ast::statement::Statement<Loc, Loc>) -> Result<(), !> {
        match stmt.deref() {
            StatementInner::DeclareModule { .. } => {
                // Hoister should never add inner module declarations to bindings.
                Ok(())
            }
            StatementInner::VariableDeclaration { .. }
            | StatementInner::ClassDeclaration { .. }
            | StatementInner::DeclareClass { .. }
            | StatementInner::RecordDeclaration { .. }
            | StatementInner::DeclareExportDeclaration { .. }
            | StatementInner::DeclareVariable { .. }
            | StatementInner::EnumDeclaration { .. }
            | StatementInner::ExportDefaultDeclaration { .. }
            | StatementInner::ExportNamedDeclaration { .. }
            | StatementInner::ImportDeclaration { .. }
            | StatementInner::ImportEqualsDeclaration { .. }
            | StatementInner::Labeled { .. }
            | StatementInner::ComponentDeclaration { .. }
            | StatementInner::FunctionDeclaration { .. }
            | StatementInner::DeclareComponent { .. }
            | StatementInner::DeclareFunction { .. } => ast_visitor::statement_default(self, stmt),
            StatementInner::Block { .. }
            | StatementInner::Break { .. }
            | StatementInner::Continue { .. }
            | StatementInner::Debugger { .. }
            | StatementInner::DeclareEnum { .. }
            | StatementInner::DeclareInterface { .. }
            | StatementInner::DeclareModuleExports { .. }
            | StatementInner::DeclareNamespace { .. }
            | StatementInner::DeclareTypeAlias { .. }
            | StatementInner::DeclareOpaqueType { .. }
            | StatementInner::DoWhile { .. }
            | StatementInner::Empty { .. }
            | StatementInner::ExportAssignment { .. }
            | StatementInner::NamespaceExportDeclaration { .. }
            | StatementInner::Expression { .. }
            | StatementInner::For { .. }
            | StatementInner::ForIn { .. }
            | StatementInner::ForOf { .. }
            | StatementInner::If { .. }
            | StatementInner::InterfaceDeclaration { .. }
            | StatementInner::Match { .. }
            | StatementInner::Return { .. }
            | StatementInner::Switch { .. }
            | StatementInner::Throw { .. }
            | StatementInner::Try { .. }
            | StatementInner::TypeAlias { .. }
            | StatementInner::OpaqueType { .. }
            | StatementInner::While { .. }
            | StatementInner::With { .. } => self.non_lexical_statement(stmt),
        }
    }

    // Ignore expressions. This includes, importantly, initializers of variable declarations.
    fn expression(&mut self, _expr: &ast::expression::Expression<Loc, Loc>) -> Result<(), !> {
        // Do nothing
        Ok(())
    }

    // This is visited by variable declarations, as well as other kinds of patterns that we ignore.
    fn pattern(
        &mut self,
        kind: Option<ast::VariableKind>,
        pattern: &ast::pattern::Pattern<Loc, Loc>,
    ) -> Result<(), !> {
        match kind {
            None => {
                // No kind specified, don't process
                Ok(())
            }
            Some(ast::VariableKind::Let) | Some(ast::VariableKind::Const) => {
                let is_let = matches!(kind, Some(ast::VariableKind::Let));
                match pattern {
                    ast::pattern::Pattern::Identifier { inner, .. } => {
                        if is_let {
                            self.add_let_binding(None, &inner.name)
                        } else {
                            self.add_const_binding(None, &inner.name)
                        }
                        Ok(())
                    }
                    ast::pattern::Pattern::Object { .. } | ast::pattern::Pattern::Array { .. } => {
                        ast_visitor::pattern_default(self, kind, pattern)
                    }
                    ast::pattern::Pattern::Expression { .. } => Ok(()),
                }
            }
            Some(ast::VariableKind::Var) => {
                if !self.lexical_only {
                    match pattern {
                        ast::pattern::Pattern::Identifier { inner, .. } => {
                            self.add_var_binding(&inner.name);
                            Ok(())
                        }
                        ast::pattern::Pattern::Object { .. }
                        | ast::pattern::Pattern::Array { .. } => {
                            ast_visitor::pattern_default(self, kind, pattern)
                        }
                        ast::pattern::Pattern::Expression { .. } => Ok(()),
                    }
                } else {
                    Ok(())
                }
            }
        }
    }

    fn match_binding_pattern(
        &mut self,
        _loc: &Loc,
        binding_pattern: &ast::match_pattern::BindingPattern<Loc, Loc>,
    ) -> Result<(), !> {
        match binding_pattern.kind {
            ast::VariableKind::Let => self.add_let_binding(None, &binding_pattern.id),
            ast::VariableKind::Const => self.add_const_binding(None, &binding_pattern.id),
            ast::VariableKind::Var => {
                // `var` is not allowed
            }
        }
        Ok(())
    }

    fn match_as_pattern(
        &mut self,
        as_pattern: &ast::match_pattern::AsPattern<Loc, Loc>,
    ) -> Result<(), !> {
        match &as_pattern.target {
            ast::match_pattern::as_pattern::Target::Binding { loc, pattern } => {
                let Ok(()) = self.match_binding_pattern(loc, pattern);
            }
            ast::match_pattern::as_pattern::Target::Identifier(id) => {
                let Ok(()) = ast_visitor::match_pattern_default(self, &as_pattern.pattern);
                self.add_const_binding(None, id);
            }
        }
        Ok(())
    }

    fn function_param_pattern(
        &mut self,
        pattern: &ast::pattern::Pattern<Loc, Loc>,
    ) -> Result<(), !> {
        let old_let_kind = self.let_kind;
        self.let_kind = Kind::Parameter;
        let Ok(()) = self.binding_pattern(ast::VariableKind::Let, pattern);
        self.let_kind = old_let_kind;
        Ok(())
    }

    fn component_param_pattern(
        &mut self,
        pattern: &ast::pattern::Pattern<Loc, Loc>,
    ) -> Result<(), !> {
        let old_let_kind = self.let_kind;
        self.let_kind = Kind::ComponentParameter;
        let Ok(()) = self.binding_pattern(ast::VariableKind::Let, pattern);
        self.let_kind = old_let_kind;
        Ok(())
    }

    fn catch_clause_pattern(&mut self, pattern: &ast::pattern::Pattern<Loc, Loc>) -> Result<(), !> {
        let old_let_kind = self.let_kind;
        self.let_kind = Kind::CatchParameter;
        let Ok(()) = self.binding_pattern(ast::VariableKind::Let, pattern);
        self.let_kind = old_let_kind;
        Ok(())
    }

    fn class_(&mut self, _loc: &Loc, cls: &ast::class::Class<Loc, Loc>) -> Result<(), !> {
        if let Some(id) = &cls.id {
            self.add_let_binding(Some(Kind::Class), id);
        }
        Ok(())
    }

    fn function_declaration(
        &mut self,
        _loc: &Loc,
        func: &ast::function::Function<Loc, Loc>,
    ) -> Result<(), !> {
        if let Some(id) = &func.id {
            self.add_function_binding(id);
        }
        Ok(())
    }

    fn declare_component(
        &mut self,
        _loc: &Loc,
        decl: &ast::statement::DeclareComponent<Loc, Loc>,
    ) -> Result<(), !> {
        self.add_component_binding(&decl.id);
        Ok(())
    }

    fn component_declaration(
        &mut self,
        _loc: &Loc,
        decl: &ast::statement::ComponentDeclaration<Loc, Loc>,
    ) -> Result<(), !> {
        self.add_component_binding(&decl.id);
        Ok(())
    }

    fn declare_class(
        &mut self,
        _loc: &Loc,
        decl: &ast::statement::DeclareClass<Loc, Loc>,
    ) -> Result<(), !> {
        self.add_declared_class_binding(&decl.id);
        Ok(())
    }

    fn declare_function(
        &mut self,
        _loc: &Loc,
        decl: &ast::statement::DeclareFunction<Loc, Loc>,
    ) -> Result<(), !> {
        // The first binding found wins, so we make sure add a declared function binding when
        // we come across it before attempting to transform it into a regular function
        if let Some(id) = &decl.id {
            self.add_declared_function_binding(id);
        }
        Ok(())
    }

    fn declare_variable(
        &mut self,
        _loc: &Loc,
        decl: &ast::statement::DeclareVariable<Loc, Loc>,
    ) -> Result<(), !> {
        let kind = decl.kind;
        for declarator in decl.declarations.iter() {
            if let ast::pattern::Pattern::Identifier { inner, .. } = &declarator.id {
                match kind {
                    ast::VariableKind::Var => self.add_declared_var_binding(&inner.name),
                    ast::VariableKind::Let => self.add_declared_let_binding(&inner.name),
                    ast::VariableKind::Const => self.add_declared_const_binding(&inner.name),
                }
            }
        }
        if !self.lexical_only {
            for declarator in decl.declarations.iter() {
                if let ast::pattern::Pattern::Identifier { inner, .. } = &declarator.id {
                    match kind {
                        ast::VariableKind::Var => {}
                        ast::VariableKind::Let => self.add_declared_let_binding(&inner.name),
                        ast::VariableKind::Const => self.add_declared_const_binding(&inner.name),
                    }
                }
            }
        }
        Ok(())
    }

    fn enum_declaration(
        &mut self,
        _loc: &Loc,
        enum_decl: &ast::statement::EnumDeclaration<Loc, Loc>,
    ) -> Result<(), !> {
        if self.enable_enums {
            self.add_const_binding(Some(Kind::Enum), &enum_decl.id);
        }
        Ok(())
    }

    fn declare_namespace(
        &mut self,
        _loc: &Loc,
        namespace: &ast::statement::DeclareNamespace<Loc, Loc>,
    ) -> Result<(), !> {
        match &namespace.id {
            ast::statement::declare_namespace::Id::Global { .. } => Ok(()),
            ast::statement::declare_namespace::Id::Local(id) => {
                let kind = if namespace
                    .body
                    .1
                    .body
                    .iter()
                    .all(flow_parser::ast_utils::is_type_only_declaration_statement)
                {
                    Kind::Type {
                        imported: false,
                        type_only_namespace: true,
                    }
                } else {
                    Kind::DeclaredConst
                };
                self.bindings.add(Entry {
                    loc: id.loc.dupe(),
                    key: VarKey::Name(id.name.dupe()),
                    kind,
                });
                Ok(())
            }
        }
    }

    fn record_declaration(
        &mut self,
        _loc: &Loc,
        record: &ast::statement::RecordDeclaration<Loc, Loc>,
    ) -> Result<(), !> {
        self.add_const_binding(Some(Kind::Record), &record.id);
        Ok(())
    }

    fn infer_type(&mut self, _infer: &ast::types::Infer<Loc, Loc>) -> Result<(), !> {
        Ok(())
    }

    fn type_alias(
        &mut self,
        _loc: &Loc,
        alias: &ast::statement::TypeAlias<Loc, Loc>,
    ) -> Result<(), !> {
        if !self.lexical_only && self.with_types {
            self.add_type_binding(false, &alias.id);
        }
        Ok(())
    }

    fn opaque_type(
        &mut self,
        _loc: &Loc,
        alias: &ast::statement::OpaqueType<Loc, Loc>,
    ) -> Result<(), !> {
        if !self.lexical_only && self.with_types {
            self.add_type_binding(false, &alias.id);
        }
        Ok(())
    }

    fn interface(
        &mut self,
        _loc: &Loc,
        interface: &ast::statement::Interface<Loc, Loc>,
    ) -> Result<(), !> {
        if !self.lexical_only && self.with_types {
            self.add_interface_binding(false, &interface.id);
        }
        Ok(())
    }

    fn import_declaration(
        &mut self,
        loc: &Loc,
        decl: &ast::statement::ImportDeclaration<Loc, Loc>,
    ) -> Result<(), !> {
        if !self.lexical_only {
            match (self.with_types, decl.import_kind) {
                (
                    false,
                    ast::statement::ImportKind::ImportType
                    | ast::statement::ImportKind::ImportTypeof,
                ) => Ok(()),
                _ => ast_visitor::import_declaration_default(self, loc, decl),
            }
        } else {
            Ok(())
        }
    }

    fn import_equals_declaration(
        &mut self,
        _loc: &Loc,
        decl: &ast::statement::ImportEqualsDeclaration<Loc, Loc>,
    ) -> Result<(), !> {
        if !self.lexical_only {
            // Only create bindings for ExternalModuleReference (require) form.
            // The Identifier form (import X = A.B.C) is not supported and has no
            // corresponding name_def entry, so creating a binding for it would
            // cause a NameDefOrderingFailure.
            match &decl.module_reference {
                ast::statement::import_equals_declaration::ModuleReference::ExternalModuleReference(
                    ..,
                ) => match decl.import_kind {
                    ast::statement::ImportKind::ImportValue => {
                        self.add_const_binding(Some(Kind::Import { namespace: false }), &decl.id);
                    }
                    ast::statement::ImportKind::ImportType
                    | ast::statement::ImportKind::ImportTypeof
                        if self.with_types =>
                    {
                        self.add_type_binding(true, &decl.id);
                    }
                    _ => {}
                },
                ast::statement::import_equals_declaration::ModuleReference::Identifier(..) => {}
            }
        }
        Ok(())
    }

    fn import_named_specifier(
        &mut self,
        import_kind: ast::statement::ImportKind,
        spec: &ast::statement::import_declaration::NamedSpecifier<Loc, Loc>,
    ) -> Result<(), !> {
        if self.lexical_only {
            return Ok(());
        }
        match spec {
            ast::statement::import_declaration::NamedSpecifier {
                kind,
                kind_loc: _,
                local: Some(binding),
                remote: _,
                remote_name_def_loc: _,
            }
            | ast::statement::import_declaration::NamedSpecifier {
                kind,
                kind_loc: _,
                local: None,
                remote: binding,
                remote_name_def_loc: _,
            } => match (kind, import_kind) {
                (None, ast::statement::ImportKind::ImportValue)
                | (Some(ast::statement::ImportKind::ImportValue), _) => {
                    self.add_const_binding(Some(Kind::Import { namespace: false }), binding);
                }
                (
                    _,
                    ast::statement::ImportKind::ImportType
                    | ast::statement::ImportKind::ImportTypeof,
                )
                | (
                    Some(
                        ast::statement::ImportKind::ImportType
                        | ast::statement::ImportKind::ImportTypeof,
                    ),
                    _,
                ) => {
                    if self.with_types {
                        self.add_type_binding(true, binding);
                    }
                }
            },
        }
        Ok(())
    }

    fn import_default_specifier(
        &mut self,
        import_kind: &ast::statement::ImportKind,
        id: &ast::Identifier<Loc, Loc>,
    ) -> Result<(), !> {
        if !self.lexical_only {
            match import_kind {
                ast::statement::ImportKind::ImportValue => {
                    self.add_const_binding(Some(Kind::Import { namespace: false }), id);
                }
                ast::statement::ImportKind::ImportType
                | ast::statement::ImportKind::ImportTypeof => {
                    if self.with_types {
                        self.add_type_binding(true, id);
                    }
                }
            }
        }
        Ok(())
    }

    fn import_namespace_specifier(
        &mut self,
        import_kind: ast::statement::ImportKind,
        _loc: &Loc,
        id: &ast::Identifier<Loc, Loc>,
    ) -> Result<(), !> {
        if !self.lexical_only {
            match import_kind {
                ast::statement::ImportKind::ImportValue => {
                    self.add_const_binding(Some(Kind::Import { namespace: true }), id);
                }
                ast::statement::ImportKind::ImportType => {
                    if self.with_types {
                        self.bindings.add(Entry {
                            loc: id.loc.dupe(),
                            key: VarKey::Name(id.name.dupe()),
                            kind: Kind::Type {
                                imported: true,
                                type_only_namespace: true,
                            },
                        });
                    }
                }
                ast::statement::ImportKind::ImportTypeof => {
                    if self.with_types {
                        self.add_type_binding(true, id);
                    }
                }
            }
        }
        Ok(())
    }
}

pub struct LexicalHoister<Loc>(HoisterBase<Loc>);

impl<Loc: Dupe> LexicalHoister<Loc> {
    pub fn new(enable_enums: bool) -> Self {
        Self(HoisterBase {
            enable_enums,
            with_types: false,
            bindings: Bindings::empty(),
            let_kind: Kind::Let,
            lexical_only: true,
            lexical: true,
        })
    }

    pub fn into_bindings(self) -> Bindings<Loc> {
        self.0.bindings
    }

    pub fn acc(&self) -> &Bindings<Loc> {
        &self.0.bindings
    }
}

impl<Loc> std::ops::Deref for LexicalHoister<Loc> {
    type Target = HoisterBase<Loc>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<Loc> std::ops::DerefMut for LexicalHoister<Loc> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

pub struct Hoister<Loc>(HoisterBase<Loc>);

impl<Loc: Dupe> Hoister<Loc> {
    pub fn new(enable_enums: bool, with_types: bool) -> Self {
        Self(HoisterBase {
            enable_enums,
            with_types,
            bindings: Bindings::empty(),
            let_kind: Kind::Let,
            lexical_only: false,
            lexical: true,
        })
    }

    pub fn into_bindings(self) -> Bindings<Loc> {
        self.0.bindings
    }

    pub fn bindings_so_far(&self) -> Bindings<Loc> {
        self.0.bindings.clone()
    }
}

impl<Loc> std::ops::Deref for Hoister<Loc> {
    type Target = HoisterBase<Loc>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<Loc> std::ops::DerefMut for Hoister<Loc> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

/// The variables that record whether a constructor has assigned each of a class's own instance
/// fields that have no initializer, at the fields' declarations. A `declare` field holds no value
/// of its own, so it has none, and neither does a field with a definite assignment assertion
/// (`value!: T`), which asserts that the field is assigned before it is read.
pub fn field_init_bindings<Loc: Dupe>(
    instance_body: &[&ast::class::BodyElement<Loc, Loc>],
) -> Bindings<Loc> {
    use ast::class::BodyElement;
    use ast::class::property::Value;

    instance_body
        .iter()
        .filter_map(|element| match element {
            BodyElement::Property(prop)
                if matches!(prop.value, Value::Uninitialized) && !prop.definite =>
            {
                property_key_name(&prop.key).map(|(name, loc)| (name, false, loc))
            }
            BodyElement::PrivateField(field)
                if matches!(field.value, Value::Uninitialized) && !field.definite =>
            {
                Some((field.key.name.dupe(), true, &field.key.loc))
            }
            _ => None,
        })
        .map(|(name, private_, loc)| Entry {
            loc: loc.dupe(),
            key: VarKey::Synthetic(SyntheticVar::FieldInit { name, private_ }),
            kind: Kind::Internal,
        })
        .collect()
}

/// The name of the property that a class property key declares, when the key states it: an
/// identifier, or a string or number literal in brackets (`["x"]`, `[0]`, `[1.5]`). Returns the
/// name as property reads spell it (`this.x`, `this[0]`), and the key's location.
pub fn property_key_name<Loc: Dupe>(
    key: &ast::expression::object::Key<Loc, Loc>,
) -> Option<(FlowSmolStr, &Loc)> {
    use ast::expression::ExpressionInner;
    use ast::expression::object::Key;

    match key {
        Key::Identifier(id) => Some((id.name.dupe(), &id.loc)),
        Key::Computed(computed) => {
            let name = match computed.expression.deref() {
                ExpressionInner::StringLiteral { inner, .. } => inner.value.dupe(),
                ExpressionInner::NumberLiteral { inner, .. } => numeric_property_name(inner.value)?,
                _ => return None,
            };
            Some((name, &computed.loc))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::parse_with_alocs;

    #[test]
    fn top_level_hoister_does_not_leak_nested_declared_class_bindings() {
        let ast = parse_with_alocs(
            r#"
{
  const C = class {};
}
{
  declare class C extends C {}
}
"#,
        );
        let mut hoister = Hoister::new(true, true);
        let Ok(()) = hoister.program(&ast);
        let bindings = hoister.into_bindings();
        assert!(
            !bindings.exists(|entry| entry.key == VarKey::Name("C".into())),
            "nested block bindings should not be hoisted into the enclosing scope"
        );
    }
}
