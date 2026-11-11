/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use flow_parser_utils::ast_builder::expressions as E;
use flow_parser_utils::ast_builder::patterns;
use flow_parser_utils::ast_builder::statements as S;
use flow_parser_utils::ast_builder::types as T;

use crate::js_layout_generator;
use crate::layout::LayoutNode;
use crate::layout_generator_test_utils::*;
use crate::layout_test_utils::layout_builder as L;
use crate::layout_test_utils::*;

#[test]
fn class_expressions() {
    assert_expression_string(false, None, None, "class{}");
    assert_expression_string(false, None, None, "class a{}");
    assert_expression_string(false, None, None, "class a extends b{}");
}

#[test]
fn class_methods() {
    assert_statement_string(false, None, "class a{b(){}}");
    assert_statement_string(true, None, "class a {\n  b() {}\n  static b() {}\n}");
    assert_statement_string(
        true,
        None,
        "class a {\n  async a() {}\n  static async a() {}\n}",
    );
    assert_statement_string(
        true,
        None,
        "class a {\n  get a() {}\n  set a() {}\n  static get a() {}\n}",
    );
    assert_statement_string(true, None, "class a {\n  constructor() {}\n}");
    assert_statement_string(false, None, "class a{@a a(){}}");
    assert_statement_string(false, None, "class a{@(()=>{}) a(){}}");
    assert_statement_string(false, None, "class a{@a@b a(){}}");
    assert_statement_string(true, None, "class a {\n  @a\n  a() {}\n}");
    assert_statement_string(true, None, "class a {\n  @a\n  @b\n  a() {}\n}");
    assert_statement_string(false, None, "class a{*b(){}}");
}

#[test]
fn class_private_methods() {
    assert_statement_string(false, None, "class a{#a(){}}");
    assert_statement_string(false, None, "class a{static #a(){}}");
}

#[test]
fn class_properties() {
    assert_statement_string(false, None, "class a{a;}");
    assert_statement_string(false, None, "class a{a:a;}");
    assert_statement_string(false, None, "class a{a;b=c;}");
    assert_statement_string(false, None, "class a{a;b:b=c;}");
    assert_statement_string(false, None, "class a{+a;}");
    assert_statement_string(false, None, "class a{+a:a=a;}");
    assert_statement_string(false, None, "class a{static a;}");
    assert_statement_string(false, None, "class a{static +a:a=a;}");
    assert_statement_string(true, None, "class a {\n  a;\n  b = c;\n  static b = c;\n}");
    assert_statement_string(true, None, "class a {\n  +a: a;\n  b: b = c;\n}");
}

#[test]
fn class_private_properties() {
    assert_statement_string(false, None, "class a{#a;}");
    assert_statement_string(false, None, "class a{#a:a;}");
    assert_statement_string(false, None, "class a{#a;#b=c;}");
    assert_statement_string(false, None, "class a{#a;#b:b=c;}");
    assert_statement_string(false, None, "class a{+#a;}");
    assert_statement_string(false, None, "class a{+#a:a=a;}");
    assert_statement_string(false, None, "class a{static #a;}");
    assert_statement_string(false, None, "class a{static +#a:a=a;}");
    assert_statement_string(
        true,
        None,
        "class a {\n  #a;\n  #b = c;\n  static #b = c;\n}",
    );
    assert_statement_string(true, None, "class a {\n  +#a: a;\n  #b: b = c;\n}");
}

#[test]
fn class_static_blocks() {
    assert_statement_string(false, None, "class C{static{}}");
    assert_statement_string(true, None, "class C {\n  static {}\n}");
    assert_statement_string(false, None, "class C{static{let x=0}}");
    assert_statement_string(true, None, "class C {\n  static {\n    let x = 0;\n  }\n}");
}

#[test]
fn class_declared_fields() {
    assert_statement_string(false, None, "class a{declare x;}");
    assert_statement_string(false, None, "class a{declare x:string;}");
}

#[test]
fn class_ts_accessibility() {
    assert_statement_string(false, None, "class C{private b(){}}");
    assert_statement_string(false, None, "class C{protected b(){}}");
    assert_statement_string(false, None, "class C{public b(){}}");
    assert_statement_string(false, None, "class C{private static b(){}}");
    assert_statement_string(false, None, "class C{protected static b(){}}");
    assert_statement_string(false, None, "class C{public static b(){}}");
    assert_statement_string(
        true,
        None,
        "class a {\n  private b() {}\n  protected c() {}\n  public d() {}\n}",
    );
    assert_statement_string(false, None, "class C{private x;}");
    assert_statement_string(false, None, "class C{protected x;}");
    assert_statement_string(false, None, "class C{public x;}");
    assert_statement_string(false, None, "class C{private static x;}");
    assert_statement_string(false, None, "class C{protected static x;}");
    assert_statement_string(false, None, "class C{public static x;}");
    assert_statement_string(false, None, "class C{public readonly x;}");
    assert_statement_string(false, None, "class C{public static readonly x;}");
    assert_statement_string(
        true,
        None,
        "class C {\n  private x;\n  protected y;\n  public z;\n}",
    );
}

