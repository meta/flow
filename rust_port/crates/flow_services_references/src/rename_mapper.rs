/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::collections::BTreeMap;

use dupe::Dupe;
use flow_data_structure_wrapper::smol_str::FlowSmolStr;
use flow_parser::ast;
use flow_parser::ast::class;
use flow_parser::ast::expression::member;
use flow_parser::ast::expression::object;
use flow_parser::ast::jsx;
use flow_parser::ast::pattern;
use flow_parser::ast::statement;
use flow_parser::ast::statement::export_named_declaration;
use flow_parser::ast::statement::import_declaration;
use flow_parser::ast_visitor;
use flow_parser::ast_visitor::AstVisitor;
use flow_parser::loc::Loc;
use flow_parser::loc_sig::LocSig;
use flow_parser_utils::ast_builder;

use crate::find_refs_types::RefKind;

fn get_rename_order<'a>(name: &'a str, new_name: &'a str, ref_kind: RefKind) -> (&'a str, &'a str) {
    match ref_kind {
        RefKind::PropertyDefinition | RefKind::PropertyAccess => (new_name, name),
        RefKind::Local => (name, new_name),
    }
}
struct RenameMapper {
    global: bool,
    targets: BTreeMap<Loc, RefKind>,
    new_name: String,
}

impl<'ast> AstVisitor<'ast, Loc, Loc, &'ast Loc, !> for RenameMapper {
    fn normalize_loc(loc: &'ast Loc) -> &'ast Loc {
        loc
    }

    fn normalize_type(type_: &'ast Loc) -> &'ast Loc {
        type_
    }

    fn map_identifier(&mut self, id: &'ast ast::Identifier<Loc, Loc>) -> ast::Identifier<Loc, Loc> {
        let loc = &id.loc;
        if self.targets.contains_key(loc) {
            ast::Identifier::new(ast::IdentifierInner {
                loc: loc.dupe(),
                name: FlowSmolStr::from(self.new_name.as_str()),
                comments: id.comments.dupe(),
            })
        } else {
            id.clone()
        }
    }

    fn map_import_named_specifier(
        &mut self,
        import_kind: statement::ImportKind,
        spec: &'ast import_declaration::NamedSpecifier<Loc, Loc>,
    ) -> import_declaration::NamedSpecifier<Loc, Loc> {
        let import_declaration::NamedSpecifier {
            local,
            remote,
            remote_name_def_loc,
            kind,
            kind_loc,
        } = spec;
        let loc = &remote.loc;
        if self.global {
            match local {
                Some(_) => ast_visitor::map_import_named_specifier_default(self, import_kind, spec),
                None => {
                    let new_remote = ast_builder::identifiers::identifier(None, &self.new_name);
                    if self.targets.contains_key(loc) {
                        import_declaration::NamedSpecifier {
                            local: None,
                            remote: new_remote,
                            remote_name_def_loc: remote_name_def_loc.clone(),
                            kind: *kind,
                            kind_loc: kind_loc.clone(),
                        }
                    } else {
                        spec.clone()
                    }
                }
            }
        } else {
            match local {
                Some(_) => ast_visitor::map_import_named_specifier_default(self, import_kind, spec),
                None => {
                    if self.targets.contains_key(loc) {
                        let local_name = ast_builder::identifiers::identifier(None, &self.new_name);
                        import_declaration::NamedSpecifier {
                            local: Some(local_name),
                            remote: remote.clone(),
                            remote_name_def_loc: remote_name_def_loc.clone(),
                            kind: *kind,
                            kind_loc: kind_loc.clone(),
                        }
                    } else {
                        spec.clone()
                    }
                }
            }
        }
    }

    fn map_export_named_declaration_specifier(
        &mut self,
        spec: &'ast export_named_declaration::ExportSpecifier<Loc, Loc>,
    ) -> export_named_declaration::ExportSpecifier<Loc, Loc> {
        let export_named_declaration::ExportSpecifier {
            loc: specifier_loc,
            local,
            exported,
            export_kind,
            from_remote,
            imported_name_def_loc,
        } = spec;
        let loc = &local.loc;
        match exported {
            Some(_) => ast_visitor::map_export_named_declaration_specifier_default(self, spec),
            None => {
                if self.targets.contains_key(loc) {
                    export_named_declaration::ExportSpecifier {
                        loc: specifier_loc.dupe(),
                        local: ast_builder::identifiers::identifier(None, &self.new_name),
                        exported: Some(ast::Identifier::new(ast::IdentifierInner {
                            loc: Loc::none(),
                            name: local.name.dupe(),
                            comments: local.comments.clone(),
                        })),
                        export_kind: *export_kind,
                        from_remote: *from_remote,
                        imported_name_def_loc: imported_name_def_loc.clone(),
                    }
                } else {
                    spec.clone()
                }
            }
        }
    }

    fn map_pattern_object_property(
        &mut self,
        kind: Option<ast::VariableKind>,
        prop: &'ast pattern::object::Property<Loc, Loc>,
    ) -> pattern::object::Property<Loc, Loc> {
        match prop {
            pattern::object::Property::NormalProperty(normal_prop) => {
                if normal_prop.shorthand {
                    match &normal_prop.key {
                        pattern::object::Key::Identifier(id)
                            if self.targets.contains_key(&id.loc) =>
                        {
                            let ref_kind = self.targets[&id.loc];
                            let (from_name, to_name) =
                                get_rename_order(&id.name, &self.new_name, ref_kind);
                            let new_prop = pattern::object::NormalProperty {
                                loc: id.loc.dupe(),
                                key: pattern::object::Key::Identifier(ast::Identifier::new(
                                    ast::IdentifierInner {
                                        loc: Loc::none(),
                                        name: FlowSmolStr::from(from_name),
                                        comments: id.comments.clone(),
                                    },
                                )),
                                pattern: ast_builder::patterns::identifier(None, None, to_name),
                                shorthand: false,
                                default: None,
                            };
                            pattern::object::Property::NormalProperty(new_prop)
                        }
                        pattern::object::Key::Computed(_)
                        | pattern::object::Key::StringLiteral(_)
                        | pattern::object::Key::NumberLiteral(_)
                        | pattern::object::Key::BigIntLiteral(_)
                        | pattern::object::Key::Identifier(_) => {
                            ast_visitor::map_pattern_object_property_default(self, kind, prop)
                        }
                    }
                } else {
                    ast_visitor::map_pattern_object_property_default(self, kind, prop)
                }
            }
            pattern::object::Property::RestElement(_) => {
                ast_visitor::map_pattern_object_property_default(self, kind, prop)
            }
        }
    }

    fn map_object_property(
        &mut self,
        prop: &'ast object::Property<Loc, Loc>,
    ) -> object::Property<Loc, Loc> {
        match prop {
            object::Property::NormalProperty(object::NormalProperty::Init {
                loc: obj_loc,
                key: object::Key::Identifier(id),
                shorthand: true,
                value: _,
            }) if self.targets.contains_key(&id.loc) => {
                let ref_kind = self.targets[&id.loc];
                let (from_name, to_name) = get_rename_order(&id.name, &self.new_name, ref_kind);
                let new_prop = object::NormalProperty::Init {
                    loc: obj_loc.dupe(),
                    key: object::Key::Identifier(ast::Identifier::new(ast::IdentifierInner {
                        loc: Loc::none(),
                        name: FlowSmolStr::from(from_name),
                        comments: id.comments.clone(),
                    })),
                    value: ast_builder::expressions::identifier(None, None, to_name),
                    shorthand: false,
                };
                object::Property::NormalProperty(new_prop)
            }
            object::Property::NormalProperty(object::NormalProperty::Init {
                shorthand: true,
                ..
            })
            | object::Property::NormalProperty(object::NormalProperty::Init { .. })
            | object::Property::NormalProperty(object::NormalProperty::Method { .. })
            | object::Property::NormalProperty(object::NormalProperty::Get { .. })
            | object::Property::NormalProperty(object::NormalProperty::Set { .. }) => {
                ast_visitor::map_object_property_default(self, prop)
            }
            object::Property::SpreadProperty(_) => {
                ast_visitor::map_object_property_default(self, prop)
            }
        }
    }

    fn map_member_property(
        &mut self,
        prop: &'ast member::Property<Loc, Loc>,
    ) -> member::Property<Loc, Loc> {
        match prop {
            member::Property::PropertyPrivateName(pn) if self.targets.contains_key(&pn.loc) => {
                if self.new_name.starts_with('#') {
                    member::Property::PropertyPrivateName(ast::PrivateName {
                        loc: pn.loc.dupe(),
                        name: FlowSmolStr::from(self.new_name.strip_prefix('#').unwrap()),
                        comments: pn.comments.clone(),
                    })
                } else {
                    member::Property::PropertyIdentifier(ast::Identifier::new(
                        ast::IdentifierInner {
                            loc: pn.loc.dupe(),
                            name: FlowSmolStr::from(self.new_name.as_str()),
                            comments: pn.comments.clone(),
                        },
                    ))
                }
            }
            _ => ast_visitor::map_member_property_default(self, prop),
        }
    }

    fn map_object_key(&mut self, key: &'ast object::Key<Loc, Loc>) -> object::Key<Loc, Loc> {
        match key {
            object::Key::PrivateName(pn) if self.targets.contains_key(&pn.loc) => {
                if self.new_name.starts_with('#') {
                    object::Key::PrivateName(ast::PrivateName {
                        loc: pn.loc.dupe(),
                        name: FlowSmolStr::from(self.new_name.strip_prefix('#').unwrap()),
                        comments: pn.comments.clone(),
                    })
                } else {
                    object::Key::Identifier(ast::Identifier::new(ast::IdentifierInner {
                        loc: pn.loc.dupe(),
                        name: FlowSmolStr::from(self.new_name.as_str()),
                        comments: pn.comments.clone(),
                    }))
                }
            }
            _ => ast_visitor::map_object_key_default(self, key),
        }
    }

    fn map_class_element(
        &mut self,
        elem: &'ast class::BodyElement<Loc, Loc>,
    ) -> class::BodyElement<Loc, Loc> {
        match elem {
            class::BodyElement::PrivateField(pf) if self.targets.contains_key(&pf.key.loc) => {
                let key_loc = &pf.key.loc;
                let name_comments = &pf.key.comments;
                if self.new_name.starts_with('#') {
                    let new_pf = class::PrivateField {
                        loc: pf.loc.dupe(),
                        key: ast::PrivateName {
                            loc: key_loc.dupe(),
                            name: FlowSmolStr::from(self.new_name.strip_prefix('#').unwrap()),
                            comments: name_comments.clone(),
                        },
                        value: pf.value.clone(),
                        annot: pf.annot.clone(),
                        static_: pf.static_,
                        override_: pf.override_,
                        optional: pf.optional,
                        definite: pf.definite,
                        variance: pf.variance.clone(),
                        ts_accessibility: pf.ts_accessibility.clone(),
                        decorators: pf.decorators.clone(),
                        comments: pf.comments.clone(),
                    };
                    let mapped = ast_visitor::map_class_private_field_default(self, &new_pf);
                    class::BodyElement::PrivateField(mapped)
                } else {
                    let new_prop = class::Property {
                        loc: pf.loc.dupe(),
                        key: object::Key::Identifier(ast::Identifier::new(ast::IdentifierInner {
                            loc: key_loc.dupe(),
                            name: FlowSmolStr::from(self.new_name.as_str()),
                            comments: name_comments.clone(),
                        })),
                        value: pf.value.clone(),
                        annot: pf.annot.clone(),
                        static_: pf.static_,
                        override_: pf.override_,
                        optional: pf.optional,
                        definite: pf.definite,
                        variance: pf.variance.clone(),
                        ts_accessibility: pf.ts_accessibility.clone(),
                        decorators: pf.decorators.clone(),
                        comments: pf.comments.clone(),
                    };
                    let mapped = ast_visitor::map_class_property_default(self, &new_prop);
                    class::BodyElement::Property(mapped)
                }
            }
            _ => ast_visitor::map_class_element_default(self, elem),
        }
    }

    fn map_jsx_element_name_identifier(
        &mut self,
        ident: &'ast jsx::Identifier<Loc, Loc>,
    ) -> jsx::Identifier<Loc, Loc> {
        let loc = &ident.loc;
        if self.targets.contains_key(loc) {
            jsx::Identifier {
                loc: loc.dupe(),
                name: FlowSmolStr::from(self.new_name.as_str()),
                comments: ident.comments.clone(),
            }
        } else {
            ident.clone()
        }
    }

    fn map_jsx_attribute_name(
        &mut self,
        name: &'ast jsx::attribute::Name<Loc, Loc>,
    ) -> jsx::attribute::Name<Loc, Loc> {
        match name {
            jsx::attribute::Name::Identifier(ident) if self.targets.contains_key(&ident.loc) => {
                jsx::attribute::Name::Identifier(jsx::Identifier {
                    loc: ident.loc.dupe(),
                    name: FlowSmolStr::from(self.new_name.as_str()),
                    comments: ident.comments.clone(),
                })
            }
            _ => ast_visitor::map_jsx_attribute_name_default(self, name),
        }
    }
}

pub fn rename(
    global: bool,
    targets: &BTreeMap<Loc, RefKind>,
    new_name: &str,
    ast: &ast::Program<Loc, Loc>,
) -> ast::Program<Loc, Loc> {
    let mut s = RenameMapper {
        global,
        targets: targets.clone(),
        new_name: new_name.to_string(),
    };
    s.map_program(ast)
}
