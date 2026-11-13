/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

#[cfg(test)]
mod parenthesized_expression_locations_tests {
    use crate::do_parse;
    use crate::init_env;
    use crate::parse_program_in_context;
    use crate::parser_env::PERMISSIVE_PARSE_OPTIONS;

    #[test]
    fn keeps_deferred_parenthesized_expression_locations() {
        let cases = [
            ("x = a ? (b) : c;", vec![((8, 11), (9, 10))], 0),
            (
                "a ? ((b)) : c;",
                vec![((5, 8), (6, 7)), ((4, 9), (6, 7))],
                0,
            ),
            (
                "a ? (b ? (c) : d) : e;",
                vec![((9, 12), (10, 11)), ((4, 17), (5, 16))],
                0,
            ),
            (
                "(p) + (a ? (b) : c) + (q);",
                vec![
                    ((0, 3), (1, 2)),
                    ((11, 14), (12, 13)),
                    ((6, 19), (7, 18)),
                    ((22, 25), (23, 24)),
                ],
                0,
            ),
            ("a ? (b): T => d : e;", vec![], 0),
            (
                "\"use strict\"; f<T>(0 ? (public) : c);",
                vec![((23, 31), (24, 30)), ((18, 36), (19, 35))],
                1,
            ),
            ("(p = a ? (b) : c) => 0;", vec![((9, 12), (10, 11))], 0),
        ];
        for (source, expected, expected_errors) in cases {
            let mut env = init_env::<()>(None, Some(PERMISSIVE_PARSE_OPTIONS), None, Ok(source));
            env.enable_parenthesized_expression_locations();
            let ((program, locations), errors) = do_parse(env, false, |env| {
                let program = parse_program_in_context(env, false);
                (program, env.take_parenthesized_expression_locations())
            })
            .expect("valid source should parse");
            assert!(program.is_ok(), "{source}: program parse rolled back");
            assert_eq!(errors.len(), expected_errors, "{source}: {errors:?}");
            let actual: Vec<_> = locations
                .iter()
                .map(|location| {
                    (
                        (location.loc.start.column, location.loc.end.column),
                        (
                            location.expression_loc.start.column,
                            location.expression_loc.end.column,
                        ),
                    )
                })
                .collect();
            assert_eq!(actual, expected, "{source}");
        }
    }
}

#[cfg(test)]
mod file_utils {
    use std::collections::BTreeSet;
    use std::fs;

    #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
    pub(super) enum FileKind {
        Dir(String),
        File(String),
    }

    pub(super) fn iter_files<T>(
        max_depth: Option<usize>,
        file_only: bool,
        paths: Vec<String>,
        acc: &mut T,
        filter: impl Fn(&str) -> bool,
        mut action: impl FnMut(FileKind, &mut T),
    ) {
        fn iter_dir<T, F>(
            depth: usize,
            max_depth: Option<usize>,
            filter: &F,
            file_only: bool,
            action: &mut impl FnMut(FileKind, &mut T),
            acc: &mut T,
            dir: String,
        ) where
            F: Fn(&str) -> bool,
        {
            if !file_only && filter(&dir) {
                action(FileKind::Dir(dir.to_owned()), acc);
            }
            if max_depth == Some(depth) {
                return;
            }
            let entries = match fs::read_dir(dir) {
                Ok(e) => e,
                Err(e) => panic!("{:?}", e),
            };
            let mut file_kinds = BTreeSet::new();
            for entry in entries.flatten() {
                let path = entry.path();
                let path_str = path.to_string_lossy().to_string();
                let Ok(metadata) = fs::symlink_metadata(&path) else {
                    continue;
                };
                let file_type = metadata.file_type();
                if file_type.is_file() {
                    file_kinds.insert(FileKind::File(path_str));
                } else if file_type.is_dir() {
                    file_kinds.insert(FileKind::Dir(path_str));
                }
            }
            for entry in file_kinds {
                match entry {
                    FileKind::File(file) if filter(&file) => {
                        action(FileKind::File(file), acc);
                    }
                    FileKind::Dir(dir) => {
                        iter_dir(depth + 1, max_depth, filter, file_only, action, acc, dir);
                    }
                    _ => {}
                }
            }
        }

        for path in paths {
            iter_dir(0, max_depth, &filter, file_only, &mut action, acc, path);
        }
    }

    pub(super) fn split_extension(s: String) -> (String, String) {
        if let Some(pos) = s.rfind('.') {
            (s[..pos].to_owned(), s[pos + 1..].to_owned())
        } else {
            (s, "".to_owned())
        }
    }
}

#[cfg(test)]
mod common {
    use crate::parser_env::ParseOptions;

    #[derive(Debug, Clone, Copy, Default)]
    pub(super) struct TestOptions {
        pub(super) intern_comments: bool,
    }

    /// Parse options from JSON content
    pub(super) fn parse_options(content: &str) -> Result<(TestOptions, ParseOptions), String> {
        let json: serde_json::Value =
            serde_json::from_str(content).map_err(|e| format!("JSON parse error: {}", e))?;

        let obj = json
            .as_object()
            .ok_or_else(|| "expected options to be a JSON object".to_string())?;

        let mut test_opts = TestOptions::default();
        let mut parse_opts = ParseOptions::default();

        for (key, value) in obj {
            match key.as_str() {
                "components" => {
                    parse_opts.components = value
                        .as_bool()
                        .ok_or_else(|| format!("invalid value for {:?}, expected bool", key))?;
                }
                "enums" => {
                    parse_opts.enums = value
                        .as_bool()
                        .ok_or_else(|| format!("invalid value for {:?}, expected bool", key))?;
                }
                "pattern_matching" => {
                    parse_opts.pattern_matching = value
                        .as_bool()
                        .ok_or_else(|| format!("invalid value for {:?}, expected bool", key))?;
                }
                "records" => {
                    parse_opts.records = value
                        .as_bool()
                        .ok_or_else(|| format!("invalid value for {:?}, expected bool", key))?;
                }
                "oxc_type_feature_parser_rules" => {
                    parse_opts.oxc_type_feature_parser_rules = value
                        .as_bool()
                        .ok_or_else(|| format!("invalid value for {:?}, expected bool", key))?;
                }
                "esproposal_decorators" => {
                    parse_opts.esproposal_decorators = value
                        .as_bool()
                        .ok_or_else(|| format!("invalid value for {:?}, expected bool", key))?;
                }
                "types" => {
                    parse_opts.types = value
                        .as_bool()
                        .ok_or_else(|| format!("invalid value for {:?}, expected bool", key))?;
                }
                "use_strict" => {
                    parse_opts.use_strict = value
                        .as_bool()
                        .ok_or_else(|| format!("invalid value for {:?}, expected bool", key))?;
                }
                "intern_comments" => {
                    test_opts.intern_comments = value
                        .as_bool()
                        .ok_or_else(|| format!("invalid value for {:?}, expected bool", key))?;
                }
                _ => return Err(format!("unknown option {:?}", key)),
            }
        }

        Ok((test_opts, parse_opts))
    }
}