#[test]
fn declare_class_ts_accessibility() {
    // accessibility without annotation
    assert_statement_string(false, None, "declare class C{private b}");
    assert_statement_string(false, None, "declare class C{private readonly b}");
    assert_statement_string(false, None, "declare class C{private static b}");
    // accessibility with annotation
    assert_statement_string(false, None, "declare class C{private b:T}");
    assert_statement_string(false, None, "declare class C{protected b:T}");
    assert_statement_string(false, None, "declare class C{public b:T}");
    // accessibility with static
    assert_statement_string(false, None, "declare class C{protected static b:T}");
    assert_statement_string(false, None, "declare class C{public static b:T}");
    assert_statement_string(false, None, "declare class C{private static readonly b:T}");
    // accessibility with methods
    assert_statement_string(false, None, "declare class C{private b():T}");
    assert_statement_string(false, None, "declare class C{protected b():T}");
    assert_statement_string(false, None, "declare class C{public b():T}");
    // accessibility modifiers as property names
    assert_statement_string(false, None, "declare class C{private:T}");
    assert_statement_string(false, None, "declare class C{protected:T}");
    assert_statement_string(false, None, "declare class C{public:T}");
    // pretty printed
    assert_statement_string(
        true,
        None,
        "declare class C {\n  private b,\n  private readonly c,\n  protected d: T,\n}",
    );
}

#[test]
fn declare_class_property_value_initializer() {
    assert_statement_string(
        true,
        None,
        "declare class Foo {\n  static readonly BUFFER_SIZE = 256,\n}",
    );
    assert_statement_string(
        true,
        None,
        "declare class Foo {\n  readonly code = \"NEXT_STATIC_GEN_BAILOUT\",\n}",
    );
    assert_statement_string(true, None, "declare class Foo {\n  readonly neg = -1,\n}");
    assert_statement_string(true, None, "declare class Foo {\n  readonly x? = 1,\n}");
    assert_statement_string(
        true,
        None,
        "declare class Foo {\n  private readonly x = 42,\n}",
    );
    assert_statement_string(
        true,
        None,
        "declare class Foo {\n  protected readonly x = 42,\n}",
    );
    assert_statement_string(
        true,
        None,
        "declare class Foo {\n  readonly [\n    Symbol.iterator\n  ] = 0,\n}",
    );
}

#[test]
fn class_ts_parameter_properties() {
    assert_statement_string(false, None, "class C{constructor(private x:number){}}");
    assert_statement_string(false, None, "class C{constructor(protected x:string){}}");
    assert_statement_string(false, None, "class C{constructor(public x:boolean){}}");
    assert_statement_string(false, None, "class C{constructor(public x:boolean=true){}}");
    assert_statement_string(
        true,
        None,
        "class C {\n  constructor(private x: number, protected y: string) {}\n}",
    );
}

#[test]
fn abstract_classes() {
    assert_statement_string(false, None, "abstract class C{}");
    assert_statement_string(false, None, "abstract class C extends B{}");
    assert_statement_string(false, None, "abstract class C{a(){}}");
    assert_statement_string(true, None, "abstract class C {\n  a() {}\n}");
    assert_statement_string(false, None, "abstract class C{abstract a():void;}");
    assert_statement_string(true, None, "abstract class C {\n  abstract a(): void;\n}");
    assert_statement_string(
        true,
        None,
        "abstract class C {\n  abstract a(): void;\n  b() {}\n}",
    );
}

#[test]
fn abstract_classes_with_accessibility() {
    assert_statement_string(false, None, "abstract class C{abstract a:string;}");
    assert_statement_string(
        false,
        None,
        "abstract class C{protected abstract a:string;}",
    );
    assert_statement_string(false, None, "abstract class C{public abstract a:string;}");
    assert_statement_string(
        false,
        None,
        "abstract class C{protected abstract a():void;}",
    );
    assert_statement_string(false, None, "abstract class C{public abstract a():void;}");
}

#[test]
fn class_override() {
    // override methods
    assert_statement_string(false, None, "class a extends b{override a(){}}");
    assert_statement_string(true, None, "class a\n  extends b {\n  override a() {}\n}");
    assert_statement_string(false, None, "class a extends b{static override a(){}}");
    assert_statement_string(
        true,
        None,
        "class a\n  extends b {\n  static override a() {}\n}",
    );
    // override properties
    assert_statement_string(false, None, "class a extends b{override a:string;}");
    assert_statement_string(
        true,
        None,
        "class a\n  extends b {\n  override a: string;\n}",
    );
    // override getters/setters
    assert_statement_string(false, None, "class a extends b{override get a(){return 1}}");
    assert_statement_string(false, None, "class a extends b{override set a(v){}}");
    // override with abstract
    assert_statement_string(
        false,
        None,
        "abstract class a extends b{abstract override a():void;}",
    );
    assert_statement_string(
        false,
        None,
        "abstract class a extends b{abstract override a:string;}",
    );
}

#[test]
fn definite_assignment() {
    assert_statement_string(false, None, "let a!:number;");
    assert_statement_string(true, None, "let a!: number;");
    assert_statement_string(false, None, "var a!:number,b!:string;");
    assert_statement_string(false, None, "class a{b!:number;#c!:string;}");
    assert_statement_string(true, None, "class a {\n  b!: number;\n  #c!: string;\n}");
}

#[test]
fn declare_class_override() {
    // override methods
    assert_statement_string(false, None, "declare class a extends b{override a():void}");
    assert_statement_string(
        true,
        None,
        "declare class a\n  extends b {\n  override a(): void,\n}",
    );
    assert_statement_string(
        false,
        None,
        "declare class a extends b{static override a():void}",
    );
    // override properties
    assert_statement_string(false, None, "declare class a extends b{override a:string}");
    assert_statement_string(
        true,
        None,
        "declare class a\n  extends b {\n  override a: string,\n}",
    );
    // override getters/setters
    assert_statement_string(
        false,
        None,
        "declare class a extends b{override get a():number}",
    );
    assert_statement_string(
        true,
        None,
        "declare class a\n  extends b {\n  override get a(): number,\n}",
    );
}

#[test]
fn declare_abstract_classes() {
    assert_statement_string(false, None, "declare abstract class C{}");
    assert_statement_string(false, None, "declare abstract class C{a():void}");
    assert_statement_string(true, None, "declare abstract class C {\n  a(): void,\n}");
    assert_statement_string(false, None, "declare abstract class C{abstract a():void}");
    assert_statement_string(
        true,
        None,
        "declare abstract class C {\n  abstract a(): void,\n}",
    );
}

#[test]
fn forin_statement_declaration() {
    let mk_layout = |a: &str, b: &str| -> LayoutNode {
        js_layout_generator::statement(
            &opts(),
            false,
            &S::for_in(
                None,
                S::for_in_declarator(
                    None,
                    None,
                    vec![S::variable_declarator(None, None, None, a)],
                ),
                E::identifier(None, None, b),
                S::block(
                    None,
                    vec![S::expression(
                        None,
                        None,
                        None,
                        E::identifier(None, None, a),
                    )],
                ),
            ),
        )
    };
    {
        let layout = mk_layout("a", "b");
        assert_layout(
            L::loc(
                None,
                L::fused(vec![
                    L::atom("for"),
                    L::pretty_space(),
                    L::group(vec![
                        L::atom("("),
                        L::loc(
                            None,
                            L::fused(vec![
                                L::atom("var"),
                                L::space(),
                                L::loc(None, L::loc(None, L::id(None, "a"))),
                            ]),
                        ),
                        L::space(),
                        L::atom("in"),
                        L::space(),
                        L::loc(None, L::id(None, "b")),
                        L::atom(")"),
                    ]),
                    L::pretty_space(),
                    L::loc(
                        None,
                        L::loc(
                            None,
                            L::group(vec![
                                L::atom("{"),
                                L::indent(L::fused(vec![
                                    L::pretty_hardline(),
                                    L::loc(
                                        None,
                                        L::fused(vec![
                                            L::loc(None, L::id(None, "a")),
                                            LayoutNode::if_pretty(L::atom(";"), L::empty()),
                                        ]),
                                    ),
                                ])),
                                L::pretty_hardline(),
                                L::atom("}"),
                            ]),
                        ),
                    ),
                ]),
            ),
            layout.clone(),
        );
        assert_output(false, "for(var a in b){a}", &layout);
        assert_output(true, "for (var a in b) {\n  a;\n}", &layout);
    }

    let a80 = "a".repeat(80);
    let layout = mk_layout(&a80, "b");
    assert_output(false, &format!("for(var {} in b){{{}}}", a80, a80), &layout);
    assert_output(
        true,
        &format!("for (var {} in b) {{\n  {};\n}}", a80, a80),
        &layout,
    );
}

#[test]
fn forin_statement_pattern_identifier() {
    let mk_layout = |a: &str, b: &str| -> LayoutNode {
        js_layout_generator::statement(
            &opts(),
            false,
            &S::for_in(
                None,
                S::for_in_pattern(patterns::identifier(None, None, a)),
                E::identifier(None, None, b),
                S::block(None, vec![]),
            ),
        )
    };
    {
        let layout = mk_layout("a", "b");
        assert_layout(
            L::loc(
                None,
                L::fused(vec![
                    L::atom("for"),
                    L::pretty_space(),
                    L::group(vec![
                        L::atom("("),
                        L::loc(None, L::id(None, "a")),
                        L::space(),
                        L::atom("in"),
                        L::space(),
                        L::loc(None, L::id(None, "b")),
                        L::atom(")"),
                    ]),
                    L::pretty_space(),
                    L::loc(None, L::loc(None, L::atom("{}"))),
                ]),
            ),
            layout.clone(),
        );
        assert_output(false, "for(a in b){}", &layout);
        assert_output(true, "for (a in b) {}", &layout);
    }

    let a80 = "a".repeat(80);
    let layout = mk_layout(&a80, "b");
    assert_output(false, &format!("for({} in b){{}}", a80), &layout);
    assert_output(true, &format!("for ({} in b) {{}}", a80), &layout);
}

#[test]
fn forin_statement_without_block() {
    assert_statement_string(false, None, "for(a in b)x;");
    assert_statement_string(false, None, "{for(a in b)x}");
}

#[test]
fn forin_empty_body() {
    let layout = js_layout_generator::statement(
        &opts(),
        false,
        &S::for_in(
            None,
            S::for_in_pattern(patterns::identifier(None, None, "a")),
            E::identifier(None, None, "b"),
            S::empty(None),
        ),
    );
    assert_output(false, "for(a in b);", &layout);
    assert_output(true, "for (a in b);", &layout);
}