#[cfg(test)]
mod esprima_tests {
    use std::collections::BTreeMap;
    use std::collections::HashMap;
    use std::fs;
    use std::ops::Deref;
    use std::sync::Arc;

    use dupe::Dupe;
    use flow_data_structure_wrapper::smol_str::FlowSmolStr;

    use crate::ast;
    use crate::loc::LOC_NONE;
    use crate::loc::Loc;
    use crate::loc_sig::LocSig;
    use crate::main_parser;
    use crate::parser_env::ParseOptions;
    use crate::parser_integration_tests::common::TestOptions;
    use crate::parser_integration_tests::common::parse_options;
    use crate::parser_integration_tests::file_utils;
    use crate::parser_integration_tests::file_utils::split_extension;

    #[derive(Debug, Clone)]
    enum CaseExpectation {
        Module,
        Tree(String),
        Tokens,
        Failure(String),
    }

    #[derive(Debug, Clone, Default)]
    enum CaseDiff {
        Diff(String),
        Todo(String),
        #[default]
        Same,
    }

    #[derive(Debug, Clone, Default)]
    struct Case {
        source: Option<Result<String, ()>>,
        options: (TestOptions, Option<ParseOptions>),
        expected: Option<CaseExpectation>,
        diff: CaseDiff,
        skipped: Vec<String>,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    enum PathPart {
        Prop(String),
        Index(usize),
    }

    #[derive(Debug, Clone)]
    struct Test {
        test_name: String,
        cases: BTreeMap<String, Case>,
    }

    #[derive(Debug)]
    struct Fix {
        path: Vec<String>,
        json: serde_json::Value,
    }

    #[derive(Debug)]
    enum CaseResult {
        CaseOk,
        CaseSkipped { reason: Option<String> },
        CaseError(Vec<(String, Option<Fix>)>),
    }

    // .source.js files look like `var source = 'X'`. the esprima runner evals this
    // and runs the test on the value of `source`. For example, to test invalid Unicode,
    // the invalid codepoints are validly encoded into the string; when eval'd, the
    // result is invalid, but the test file itself is valid UTF8.
    fn eval_source(content: &str) -> Option<Result<String, ()>> {
        let (ast::Program { statements, .. }, errors) =
            main_parser::parse_program_without_file(false, None, None, Ok(content));
        let stmt = statements.last()?;
        let ast::statement::StatementInner::VariableDeclaration { loc: _, inner } = stmt.deref()
        else {
            return None;
        };
        let declarations = &inner.declarations;
        if declarations.len() != 1 {
            return None;
        }
        let Some(ast::statement::variable::Declarator {
            loc: _,
            id:
                ast::pattern::Pattern::Identifier {
                    loc: _,
                    inner: id_inner,
                },
            init: Some(init_expr),
            definite: _,
        }) = declarations.first()
        else {
            return None;
        };
        let ast::expression::ExpressionInner::StringLiteral {
            loc: _,
            inner: str_inner,
        } = init_expr.deref()
        else {
            return None;
        };
        let name = &id_inner.name.name;
        let source = &str_inner.value;
        if name == "source" && !errors.is_empty() {
            // This is implementation defined. But lexer uses it to signal but utf8 encoding.
            Some(Err(()))
        } else {
            Some(Ok(source.as_str().to_owned()))
        }
    }

    fn tests_of_path(path: &str) -> Vec<Test> {
        let mut tests = Vec::new();

        fn action(root_path: &str, kind: file_utils::FileKind, tests: &mut Vec<Test>) {
            match kind {
                file_utils::FileKind::Dir(dir) => {
                    let test = Test {
                        test_name: dir.strip_prefix(root_path).unwrap().to_owned(),
                        cases: BTreeMap::new(),
                    };
                    tests.push(test)
                }
                file_utils::FileKind::File(file) => {
                    let Some(mut test) = tests.pop() else {
                        return;
                    };
                    let case_name = {
                        let file = file.strip_prefix(root_path).unwrap();
                        file.strip_prefix(&format!(
                            "{}{}",
                            test.test_name,
                            std::path::MAIN_SEPARATOR
                        ))
                        .unwrap_or(file)
                        .to_owned()
                    };
                    let (case_name, ext) = file_utils::split_extension(case_name);
                    match ext.as_str() {
                        "js" if case_name.ends_with(".source") => {
                            let case_name = case_name.strip_suffix(".source").unwrap().to_owned();
                            let mut case = test.cases.remove(&case_name).unwrap_or_default();
                            let content = fs::read_to_string(&file).unwrap();
                            if let Some(source) = eval_source(&content) {
                                case.source = Some(source);
                            } else {
                                case.skipped.push(file);
                            }
                            test.cases.insert(case_name, case);
                        }
                        "js" => {
                            let mut case = test.cases.remove(&case_name).unwrap_or_default();
                            let source = fs::read_to_string(&file).unwrap();
                            case.source = Some(Ok(source));
                            test.cases.insert(case_name, case);
                        }
                        "json" => {
                            let (case_name, kind) = split_extension(case_name);
                            let mut case = test.cases.remove(&case_name).unwrap_or_default();
                            let content = fs::read_to_string(&file).unwrap();
                            match kind.as_str() {
                                "module" => case.expected = Some(CaseExpectation::Module),
                                "tree" => case.expected = Some(CaseExpectation::Tree(content)),
                                "tokens" => case.expected = Some(CaseExpectation::Tokens),
                                "failure" => {
                                    case.expected = Some(CaseExpectation::Failure(content))
                                }
                                "options" => {
                                    let (test_options, parse_options) =
                                        parse_options(&content).unwrap();
                                    case.options = (test_options, Some(parse_options))
                                }
                                _ => {
                                    case.skipped.push(file);
                                }
                            };
                            test.cases.insert(case_name, case);
                        }
                        "diff" => {
                            let mut case = test.cases.remove(&case_name).unwrap_or_default();
                            let source = fs::read_to_string(&file).unwrap();
                            case.diff = CaseDiff::Diff(source);
                            test.cases.insert(case_name, case);
                        }
                        "skip" => {
                            let mut case = test.cases.remove(&case_name).unwrap_or_default();
                            let skip = fs::read_to_string(&file).unwrap().trim().to_owned();
                            case.diff = CaseDiff::Todo(skip);
                            test.cases.insert(case_name, case);
                        }
                        _ => {}
                    }
                    tests.push(test);
                }
            }
        }

        file_utils::iter_files(
            None,
            false,
            vec![path.to_owned()],
            &mut tests,
            |x| x != path,
            |kind, tests| action(path, kind, tests),
        );

        tests
    }

    fn expected_different_property(
        path: &[PathPart],
        expected: Option<&str>,
        actual: Option<&str>,
    ) -> bool {
        match (path, expected, actual) {
            // Don't ignore differences in errors at the top level
            ([PathPart::Index(_), PathPart::Prop(prop)], None, Some(_)) if prop == "errors" => {
                false
            }
            // Flow always includes comments and locations
            (_, None, Some("comments"))
            | (_, None, Some("loc"))
            | (_, None, Some("range"))
            | (_, None, Some("source"))
            // Esprima doesn't support type annotations
            | (_, None, Some("typeAnnotation"))
            | (_, None, Some("typeParameters"))
            | (_, None, Some("superTypeParameters"))
            | (_, None, Some("optional"))
            | (_, None, Some("returnType"))
            | (_, None, Some("predicate"))
            | (_, None, Some("implements"))
            | (_, None, Some("importKind"))
            | (_, None, Some("exportKind"))
            // Esprima doesn't support decorators
            // https://github.com/estree/estree/blob/master/experimental/decorators.md 
            | (_, None, Some("decorators"))
            // Esprima doesn't support async functions
            | (_, None, Some("async"))
            // Esprima doesn't support for-await
            | (_, None, Some("await"))
            // TODO: Flow should include this
            | ([], Some("sourceType"), None)
            // TODO: enable this in tests
            | ([], Some("tokens"), None)
            // Flow doesn't support this
            | (_, Some("innerComments"), None) => true,
            _ => false,
        }
    }

    fn matched_error_message(expected: &str) -> Option<&str> {
        // Check if expected matches the error regex pattern
        // Pattern: "^Error: Line [0-9]+: (.*)$"
        if expected.starts_with("Error: Line ") {
            // Find the message part after "Error: Line <num>: "
            if let Some(colon_pos) = expected[13..].find(": ") {
                let message_start = 13 + colon_pos + 2;
                let expected_message = &expected[message_start..];
                return Some(expected_message);
            }
        }
        None
    }

    fn string_value_matches(expected: &str, actual: &str) -> bool {
        if expected == actual {
            return true;
        }
        matched_error_message(expected) == Some(actual)
    }

    fn map_of_properties<M: Dupe, T: Dupe>(
        props: &[ast::expression::object::Property<M, T>],
    ) -> HashMap<String, &ast::expression::Expression<M, T>> {
        let mut map = HashMap::new();

        for prop in props {
            match prop {
                ast::expression::object::Property::NormalProperty(
                    ast::expression::object::NormalProperty::Init {
                        loc: _,
                        key:
                            ast::expression::object::Key::StringLiteral((
                                _,
                                ast::StringLiteral {
                                    value: name,
                                    raw: _,
                                    comments: _,
                                },
                            )),
                        value,
                        shorthand: false,
                    },
                ) => {
                    map.insert(name.as_str().to_owned(), value);
                }
                _ => {
                    panic!("Invalid JSON")
                }
            }
        }

        map
    }

    fn string_of_path(path: &[PathPart]) -> String {
        if path.is_empty() {
            return "root".to_string();
        }

        let mut result = String::from("root");
        for part in path.iter() {
            match part {
                PathPart::Prop(p) => {
                    result.push('.');
                    result.push_str(p);
                }
                PathPart::Index(i) => {
                    result.push('[');
                    result.push_str(&i.to_string());
                    result.push(']');
                }
            }
        }
        result
    }

    fn string_of_json_type(json: &serde_json::Value) -> &'static str {
        match json {
            serde_json::Value::Object(_) => "object",
            serde_json::Value::Array(_) => "array",
            serde_json::Value::String(_) => "string",
            serde_json::Value::Number(_) => "number",
            serde_json::Value::Bool(_) => "bool",
            serde_json::Value::Null => "null",
        }
    }

    fn test_tree(
        path: &mut Vec<PathPart>,
        errors: &mut Vec<(String, Option<Fix>)>,
        actual: &serde_json::Value,
        expected: &ast::expression::Expression<Loc, Loc>,
    ) {
        use serde_json::Value as Json;

        match (actual, expected.deref()) {
            (Json::Object(aprops), ast::expression::ExpressionInner::Object { loc: _, inner }) => {
                let eprops = &inner.properties;
                let emap = map_of_properties(eprops);
                for (name, value) in aprops.iter() {
                    test_actual_prop(path, errors, &emap, name, value);
                }
                for name in emap.keys() {
                    test_expected_prop(path, errors, aprops, name);
                }
            }
            (Json::Array(aitems), ast::expression::ExpressionInner::Array { loc: _, inner }) => {
                let eitems = &inner.elements;
                let a_len = aitems.len();
                let e_len = eitems.len();
                if e_len != a_len {
                    let path_str = string_of_path(path);
                    let err = format!("{}: Expected {} elements, got {}.", path_str, e_len, a_len);
                    errors.push((err, None));
                    return;
                }
                for (i, (actual_item, expected_item)) in
                    aitems.iter().zip(eitems.iter()).enumerate()
                {
                    path.push(PathPart::Index(i));
                    match expected_item {
                        ast::expression::ArrayElement::Expression(expr) => {
                            test_tree(path, errors, actual_item, expr);
                        }
                        _ => {
                            let path_str = string_of_path(path);
                            errors.push((format!("{}: invalid JSON", path_str), None));
                        }
                    }
                    path.pop();
                }
            }
            (
                Json::Bool(actual_val),
                ast::expression::ExpressionInner::BooleanLiteral { loc: _, inner },
            ) => {
                let expected_val = &inner.value;
                if actual_val != expected_val {
                    let path_str = string_of_path(path);
                    let err = format!(
                        "{}: Expected `{}`, got `{}`.",
                        path_str, expected_val, actual_val
                    );
                    errors.push((err, None));
                }
            }
            (
                Json::Number(actual_val),
                ast::expression::ExpressionInner::NumberLiteral { loc: _, inner },
            ) => {
                let expected = &inner.raw;
                let actual_str = actual_val.to_string();
                if actual_str.as_str() != expected.as_str()
                    && actual_str.parse::<f64>() != expected.parse::<f64>()
                {
                    let path_str = string_of_path(path);
                    let err = format!(
                        "{}: Expected `{}`, got `{}`.",
                        path_str, expected, actual_str
                    );
                    errors.push((err, None));
                }
            }
            (
                Json::Number(actual_val),
                ast::expression::ExpressionInner::Unary {
                    loc: _,
                    inner: unary_inner,
                },
            ) if unary_inner.operator == ast::expression::UnaryOperator::Minus => {
                if let ast::expression::ExpressionInner::NumberLiteral {
                    loc: _,
                    inner: num_inner,
                } = &*unary_inner.argument
                {
                    let expected = &num_inner.raw;
                    let expected_str = format!("-{}", expected);
                    let actual_str = actual_val.to_string();
                    if actual_str != expected_str {
                        let path_str = string_of_path(path);
                        let err = format!(
                            "{}: Expected `{}`, got `{}`.",
                            path_str, expected_str, actual_str
                        );
                        errors.push((err, None));
                    }
                } else {
                    let path_str = string_of_path(path);
                    let act_type = string_of_json_type(actual);
                    let err = format!("{}: Types do not match, got {}", path_str, act_type);
                    errors.push((err, None));
                }
            }
            (
                Json::String(actual_val),
                ast::expression::ExpressionInner::StringLiteral { loc: _, inner },
            ) => {
                let expected = &inner.value;
                if !string_value_matches(expected, actual_val) {
                    let path_str = string_of_path(path);
                    let err = format!(
                        "{}: Expected `{}`, got `{}`.",
                        path_str, expected, actual_val
                    );
                    errors.push((err, None));
                }
            }
            (Json::Null, ast::expression::ExpressionInner::NullLiteral { .. }) => {}
            (_, _) => {
                let path_str = string_of_path(path);
                let act_type = string_of_json_type(actual);
                let err = format!("{}: Types do not match, got {}", path_str, act_type);
                errors.push((err, None));
            }
        }
    }