#[test]
fn forof_statement_declaration() {
    let mk_layout = |a: &str, b: &str| -> LayoutNode {
        js_layout_generator::statement(
            &opts(),
            false,
            &S::for_of(
                None,
                None,
                S::for_of_declarator(
                    None,
                    None,
                    vec![S::variable_declarator(None, None, None, a)],
                ),
                E::identifier(None, None, b),
                S::block(
                    None,
                    vec![S::expression(
                        None,
                        None,
                        None,
                        E::identifier(None, None, a),
                    )],
                ),
            ),
        )
    };
    {
        let layout = mk_layout("a", "b");
        assert_layout(
            L::loc(
                None,
                L::fused(vec![
                    L::atom("for"),
                    L::pretty_space(),
                    L::group(vec![
                        L::atom("("),
                        L::loc(
                            None,
                            L::fused(vec![
                                L::atom("var"),
                                L::space(),
                                L::loc(None, L::loc(None, L::id(None, "a"))),
                            ]),
                        ),
                        L::space(),
                        L::atom("of"),
                        L::space(),
                        L::loc(None, L::id(None, "b")),
                        L::atom(")"),
                    ]),
                    L::pretty_space(),
                    L::loc(
                        None,
                        L::loc(
                            None,
                            L::group(vec![
                                L::atom("{"),
                                L::indent(L::fused(vec![
                                    L::pretty_hardline(),
                                    L::loc(
                                        None,
                                        L::fused(vec![
                                            L::loc(None, L::id(None, "a")),
                                            LayoutNode::if_pretty(L::atom(";"), L::empty()),
                                        ]),
                                    ),
                                ])),
                                L::pretty_hardline(),
                                L::atom("}"),
                            ]),
                        ),
                    ),
                ]),
            ),
            layout.clone(),
        );
        assert_output(false, "for(var a of b){a}", &layout);
        assert_output(true, "for (var a of b) {\n  a;\n}", &layout);
    }

    let a80 = "a".repeat(80);
    let layout = mk_layout(&a80, "b");
    assert_output(false, &format!("for(var {} of b){{{}}}", a80, a80), &layout);
    assert_output(
        true,
        &format!("for (var {} of b) {{\n  {};\n}}", a80, a80),
        &layout,
    );
}

#[test]
fn forof_statement_pattern_identifier() {
    let mk_layout = |a: &str, b: &str| -> LayoutNode {
        js_layout_generator::statement(
            &opts(),
            false,
            &S::for_of(
                None,
                None,
                S::for_of_pattern(patterns::identifier(None, None, a)),
                E::identifier(None, None, b),
                S::block(None, vec![]),
            ),
        )
    };
    {
        let layout = mk_layout("a", "b");
        assert_layout(
            L::loc(
                None,
                L::fused(vec![
                    L::atom("for"),
                    L::pretty_space(),
                    L::group(vec![
                        L::atom("("),
                        L::loc(None, L::id(None, "a")),
                        L::space(),
                        L::atom("of"),
                        L::space(),
                        L::loc(None, L::id(None, "b")),
                        L::atom(")"),
                    ]),
                    L::pretty_space(),
                    L::loc(None, L::loc(None, L::atom("{}"))),
                ]),
            ),
            layout.clone(),
        );
        assert_output(false, "for(a of b){}", &layout);
        assert_output(true, "for (a of b) {}", &layout);
    }

    let a80 = "a".repeat(80);
    let layout = mk_layout(&a80, "b");
    assert_output(false, &format!("for({} of b){{}}", a80), &layout);
    assert_output(true, &format!("for ({} of b) {{}}", a80), &layout);
}

#[test]
fn forof_statement_async() {
    assert_statement_string(false, None, "async function f(){for await(let x of y){}}");
}

#[test]
fn forof_statement_without_block() {
    assert_statement_string(false, None, "for(a of b)x;");
    assert_statement_string(false, None, "{for(a of b)x}");
}

#[test]
fn forof_empty_body() {
    let layout = js_layout_generator::statement(
        &opts(),
        false,
        &S::for_of(
            None,
            None,
            S::for_of_pattern(patterns::identifier(None, None, "a")),
            E::identifier(None, None, "b"),
            S::empty(None),
        ),
    );
    assert_output(false, "for(a of b);", &layout);
    assert_output(true, "for (a of b);", &layout);
}

#[test]
fn forof_space() {
    assert_statement_string(false, None, "for(let x of y);");
    assert_statement_string(true, None, "for (let x of y);");
    assert_statement_string(false, None, "for(let x of[]);");
    assert_statement_string(true, None, "for (let x of []);");
    assert_statement_string(false, None, "for(let{x,y}of z);");
    assert_statement_string(true, None, "for (let {x, y} of z);");
}

#[test]
fn forof_sequence() {
    assert_statement_string(true, None, "for (var a of ([1], [2]));");
    assert_statement_string(false, None, "for(var a of([1],[2]));");
    assert_statement_string(true, None, "for (var a of ([1], [2], [3]));");
    assert_statement_string(true, None, "for (var a of ([1], ([2], [3])));");
    assert_statement_string(true, None, "for (var a of (([1], [2]), [3]));");
}

#[test]
fn forof_async() {
    // the parens are required
    assert_statement_string(false, None, "for((async)of y);");
    assert_statement_string(true, None, "for ((async) of y);");
    assert_statement_string(true, None, "for ((/* needs parens */ async) of y);");
}

#[test]
fn yield_expressions() {
    assert_expression_string(false, None, None, "function* f(){yield}");
    assert_expression_string(false, None, None, "function* f(){yield a}");
    assert_expression_string(false, None, None, "function* f(){yield* a}");
}

#[test]
fn meta_property_expressions() {
    assert_statement_string(false, None, "function F(){new.target}");
    assert_statement_string(false, None, "function F(){new.target.name}");
}

#[test]
fn tagged_template_expressions() {
    assert_expression_string(false, None, None, "a``");
    assert_expression_string(false, None, None, "b.c``");
    assert_expression_string(false, None, None, "(()=>{})``");
    assert_expression_string(false, None, None, "(b=c)``");
    assert_expression_string(false, None, None, "(b+c)``");
    assert_expression_string(false, None, None, "b()``");
    assert_expression_string(false, None, None, "(class{})``");
    assert_expression_string(false, None, None, "(b?c:d)``");
    assert_expression_string(false, None, None, "(function(){})``");
    assert_expression_string(false, None, None, "(b||c)``");
    assert_expression_string(false, None, None, "(new B())``");
    assert_expression_string(false, None, None, "({})``");
    assert_expression_string(false, None, None, "(b,c)``");
    assert_expression_string(false, None, None, "````");
    assert_expression_string(false, None, None, "(void b)``");
    assert_expression_string(false, None, None, "(++b)``");
}

#[test]
fn template_expressions() {
    assert_expression_string(false, None, None, "``");
    assert_expression_string(false, None, None, "`${a}`");
    assert_expression_string(false, None, None, "`a${b}c`");
    assert_expression_string(false, None, None, "`a${b}c${d}e`");
    assert_expression_string(false, None, None, "`\\``");
}

#[test]
fn template_literal_type() {
    assert_statement_string(false, None, "type T=`foo ${S}`;");
    assert_statement_string(false, None, "type T=`hello`;");
    assert_statement_string(false, None, "type T=`${A} and ${B}`;");
    assert_statement_string(false, None, "type T=``;");
}

#[test]
fn import_expressions() {
    assert_expression_string(false, None, None, "import(\"a\")");
    assert_expression_string(false, None, None, "import(a)");
}

#[test]
fn export_declaration_statement() {
    assert_statement_string(false, None, "export{};");
    assert_statement_string(false, None, "export{}from\"a\";");
    assert_statement_string(false, None, "export{a}from\"a\";");
    assert_statement_string(false, None, "export{a,b as c};");
    assert_statement_string(false, None, "export*from\"a\";");
    assert_statement_string(false, None, "export*as a from\"a\";");
    assert_statement_string(false, None, "export type{};");
    assert_statement_string(false, None, "export type{a};");
    assert_statement_string(false, None, "export type a=b;");
    assert_statement_string(false, None, "export let a;");
    assert_statement_string(false, None, "export const a=b;");
    assert_statement_string(false, None, "export interface a{a():b}");
    assert_statement_string(true, None, "export {};");
    assert_statement_string(true, None, "export { a } from \"a\";");
    assert_statement_string(
        true,
        Some(&no_bracket_spacing(&opts())),
        "export {a} from \"a\";",
    );
    assert_statement_string(true, None, "export * from \"a\";");
    assert_statement_string(true, None, "export * as a from \"a\";");
    assert_statement_string(true, None, "export type { a };");
    assert_statement_string(true, Some(&no_bracket_spacing(&opts())), "export type {a};");
    assert_statement_string(
        true,
        None,
        &format!(
            "export {{\n  a,\n  b as {},\n}} from \"a\";",
            "c".repeat(80)
        ),
    );
    assert_statement_string(
        true,
        None,
        &format!("export * as {} from \"a\";", "a".repeat(80)),
    );
    assert_statement_string(true, None, "export opaque type a = b;");
    assert_statement_string(false, None, "export{type a};");
    assert_statement_string(false, None, "export{a,type b};");
    assert_statement_string(false, None, "export{type a}from\"a\";");
    assert_statement_string(true, None, "export { type a };");
    assert_statement_string(true, None, "export { a, type b };");
    assert_statement_string(false, None, "export{type a as b};");
    assert_statement_string(true, None, "export { type a as b };");
}
// TODO: Flow does not parse this but should
// assert_statement_string(false, None, "export a,{b}from'a';");
// assert_statement_string(false, None, "export*as foo,{bar}from'a';");

#[test]
fn default_export_declaration_statement() {
    assert_statement_string(false, None, "export default a;");
    assert_statement_string(false, None, "export default a=b;");
    assert_statement_string(false, None, "export default function(){}");
    assert_statement_string(false, None, "export default class{}");
}

#[test]
fn export_assignment_statement() {
    assert_statement_string(false, None, "export=foo;");
    assert_statement_string(false, None, "export={a:1,b:2};");
    assert_statement_string(true, None, "export = foo;");
    assert_statement_string(true, None, "export = { a: 1, b: 2 };");
    assert_statement_string_with_filename(
        true,
        "test.d.cts",
        None,
        "export = function foo(x: string): number;",
    );
    assert_statement_string_with_filename(
        false,
        "test.d.cts",
        None,
        "export=function foo(x:string):number;",
    );
    assert_statement_string(false, None, "export=function foo(){};");
    assert_statement_string(true, None, "export = function foo() {};");
}