    fn test_actual_prop(
        path: &mut Vec<PathPart>,
        errors: &mut Vec<(String, Option<Fix>)>,
        expected_map: &HashMap<String, &ast::expression::Expression<Loc, Loc>>,
        name: &str,
        value: &serde_json::Value,
    ) {
        if let Some(expected_value) = expected_map.get(name) {
            path.push(PathPart::Prop(name.to_string()));
            test_tree(path, errors, value, expected_value);
            path.pop();
        } else if value == &serde_json::Value::Null {
            // Allow Flow to have extra properties when their values are null.
            // This is a narrower exception than `expected_different_property`;
            // that function allows the field to have any value, but sometimes
            // we want to allow Flow to return consistent shapes, without allowing
            // arbitrary differences.
        } else if expected_different_property(path, None, Some(name)) {
        } else {
            let path_str = string_of_path(path);
            let err = format!("{}: Unexpected key {:?}", path_str, name);
            errors.push((err, None));
        }
    }

    fn test_expected_prop(
        path: &[PathPart],
        errors: &mut Vec<(String, Option<Fix>)>,
        actual_map: &serde_json::Map<String, serde_json::Value>,
        name: &str,
    ) {
        if actual_map.contains_key(name) || expected_different_property(path, Some(name), None) {
            return;
        }
        let path_str = string_of_path(path);
        let err = format!("{}: Missing key {:?}", path_str, name);
        errors.push((err, None));
    }

    fn prop_name_and_value<M: Dupe, T: Dupe>(
        prop: &ast::expression::object::NormalProperty<M, T>,
    ) -> (&str, &ast::expression::Expression<M, T>) {
        match prop {
            ast::expression::object::NormalProperty::Init {
                loc: _,
                key:
                    ast::expression::object::Key::StringLiteral((
                        _,
                        ast::StringLiteral {
                            value: name,
                            raw: _,
                            comments: _,
                        },
                    )),
                value,
                shorthand: false,
            } => (name, value),
            _ => panic!("Invalid JSON"),
        }
    }

    fn into_prop_name_and_value<M: Dupe, T: Dupe>(
        prop: ast::expression::object::NormalProperty<M, T>,
    ) -> (FlowSmolStr, ast::expression::Expression<M, T>) {
        match prop {
            ast::expression::object::NormalProperty::Init {
                loc: _,
                key:
                    ast::expression::object::Key::StringLiteral((
                        _,
                        ast::StringLiteral {
                            value: name,
                            raw: _,
                            comments: _,
                        },
                    )),
                value,
                shorthand: false,
            } => (name, value),
            _ => panic!("Invalid JSON"),
        }
    }

    fn has_prop<M: Dupe, T: Dupe>(
        needle: &str,
        haystack: &[ast::expression::object::Property<M, T>],
    ) -> bool {
        haystack.iter().any(|prop| match prop {
            ast::expression::object::Property::NormalProperty(normal_prop) => {
                let (name, _) = prop_name_and_value(normal_prop);
                name == needle
            }
            _ => false,
        })
    }

    fn test_failure(
        expected: &serde_json::Value,
        actual: &serde_json::Value,
        diff: Option<&serde_json::Value>,
    ) -> Vec<(String, Option<Fix>)> {
        let expected_message = if let Some(message) = diff.and_then(|diff| diff.get("message")) {
            message.as_str().unwrap()
        } else {
            expected.get("message").unwrap().as_str().unwrap()
        };
        let expected_message = if let Some(matched) = matched_error_message(expected_message) {
            matched
        } else {
            expected_message
        };
        match actual.get("errors") {
            Some(serde_json::Value::Array(errors)) if errors.is_empty() => {
                vec![("Expected errors but Flow didn't error".to_string(), None)]
            }
            None => vec![("Expected errors but Flow didn't error".to_string(), None)],
            Some(serde_json::Value::Array(errors)) => {
                let first_error = &errors[0];
                let actual_message = first_error.get("message").unwrap().as_str().unwrap();

                let mut errs = Vec::new();
                // TODO: also check line and column
                if expected_message != actual_message {
                    let err = format!(
                        "Expected message {:?}, got {:?}",
                        expected_message, actual_message
                    );
                    let fix = Some(Fix {
                        path: vec!["message".to_string()],
                        json: serde_json::Value::String(actual_message.to_string()),
                    });
                    errs.push((err, fix));
                }
                errs
            }
            Some(_) => panic!("Expected 'errors' to be an array in actual result"),
        }
    }

    fn apply_diff(
        diff: ast::expression::Expression<Loc, Loc>,
        expected: ast::expression::Expression<Loc, Loc>,
    ) -> Option<ast::expression::Expression<Loc, Loc>> {
        match diff.deref() {
            ast::expression::ExpressionInner::Object {
                loc: _,
                inner: diff_inner,
            } => {
                let diff_props = &diff_inner.properties;
                match expected.deref() {
                    ast::expression::ExpressionInner::Object {
                        loc,
                        inner: expected_inner,
                    } => {
                        let expected_props = &expected_inner.properties;
                        let comments = expected_inner.comments.dupe();
                        let mut properties: Vec<_> = expected_props.iter().cloned().collect();
                        for diff_prop in diff_props.iter() {
                            match diff_prop {
                                ast::expression::object::Property::NormalProperty(
                                    diff_normal_prop,
                                ) => {
                                    let (diff_name, diff_value) =
                                        prop_name_and_value(diff_normal_prop);

                                    if !has_prop(diff_name, &properties) {
                                        properties.push(
                                            ast::expression::object::Property::NormalProperty(
                                                diff_normal_prop.clone(),
                                            ),
                                        );
                                    } else {
                                        let mut new_properties = Vec::new();
                                        for exp_prop in properties.iter() {
                                            match exp_prop {
                                                ast::expression::object::Property::NormalProperty(
                                                    exp_normal_prop,
                                                ) => {
                                                    let (exp_name, _) =
                                                        prop_name_and_value(exp_normal_prop);

                                                    if exp_name == diff_name {
                                                        let (loc,key) = match exp_normal_prop {
                                                                ast::expression::object::NormalProperty::Init { loc,key, .. } => (loc.dupe(), key.clone()),
                                                                _ => panic!("Invalid JSON"),
                                                            };
                                                        let (_, exp_value) =
                                                            into_prop_name_and_value(exp_normal_prop.clone());
                                                        if let Some(value) =
                                                            apply_diff(diff_value.clone(), exp_value)
                                                        {
                                                            let new_prop = ast::expression::object::Property::NormalProperty(
                                                                ast::expression::object::NormalProperty::Init {
                                                                    loc ,
                                                                    key,
                                                                    value,
                                                                    shorthand: false,
                                                                }
                                                            );
                                                            new_properties.push(new_prop);
                                                        }
                                                    } else {
                                                        new_properties.push( ast::expression::object::Property::NormalProperty(
                                                    exp_normal_prop.clone(),
                                                ));
                                                    }
                                                }
                                                _ => {
                                                    new_properties.push(exp_prop.clone());
                                                }
                                            }
                                        }
                                        properties = new_properties;
                                    }
                                }
                                _ => panic!("Invalid JSON"),
                            }
                        }
                        Some(ast::expression::Expression::new(
                            ast::expression::ExpressionInner::Object {
                                loc: loc.dupe(),
                                inner: Arc::new(ast::expression::Object {
                                    properties: properties.into(),
                                    comments,
                                }),
                            },
                        ))
                    }
                    ast::expression::ExpressionInner::Array {
                        loc,
                        inner: expected_inner,
                    } => {
                        let expected_elems = &expected_inner.elements;
                        let comments = expected_inner.comments.dupe();
                        let expected_length = expected_elems.len();
                        let mut elements: Vec<_> = expected_elems.iter().cloned().collect();
                        for diff_prop in diff_props.iter() {
                            match diff_prop {
                                ast::expression::object::Property::NormalProperty(
                                    diff_normal_prop,
                                ) => {
                                    let (diff_name, diff_value) =
                                        into_prop_name_and_value(diff_normal_prop.clone());
                                    let diff_index: usize =
                                        diff_name.parse().expect("Invalid array index");

                                    if diff_index >= expected_length {
                                        // Append the diff
                                        // TODO: this should insert gaps, but I don't expect people to
                                        // write diffs that have gaps.
                                        elements.push(ast::expression::ArrayElement::Expression(
                                            diff_value,
                                        ));
                                    } else {
                                        elements = elements
                                            .into_iter()
                                            .enumerate()
                                            .map(|(index, elem)| {
                                                if index != diff_index {
                                                    elem
                                                } else {
                                                    match elem {
                                                        ast::expression::ArrayElement::Hole(_) => {
                                                            ast::expression::ArrayElement::Expression(
                                                                diff_value.clone()
                                                            )
                                                        }
                                                        ast::expression::ArrayElement::Expression(
                                                            exp_value,
                                                        ) => {
                                                            if let Some(value) =
                                                                apply_diff(diff_value.clone(), exp_value)
                                                            {
                                                                ast::expression::ArrayElement::Expression(value)
                                                            } else {
                                                                ast::expression::ArrayElement::Hole(
                                                                    LOC_NONE,
                                                                )
                                                            }
                                                        }
                                                        ast::expression::ArrayElement::Spread { .. } => {
                                                            panic!("Invalid JSON")
                                                        }
                                                    }
                                                }
                                            })
                                            .collect();
                                    }
                                }
                                _ => panic!("Invalid JSON"),
                            }
                        }

                        Some(ast::expression::Expression::new(
                            ast::expression::ExpressionInner::Array {
                                loc: loc.dupe(),
                                inner: Arc::new(ast::expression::Array {
                                    elements: elements.into(),
                                    trailing_comma: false,
                                    comments,
                                }),
                            },
                        ))
                    }
                    _ => Some(expected),
                }
            }
            ast::expression::ExpressionInner::StringLiteral { .. }
            | ast::expression::ExpressionInner::NumberLiteral { .. }
            | ast::expression::ExpressionInner::BooleanLiteral { .. }
            | ast::expression::ExpressionInner::NullLiteral { .. }
            | ast::expression::ExpressionInner::RegExpLiteral { .. } => Some(diff),
            ast::expression::ExpressionInner::Identifier { loc: _, inner }
                if inner.name == "undefined" =>
            {
                None
            }
            _ => panic!("Invalid diff format"),
        }
    }

    fn parse_file(
        (test_options, parse_options): &(TestOptions, Option<ParseOptions>),
        content: Result<&str, ()>,
    ) -> serde_json::Value {
        main_parser::parse_to_json(parse_options.clone(), test_options.intern_comments, content)
    }

    fn run_case(case: &Case) -> CaseResult {
        let Some(content) = &case.source else {
            if case.skipped.is_empty() && matches!(case.diff, CaseDiff::Same) {
                return CaseResult::CaseError(vec![("No source".to_string(), None)]);
            } else {
                return CaseResult::CaseSkipped { reason: None };
            }
        };

        let content = match content {
            Ok(s) => Ok(s.as_str()),
            Err(()) => Err(()),
        };
        let actual = parse_file(&case.options, content);
        let (diff, todo) = match &case.diff {
            CaseDiff::Diff(str) => (Some(str), None),
            CaseDiff::Todo(str) => (None, Some(str)),
            CaseDiff::Same => (None, None),
        };
        match &case.expected {
            Some(CaseExpectation::Module) => CaseResult::CaseSkipped { reason: None },
            Some(CaseExpectation::Tree(tree)) => {
                let (expected, json_errors) =
                    main_parser::parse_json_file(false, None, None, None, Ok(tree));
                if let Some((loc, err)) = json_errors.first() {
                    let err_str = format!(
                        "Unable to parse .tree.json: {:?}: {:?}\n\nContents: {}",
                        loc.debug_to_string(false),
                        err,
                        tree
                    );
                    return CaseResult::CaseError(vec![(err_str, None)]);
                }
                let expected = if let Some(diff_str) = diff {
                    let (diffs, _) =
                        main_parser::parse_json_file(true, None, None, None, Ok(diff_str));
                    match apply_diff(diffs, expected) {
                        Some(x) => x,
                        None => {
                            panic!("unexpected diff: removed everything")
                        }
                    }
                } else {
                    expected
                };
                let mut path = Vec::new();
                let mut errors = Vec::new();
                test_tree(&mut path, &mut errors, &actual, &expected);
                match (errors.is_empty(), todo) {
                    (true, None) => CaseResult::CaseOk,
                    (true, Some(_)) => {
                        CaseResult::CaseError(vec![("Skipped test passes".to_string(), None)])
                    }
                    (false, Some(reason)) => CaseResult::CaseSkipped {
                        reason: Some(reason.to_owned()),
                    },
                    (false, None) => CaseResult::CaseError(errors),
                }
            }
            Some(CaseExpectation::Tokens) => CaseResult::CaseSkipped { reason: None },
            Some(CaseExpectation::Failure(contents)) => {
                let expected: Result<serde_json::Value, String> =
                    serde_json::from_str(contents).map_err(|e| e.to_string());
                let diff: Result<Option<serde_json::Value>, String> = match diff {
                    Some(str) => serde_json::from_str(str)
                        .map(Some)
                        .map_err(|e| e.to_string()),
                    None => Ok(None),
                };

                match (expected, diff) {
                    (Ok(expected), Ok(diff)) => {
                        let errors = test_failure(&expected, &actual, diff.as_ref());
                        match (errors.is_empty(), todo) {
                            (true, None) => CaseResult::CaseOk,
                            (true, Some(_)) => CaseResult::CaseError(vec![(
                                "Skipped test passes".to_string(),
                                None,
                            )]),
                            (false, Some(reason)) => CaseResult::CaseSkipped {
                                reason: Some(reason.to_owned()),
                            },
                            (false, None) => CaseResult::CaseError(errors),
                        }
                    }
                    (Err(err), _) => CaseResult::CaseError(vec![(err, None)]),
                    (_, Err(err)) => {
                        CaseResult::CaseError(vec![(format!("Invalid .diff: {}", err), None)])
                    }
                }
            }
            None => CaseResult::CaseError(vec![("Nothing to do".to_string(), None)]),
        }
    }