#[test]
fn type_alias_statement() {
    assert_statement_string(false, None, "type a=a;");
    assert_statement_string(false, None, "type a<a>=a;");
    assert_statement_string(true, None, "type a = a;");
}

#[test]
fn conditional_types() {
    assert_statement_string(true, None, "type conditional = a extends b ? c : d;");
    assert_statement_string(false, None, "type conditional=a extends b?c:d;");
    assert_statement_string(
        true,
        None,
        "type conditional = a extends (a extends b ? c : d) ? c : d;",
    );
    assert_statement_string(
        true,
        None,
        "type conditional = aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa extends bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb\n  ? ccccccccccccc\n  : dddddd;",
    );
}

#[test]
fn opaque_type_alias_statement() {
    assert_statement_string(false, None, "opaque type a=a;");
    assert_statement_string(false, None, "opaque type a:b=a;");
    assert_statement_string(false, None, "opaque type a<a>=a;");
    assert_statement_string(false, None, "opaque type a<a>:b<a>=a;");
    assert_statement_string(false, None, "opaque type a<a>:b<a>=c<a>;");
    assert_statement_string(true, None, "opaque type a = a;");
    assert_statement_string(true, None, "opaque type a: b = a;");
    assert_statement_string(true, None, "opaque type a super b extends c = a;");
}

#[test]
fn declare_opaque_type_alias_statement() {
    assert_statement_string(false, None, "declare opaque type a;");
    assert_statement_string(false, None, "declare opaque type a:b;");
    assert_statement_string(true, None, "declare opaque type a: b;");
    assert_statement_string(false, None, "declare export opaque type a;");
    assert_statement_string(false, None, "declare export opaque type a:b;");
    assert_statement_string(true, None, "declare export opaque type a: b;");
}

#[test]
fn type_cast_expression() {
    let layout = js_layout_generator::expression(
        &opts(),
        None,
        &E::typecast(
            None,
            None,
            E::identifier(None, None, "a"),
            T::mixed(None, None),
        ),
    );
    assert_layout(
        L::loc(
            None,
            L::group(vec![
                L::atom("("),
                L::loc(None, L::id(None, "a")),
                L::loc(
                    None,
                    L::fused(vec![
                        L::atom(":"),
                        L::pretty_space(),
                        L::loc(None, L::atom("mixed")),
                    ]),
                ),
                L::atom(")"),
            ]),
        ),
        layout.clone(),
    );
    assert_output(false, "(a:mixed)", &layout);
    assert_output(true, "(a: mixed)", &layout);

    let a80 = "a".repeat(80);
    let layout = js_layout_generator::expression(
        &opts(),
        None,
        &E::typecast(
            None,
            None,
            E::identifier(None, None, &a80),
            T::mixed(None, None),
        ),
    );
    assert_output(false, &format!("({}:mixed)", a80), &layout);
    assert_output(true, &format!("({}: mixed)", a80), &layout);
    assert_statement_string(true, None, "var a = (b: mixed);");
    // Arrow function with type params is wrapped in parens
    assert_expression_string(false, None, None, "((<A>()=>B):C)");
}

#[test]
fn readonly_variance() {
    assert_statement_string(false, None, "type a={readonly foo:string};");
    assert_statement_string(false, None, "type a={readonly [string]:mixed};");
}

#[test]
fn in_out_variance() {
    assert_statement_string(false, None, "type T<in A>=A;");
    assert_statement_string(false, None, "type T<out A>=A;");
    assert_statement_string(false, None, "type T<in out A>=A;");
}

#[test]
fn as_expression() {
    assert_expression_string(false, None, None, "a as T");
}

#[test]
fn as_expression_precedence() {
    assert_expression_string(true, None, None, "(x ?? a) as T");
}

#[test]
fn as_const_expression() {
    assert_expression_string(false, None, None, "a as const");
}

#[test]
fn as_const_expression_precedence() {
    assert_expression_string(true, None, None, "(x ?? a) as const");
}

#[test]
fn satisfies_expression() {
    assert_expression_string(false, None, None, "a satisfies T");
}

#[test]
fn type_parameter() {
    assert_statement_string(false, None, "type a<a>=a;");
    assert_statement_string(false, None, "type a<a,b>=a;");
    assert_statement_string(false, None, "type a<+a>=a;");
    assert_statement_string(false, None, "type a<c=a>=a;");
    assert_statement_string(false, None, "type a<a:b>=a;");
    assert_statement_string(false, None, "type a<a:b=c>=a;");
    assert_statement_string(false, None, "type a<a,+a:b=c>=a;");
    assert_statement_string(true, None, "type a<a, +a: b = c> = a;");
    assert_statement_string(
        true,
        None,
        &format!("type a<\n  a,\n  +a: b = {},\n> = a;", "c".repeat(80)),
    );
    assert_statement_string(
        true,
        None,
        &format!("type a<\n  a,\n  b,\n> = {};", "a".repeat(80)),
    );
    assert_statement_string(false, None, "type a<a extends b>=a;");
    assert_statement_string(false, None, "type a<+a extends b>=a;");
    assert_statement_string(false, None, "type a<a extends b=c>=a;");
    assert_statement_string(false, None, "type a<+a extends b=c>=a;");
}