    #[derive(Debug, Clone, Copy, Default)]
    struct TestResults {
        ok: usize,
        skipped: usize,
        failed: usize,
        crashed: usize,
    }

    #[derive(Debug, Clone, Copy, Default)]
    struct SuiteResults {
        ok_tests: usize,
        ok_cases: usize,
        skipped_cases: usize,
        failed_tests: usize,
        failed_cases: usize,
        crashed_cases: usize,
    }

    impl SuiteResults {
        fn add(&mut self, test: TestResults) {
            self.ok_cases += test.ok;
            self.skipped_cases += test.skipped;
            self.failed_cases += test.failed;
            self.crashed_cases += test.crashed;
        }
    }

    fn record_tree(path: &str, test_name: &str, case_name: &str, case: &Case) -> bool {
        match (&case.source, &case.expected) {
            (Some(content), None) | (Some(content), Some(CaseExpectation::Tree(_))) => {
                let filename = format!(
                    "{}{}{}{}{}.tree.json",
                    path,
                    std::path::MAIN_SEPARATOR,
                    test_name,
                    std::path::MAIN_SEPARATOR,
                    case_name
                );
                let json = parse_file(
                    &case.options,
                    match content {
                        Ok(s) => Ok(s.as_str()),
                        Err(_) => Err(()),
                    },
                );
                let json_str = serde_json::to_string_pretty(&json).unwrap();
                fs::write(&filename, format!("{}\n", json_str)).expect(&filename);
                true
            }
            _ => false,
        }
    }

    fn write_diff(
        path: &str,
        test_name: &str,
        case_name: &str,
        case: &Case,
        errs: &[(String, Option<Fix>)],
    ) -> bool {
        match &case.expected {
            Some(CaseExpectation::Failure(_)) => {
                let filename = format!(
                    "{}{}{}{}{}.diff",
                    path,
                    std::path::MAIN_SEPARATOR,
                    test_name,
                    std::path::MAIN_SEPARATOR,
                    case_name
                );
                let props: Vec<_> = errs
                    .iter()
                    .filter_map(|(_, fix)| {
                        if let Some(Fix { path, json }) = fix {
                            if let Some(name) = path.first() {
                                // Failures are flat objects so we don't need to handle deep paths
                                return Some((name.clone(), json.clone()));
                            }
                        }
                        None
                    })
                    .collect();

                if !props.is_empty() {
                    let json = serde_json::Value::Object(props.into_iter().collect());
                    let json_str = serde_json::to_string_pretty(&json).unwrap();
                    fs::write(&filename, format!("{}\n", json_str)).expect(&filename);
                    true
                } else {
                    false
                }
            }
            _ => {
                // TODO: handle writing other kinds of diffs
                false
            }
        }
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum VerboseMode {
        Quiet,
        Verbose,
        #[expect(dead_code)]
        Normal,
    }

    fn run_internal(path: &str, verbose_mode: VerboseMode, record: bool, diff: bool) {
        let tests = tests_of_path(path);
        let mut suite_results = SuiteResults::default();
        for Test { test_name, cases } in tests {
            let mut result = TestResults::default();
            let mut shown_header = false;
            if verbose_mode != VerboseMode::Quiet {
                eprintln!("=== {} ===", test_name);
            }
            for (key, case) in cases {
                match std::panic::catch_unwind(|| run_case(&case)) {
                    Ok(CaseResult::CaseOk) => {
                        if verbose_mode == VerboseMode::Verbose {
                            eprintln!("PASS: {}", key);
                        }
                        result.ok += 1;
                    }
                    Ok(CaseResult::CaseSkipped { reason }) => {
                        if let Some(reason) = reason {
                            if !reason.is_empty() && verbose_mode != VerboseMode::Quiet {
                                eprintln!("[-] SKIP: {} - {}", key, reason);
                            }
                        }
                        result.skipped += 1;
                    }
                    Ok(CaseResult::CaseError(errs)) => {
                        if verbose_mode == VerboseMode::Quiet && !shown_header {
                            eprintln!("=== {} ===", test_name);
                        }
                        eprintln!("[x] FAIL: {}", key);
                        for (err, _) in &errs {
                            eprintln!("    {}", err);
                        }
                        let fixed = record && record_tree(path, &test_name, &key, &case);
                        if !fixed && diff {
                            write_diff(path, &test_name, &key, &case, &errs);
                        }
                        // TODO: hardcoded allowed failure. They are just different in errors.
                        // If sam has time, let's finish this.
                        if test_name.as_str() != "/ES6/identifier"
                            || &key != "invalid_lone_surrogate"
                        {
                            result.failed += 1;
                            shown_header = true;
                        }
                    }
                    Err(_) => {
                        if verbose_mode == VerboseMode::Quiet && !shown_header {
                            eprintln!("=== {} ===", test_name);
                        }
                        eprintln!("[x] DEAD: {}", key);
                        result.crashed += 1;
                        shown_header = true;
                    }
                }
            }
            if result.failed > 0 || result.crashed > 0 {
                suite_results.failed_tests += 1;
            } else {
                suite_results.ok_tests += 1;
            }
            suite_results.add(result);
        }
        if suite_results.failed_tests == 0 {
            eprintln!(
                "Passed: {} ({} cases), Failed: {}, Skipped: {} cases",
                suite_results.ok_tests,
                suite_results.ok_cases,
                suite_results.failed_tests,
                suite_results.skipped_cases
            );
        } else {
            eprintln!(
                "Passed: {} ({} cases), Failed: {} ({} cases failed, {} cases crashed), Skipped: {} cases",
                suite_results.ok_tests,
                suite_results.ok_cases,
                suite_results.failed_tests,
                suite_results.failed_cases,
                suite_results.crashed_cases,
                suite_results.skipped_cases
            );
            panic!("FAILURE")
        }
    }

    fn resource_path() -> String {
        match buck_resources::get("flow/rust_port/crates/flow_parser/esprima_tests") {
            Ok(resource_dir) => resource_dir.join("esprima").to_str().unwrap().to_string(),
            Err(_) => {
                if let Some(cargo_manifest_dir) = option_env!("CARGO_MANIFEST_DIR") {
                    std::path::Path::new(cargo_manifest_dir)
                        .join("../../../src/parser/test/esprima")
                        .to_str()
                        .unwrap()
                        .to_string()
                } else {
                    panic!(
                        "esprima tests resource not found and CARGO_MANIFEST_DIR is unavailable"
                    );
                }
            }
        }
    }

    #[test]
    fn run() {
        let path = resource_path();
        run_internal(&path, VerboseMode::Quiet, false, false);
    }
}

#[cfg(test)]
mod hardcoded_tests {
    use std::collections::BTreeMap;
    use std::fs;

    use crate::file_key::has_ts_ext;
    use crate::main_parser;
    use crate::parser_env::ParseOptions;
    use crate::parser_integration_tests::common::TestOptions;
    use crate::parser_integration_tests::common::parse_options;
    use crate::parser_integration_tests::file_utils;
    use crate::parser_integration_tests::file_utils::split_extension;

    #[derive(Debug, Clone, Default)]
    struct TestCase {
        content: Option<String>,
        filename: Option<String>,
        expected_ast: Option<String>,
        options: (TestOptions, ParseOptions),
    }

    #[derive(Debug, Clone)]
    struct Test {
        test_name: String,
        cases: BTreeMap<String, TestCase>,
    }

    #[derive(Debug)]
    enum CaseResult {
        CaseOk,
        CaseNoSnapshot,
        CaseError(Vec<String>),
    }

    fn tests_of_path(path: &str) -> Vec<Test> {
        let mut tests = Vec::new();

        fn action(root_path: &str, kind: file_utils::FileKind, tests: &mut Vec<Test>) {
            match kind {
                file_utils::FileKind::Dir(dir) => {
                    let test = Test {
                        test_name: dir.strip_prefix(root_path).unwrap().to_owned(),
                        cases: BTreeMap::new(),
                    };
                    tests.push(test)
                }
                file_utils::FileKind::File(file) => {
                    let Some(mut test) = tests.pop() else {
                        return;
                    };
                    let case_name = {
                        let file = file.strip_prefix(root_path).unwrap();
                        file.strip_prefix(&format!(
                            "{}{}",
                            test.test_name,
                            std::path::MAIN_SEPARATOR
                        ))
                        .unwrap_or(file)
                        .to_owned()
                    };
                    let (case_name, ext) = file_utils::split_extension(case_name);
                    match ext.as_str() {
                        "js" => {
                            let mut case = test.cases.remove(&case_name).unwrap_or_default();
                            let content = fs::read_to_string(&file).unwrap();
                            case.content = Some(content);
                            case.filename = Some(file.clone());
                            test.cases.insert(case_name, case);
                        }
                        "flow" => {
                            // For .js.flow files, strip the .js extension too (e.g., foo.js.flow -> foo)
                            let case_name = if case_name.ends_with(".js") {
                                case_name.strip_suffix(".js").unwrap().to_owned()
                            } else {
                                case_name
                            };
                            let mut case = test.cases.remove(&case_name).unwrap_or_default();
                            let content = fs::read_to_string(&file).unwrap();
                            case.content = Some(content);
                            case.filename = Some(file.clone());
                            // .flow files are ambient context
                            case.options.1.ambient = true;
                            test.cases.insert(case_name, case);
                        }
                        "ts" | "tsx" | "mts" | "cts" => {
                            let case_name = case_name
                                .strip_suffix(".d")
                                .unwrap_or(&case_name)
                                .to_owned();
                            let mut case = test.cases.remove(&case_name).unwrap_or_default();
                            let content = fs::read_to_string(&file).unwrap();
                            case.content = Some(content);
                            case.filename = Some(file.clone());
                            test.cases.insert(case_name, case);
                        }
                        "json" => {
                            let (case_name, kind) = split_extension(case_name);
                            let mut case = test.cases.remove(&case_name).unwrap_or_default();
                            let content = fs::read_to_string(&file).unwrap();
                            match kind.as_str() {
                                "tree" => {
                                    case.expected_ast = Some(content);
                                }
                                "options" => {
                                    let (test_options, parse_options) =
                                        parse_options(&content).unwrap();
                                    case.options = (test_options, parse_options)
                                }
                                _ => {}
                            };
                            test.cases.insert(case_name, case);
                        }
                        _ => {}
                    }
                    tests.push(test);
                }
            }
        }

        file_utils::iter_files(
            None,
            false,
            vec![path.to_owned()],
            &mut tests,
            |x| x != path,
            |kind, tests| action(path, kind, tests),
        );

        tests
    }

    fn parse_file(
        (test_options, parse_options): &(TestOptions, ParseOptions),
        content: &str,
        filename: Option<&str>,
    ) -> serde_json::Value {
        if filename.is_some_and(has_ts_ext) {
            use crate::comment_utils;
            use crate::estree_translator;
            use crate::file_key::FileKey;
            use crate::file_key::FileKeyInner;
            use crate::offset_utils;

            let offset_table = offset_utils::OffsetTable::make_with_kind(
                offset_utils::OffsetKind::JavaScript,
                content,
            );
            let file_key = FileKey::new(FileKeyInner::SourceFile(filename.unwrap().to_owned()));
            let (mut ast, errors) = main_parser::parse_program_file::<()>(
                false,
                None,
                Some(parse_options.clone()),
                file_key,
                Ok(content),
            );
            if !test_options.intern_comments {
                comment_utils::strip_inlined_comments(&mut ast);
            }
            let mut result = estree_translator::program(
                &offset_table,
                &estree_translator::Config {
                    include_locs: true,
                    include_filename: false,
                    offset_style: estree_translator::OffsetStyle::JsIndices,
                },
                &ast,
            );
            match &mut result {
                serde_json::Value::Object(obj) => {
                    if !errors.is_empty() {
                        obj.insert(
                            "errors".to_owned(),
                            estree_translator::errors(
                                &offset_table,
                                false,
                                estree_translator::OffsetStyle::JsIndices,
                                &errors,
                            ),
                        );
                    }
                }
                _ => unreachable!(),
            }
            result
        } else {
            main_parser::parse_to_json(
                Some(parse_options.clone()),
                test_options.intern_comments,
                Ok(content),
            )
        }
    }

    fn compare_json(
        path: &str,
        actual: &serde_json::Value,
        expected: &serde_json::Value,
    ) -> Vec<String> {
        let mut errors = Vec::new();

        match (actual, expected) {
            (serde_json::Value::Object(actual_obj), serde_json::Value::Object(expected_obj)) => {
                for (key, expected_val) in expected_obj {
                    if let Some(actual_val) = actual_obj.get(key) {
                        let new_path = format!("{}.{}", path, key);
                        errors.extend(compare_json(&new_path, actual_val, expected_val));
                    } else {
                        errors.push(format!("{}: Missing key {:?}", path, key));
                    }
                }
            }
            (serde_json::Value::Array(actual_arr), serde_json::Value::Array(expected_arr)) => {
                if actual_arr.len() != expected_arr.len() {
                    errors.push(format!(
                        "{}: Expected array length {}, got {}",
                        path,
                        expected_arr.len(),
                        actual_arr.len()
                    ));
                    return errors;
                }

                for (i, (actual_item, expected_item)) in
                    actual_arr.iter().zip(expected_arr.iter()).enumerate()
                {
                    let new_path = format!("{}[{}]", path, i);
                    errors.extend(compare_json(&new_path, actual_item, expected_item));
                }
            }
            (actual, expected) if actual == expected => {}
            (actual, expected) => {
                errors.push(format!(
                    "{}: Expected `{}`, got `{}`",
                    path,
                    serde_json::to_string(expected).unwrap(),
                    serde_json::to_string(actual).unwrap(),
                ));
            }
        }

        errors
    }