#[test]
fn type_() {
    assert_output(
        false,
        "mixed",
        &js_layout_generator::type_(&opts(), &T::mixed(None, None)),
    );
    assert_output(
        false,
        "empty",
        &js_layout_generator::type_(&opts(), &T::empty(None, None)),
    );
    assert_output(
        false,
        "void",
        &js_layout_generator::type_(&opts(), &T::void(None, None)),
    );
    assert_statement_string(false, None, "type a=mixed;");
    assert_statement_string(false, None, "type a=any;");
    assert_statement_string(false, None, "type a=mixed;");
    assert_statement_string(false, None, "type a=empty;");
    assert_statement_string(false, None, "type a=void;");
    assert_statement_string(false, None, "type a=null;");
    assert_statement_string(false, None, "type a=number;");
    assert_statement_string(false, None, "type a=string;");
    assert_statement_string(false, None, "type a=boolean;");
    assert_statement_string(false, None, "type a=a;");
    assert_statement_string(false, None, "type a=?a;");
    assert_statement_string(true, None, "type a = ?a;");
    assert_statement_string(false, None, "type a=Array<a>;");
    assert_statement_string(false, None, "type a=a.b;");
    assert_statement_string(false, None, "type a=a.b.c;");
    assert_statement_string(false, None, "type a=a<b>;");
    assert_statement_string(false, None, "type a=a.b<c,d>;");
    assert_statement_string(true, None, "type a = a.b<c, d>;");
    assert_statement_string(
        true,
        None,
        &format!("type a = a.b<\n  c,\n  {},\n>;", "d".repeat(80)),
    );
    assert_statement_string(false, None, "type a=O.readonly;");
    assert_statement_string(true, None, "type a = O.readonly;");
    assert_statement_string(false, None, "type a=O.infer;");
    assert_statement_string(true, None, "type a = O.infer;");
    assert_statement_string(false, None, "type a=typeof a;");
    assert_statement_string(true, None, "type a = typeof import(\"foo\");");
    assert_statement_string(true, None, "type a = typeof import(\"bar\").baz;");
    assert_statement_string(true, None, "type a = typeof this;");
    assert_statement_string(true, None, "type a = typeof this.property;");
    assert_statement_string(false, None, "type a=[a,b];");
    assert_statement_string(true, None, "type a = [a, b];");
    assert_statement_string(
        true,
        None,
        &format!("type a = [\n  a,\n  {},\n];", "b".repeat(80)),
    );
    assert_statement_string(false, None, "type a=*;");
    assert_statement_string(false, None, "type a=\"\";");
    assert_statement_string(false, None, "type a=1;");
    assert_statement_string(false, None, "type a=true;");
    assert_statement_string(false, None, "type a=unknown;");
    assert_statement_string(false, None, "type a=never;");
    assert_statement_string(false, None, "type a=undefined;");
    assert_statement_string(false, None, "type a=keyof O;");
    assert_statement_string(false, None, "type a=readonly T;");
    assert_statement_string(false, None, "type a=readonly [string,number];");
}

#[test]
fn type_function() {
    assert_statement_string(false, None, "type a=()=>c;");
    assert_statement_string(false, None, "type a=(a:b)=>c;");
    assert_statement_string(false, None, "type a=(a:b,c:d)=>c;");
    assert_statement_string(false, None, "type a=(a:b,c?:d)=>c;");
    assert_statement_string(false, None, "type a=(a,b)=>c;");
    assert_statement_string(false, None, "type a=<a>()=>c;");
    assert_statement_string(false, None, "type a=(...a)=>c;");
    assert_statement_string(true, None, "type a = () => c;");
    assert_statement_string(true, None, "type a = (a) => c;");
    assert_statement_string(true, None, "type a = (a: b) => c;");
    assert_statement_string(true, None, "type a = (a?: b) => c;");
    assert_statement_string(true, None, "type a = (a?: b, c) => c;");
    assert_statement_string(true, None, "type a = <a>(a?: b, c) => c;");
    assert_statement_string(
        true,
        None,
        &format!("type a = <a>(\n  a?: b,\n  {}\n) => c;", "c".repeat(80)),
    );
    let a30 = "a".repeat(30);
    let b30 = "b".repeat(30);
    assert_expression_string(
        false,
        None,
        None,
        &format!("({}:{},...{}:{}):c=>{{}}", a30, a30, b30, b30),
    );
}