    fn run_case(case: &TestCase) -> CaseResult {
        let Some(ref content) = case.content else {
            return CaseResult::CaseError(vec!["No source file".to_string()]);
        };

        let Some(ref expected_ast_str) = case.expected_ast else {
            return CaseResult::CaseNoSnapshot;
        };

        let actual = parse_file(&case.options, content, case.filename.as_deref());

        let expected: Result<serde_json::Value, _> = serde_json::from_str(expected_ast_str);
        let expected = match expected {
            Ok(e) => e,
            Err(err) => {
                return CaseResult::CaseError(vec![format!(
                    "Failed to parse expected AST: {}",
                    err
                )]);
            }
        };

        let errors = compare_json("root", &actual, &expected);

        if errors.is_empty() {
            CaseResult::CaseOk
        } else {
            CaseResult::CaseError(errors)
        }
    }

    #[derive(Debug, Clone, Copy, Default)]
    struct TestResults {
        ok: usize,
        skipped: usize,
        failed: usize,
        crashed: usize,
    }

    #[derive(Debug, Clone, Copy, Default)]
    struct SuiteResults {
        ok_tests: usize,
        ok_cases: usize,
        skipped_cases: usize,
        failed_tests: usize,
        failed_cases: usize,
        crashed_cases: usize,
    }

    impl SuiteResults {
        fn add(&mut self, test: TestResults) {
            self.ok_cases += test.ok;
            self.skipped_cases += test.skipped;
            self.failed_cases += test.failed;
            self.crashed_cases += test.crashed;
        }
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    #[allow(dead_code)]
    enum VerboseMode {
        Quiet,
        Verbose,
        Normal,
    }

    fn snapshot_filename(source_filename: &str) -> String {
        let (mut stem, extension) = split_extension(source_filename.to_owned());
        if extension == "flow" && stem.ends_with(".js") {
            stem.truncate(stem.len() - 3);
        } else if matches!(extension.as_str(), "ts" | "tsx" | "mts" | "cts") && stem.ends_with(".d")
        {
            stem.truncate(stem.len() - 2);
        }
        format!("{stem}.tree.json")
    }

    #[test]
    fn recording_uses_the_source_path_and_normalizes_compound_extensions() {
        for (source, expected) in [
            (
                "/root/arrow_function/migrated_0006.js",
                "/root/arrow_function/migrated_0006.tree.json",
            ),
            (
                "/root/types/example.js.flow",
                "/root/types/example.tree.json",
            ),
            ("/root/types/example.d.ts", "/root/types/example.tree.json"),
            ("/root/types/example.mts", "/root/types/example.tree.json"),
        ] {
            assert_eq!(snapshot_filename(source), expected);
        }
    }

    fn record_tree(case: &TestCase) -> bool {
        let (Some(content), Some(source_filename)) = (&case.content, &case.filename) else {
            return false;
        };
        let filename = snapshot_filename(source_filename);
        let json = parse_file(&case.options, content, case.filename.as_deref());
        let json_str = serde_json::to_string_pretty(&json).unwrap();
        fs::write(&filename, format!("{}\n", json_str))
            .unwrap_or_else(|err| panic!("failed to write {}: {}", filename, err));
        true
    }

    fn run_internal(path: &str, verbose_mode: VerboseMode, record: bool) {
        let tests = tests_of_path(path);
        let mut suite_results = SuiteResults::default();

        for Test { test_name, cases } in tests {
            let mut result = TestResults::default();
            let mut shown_header = false;

            if verbose_mode != VerboseMode::Quiet {
                eprintln!("=== {} ===", test_name);
            }

            for (key, case) in cases {
                match std::panic::catch_unwind(|| run_case(&case)) {
                    Ok(CaseResult::CaseOk) => {
                        if verbose_mode == VerboseMode::Verbose {
                            eprintln!("PASS: {}", key);
                        }
                        result.ok += 1;
                    }
                    Ok(CaseResult::CaseNoSnapshot) => {
                        if verbose_mode != VerboseMode::Quiet {
                            eprintln!("[-] SKIP: {} - no .tree.json", key);
                        }
                        if record && record_tree(&case) {
                            eprintln!("[+] RECORD: {}", key);
                        }
                        result.skipped += 1;
                    }
                    Ok(CaseResult::CaseError(errs)) => {
                        if verbose_mode == VerboseMode::Quiet && !shown_header {
                            eprintln!("=== {} ===", test_name);
                        }
                        eprintln!("[x] FAIL: {}", key);
                        for err in &errs {
                            eprintln!("    {}", err);
                        }
                        if record && record_tree(&case) {
                            eprintln!("[+] RECORD: {}", key);
                        }
                        result.failed += 1;
                        shown_header = true;
                    }
                    Err(_) => {
                        if verbose_mode == VerboseMode::Quiet && !shown_header {
                            eprintln!("=== {} ===", test_name);
                        }
                        eprintln!("[x] DEAD: {}", key);
                        result.crashed += 1;
                        shown_header = true;
                    }
                }
            }

            if result.failed > 0 || result.crashed > 0 {
                suite_results.failed_tests += 1;
            } else {
                suite_results.ok_tests += 1;
            }
            suite_results.add(result);
        }

        if suite_results.failed_tests == 0 {
            eprintln!(
                "Passed: {} ({} cases), Failed: {}, Skipped: {} cases",
                suite_results.ok_tests,
                suite_results.ok_cases,
                suite_results.failed_tests,
                suite_results.skipped_cases
            );
        } else {
            eprintln!(
                "Passed: {} ({} cases), Failed: {} ({} cases failed, {} cases crashed), Skipped: {} cases",
                suite_results.ok_tests,
                suite_results.ok_cases,
                suite_results.failed_tests,
                suite_results.failed_cases,
                suite_results.crashed_cases,
                suite_results.skipped_cases
            );
            panic!("FAILURE")
        }
    }

    fn resource_path() -> String {
        match buck_resources::get("flow/rust_port/crates/flow_parser/flow_tests") {
            Ok(resource_dir) => resource_dir.join("flow").to_str().unwrap().to_string(),
            Err(_) => {
                if let Some(cargo_manifest_dir) = option_env!("CARGO_MANIFEST_DIR") {
                    std::path::Path::new(cargo_manifest_dir)
                        .join("../../../src/parser/test/flow")
                        .to_str()
                        .unwrap()
                        .to_string()
                } else {
                    panic!("flow tests resource not found and CARGO_MANIFEST_DIR is unavailable");
                }
            }
        }
    }

    #[test]
    fn run() {
        // Buck copies the fixtures into its build output tree, so recording has
        // to be pointed at the source tree to land anywhere useful.
        match std::env::var("FLOW_PARSER_TEST_RECORD") {
            Ok(path) if !path.is_empty() => run_internal(&path, VerboseMode::Quiet, true),
            _ => run_internal(&resource_path(), VerboseMode::Quiet, false),
        }
    }
}