#[test]
fn type_object() {
    assert_statement_string(false, None, "type a={};");
    assert_statement_string(false, None, "type a={...};");
    assert_statement_string(false, None, "type a={||};");
    assert_statement_string(false, None, "type a={a:b};");
    assert_statement_string(false, None, "type a={|a:b|};");
    assert_statement_string(false, None, "type a={a:b,...};");
    assert_statement_string(false, None, "type a={+a:b};");
    assert_statement_string(false, None, "type a={a?:b};");
    assert_statement_string(false, None, "type a={a:?b};");
    assert_statement_string(false, None, "type a={a?:?b};");
    assert_statement_string(false, None, "type a={\"a\":b};");
    assert_statement_string(false, None, "type a={a:b};");
    assert_statement_string(false, None, "type a={a:b,c:d};");
    assert_statement_string(false, None, "type a={...a};");
    assert_statement_string(false, None, "type a={a:b,...a};");
    assert_statement_string(true, None, "type a = { ... };");
    assert_statement_string(true, None, "type a = { a: b };");
    assert_statement_string(true, None, "type a = { a: b, ... };");
    assert_statement_string(true, Some(&no_bracket_spacing(&opts())), "type a = {a: b};");
    assert_statement_string(true, None, "type a = { a: b, c: d };");
    assert_statement_string(
        true,
        Some(&no_bracket_spacing(&opts())),
        "type a = {a: b, c: d};",
    );
    assert_statement_string(
        true,
        None,
        &format!("type a = {{\n  a: b,\n  c: {},\n}};", "d".repeat(80)),
    );
    assert_statement_string(false, None, "type a={a():b};");
    assert_statement_string(false, None, "type a={get a():b};");
    assert_statement_string(false, None, "type a={set a():b};");
    assert_statement_string(true, None, "type a = { set a(): b };");
    assert_statement_string(false, None, "type a={a?:()=>a};");
    assert_statement_string(false, None, "type a={a?():b};");
    assert_statement_string(true, None, "type a = { a?(): b };");
    assert_statement_string(false, None, "type a={a?(x:string):b};");
    assert_statement_string(true, None, "type a = { a?(x: string): b };");
    assert_statement_string(false, None, "type a={+a:()=>a};");
    assert_statement_string(false, None, "type a={():a};");
    assert_statement_string(false, None, "type a={[b]:a};");
    assert_statement_string(false, None, "type a={[a:b]:a};");
    assert_statement_string(false, None, "type a={+[a:b]:a};");
    assert_statement_string(true, None, "type a = { +[a: b]: a };");
    assert_statement_string(false, None, "type a={a:b,+[a:b]:a,():a,c():b};");
    // TODO: the RHS should be indented
    assert_statement_string(true, None, "type T =\n/* foo */\n{ ... };");
    // TODO: the RHS should be indented
    assert_statement_string(true, None, "type T =\n/* foo */\n{\n  // bar\n  ...\n};");
    assert_statement_string(true, None, "type T = {\n  // foo\n  ...\n};");
    assert_statement_string(true, None, "type T = {\n  /* foo */\n  ...\n};");
}

#[test]
fn type_tuple() {
    assert_statement_string(false, None, "type T=[];");
    assert_statement_string(true, None, "type T = [];");
    assert_statement_string(false, None, "type T=[string,number];");
    assert_statement_string(true, None, "type T = [string, number];");
    assert_statement_string(false, None, "type T=[foo:string,bar:number];");
    assert_statement_string(true, None, "type T = [foo: string, bar: number];");
    assert_statement_string(false, None, "type T=[+foo:string,-bar:number];");
    assert_statement_string(true, None, "type T = [+foo: string, -bar: number];");
    assert_statement_string(
        false,
        None,
        "type T=[foo?:string,+bar?:number,-baz?:boolean];",
    );
    assert_statement_string(
        true,
        None,
        "type T = [foo?: string, +bar?: number, -baz?: boolean];",
    );
    assert_statement_string(false, None, "type T=[...S];");
    assert_statement_string(true, None, "type T = [...S];");
    assert_statement_string(false, None, "type T=[...bar:S];");
    assert_statement_string(true, None, "type T = [...bar: S];");
    assert_statement_string(false, None, "type T=[...];");
    assert_statement_string(true, None, "type T = [...];");
    assert_statement_string(false, None, "type T=[S,...];");
    assert_statement_string(true, None, "type T = [S, ...];");
    assert_statement_string(false, None, "type T=[string?];");
    assert_statement_string(true, None, "type T = [string?];");
    assert_statement_string(false, None, "type T=[string,number?];");
    assert_statement_string(true, None, "type T = [string, number?];");
    assert_statement_string(false, None, "type T=[string?,number?];");
    assert_statement_string(true, None, "type T = [string?, number?];");
}

#[test]
fn type_union_or_intersection() {
    assert_statement_string(false, None, "type a=a|b;");
    assert_statement_string(false, None, "type a=a|b|c;");
    assert_statement_string(false, None, "type a=?(a|b);");
    assert_statement_string(false, None, "type a=a&b;");
    assert_statement_string(false, None, "type a=a&b&c;");
    assert_statement_string(false, None, "type a=?(a&b);");
    assert_statement_string(false, None, "type a=a|(b&c)|d;");
    assert_statement_string(false, None, "type a=(a|b)&c;");
    assert_statement_string(false, None, "type a=(a&b)|c;");
    assert_statement_string(false, None, "type a=a|(b|c);");
    assert_statement_string(false, None, "type a=(a&b)|c;");
    assert_statement_string(false, None, "type a=a|(()=>b)|c;");
    assert_statement_string(true, None, "type a = a | b;");
    assert_statement_string(true, None, "type a = a | b | c;");
    assert_statement_string(true, None, "type a = a & b & c;");
    assert_statement_string(
        true,
        None,
        &format!("type a = \n  | a\n  | b\n  | {};", "c".repeat(80)),
    );
}
