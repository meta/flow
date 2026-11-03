/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 *
 * @flow strict
 * @format
 */

'use strict';

import type {
  ArrowFunctionExpression,
  AsExpression,
  AssignmentExpression,
  AssignmentPattern,
  BlockStatement,
  BreakStatement,
  CallExpression,
  CatchClause,
  ComponentDeclaration,
  DeclareComponent,
  ClassDeclaration,
  ClassExpression,
  ContinueStatement,
  DeclareClass,
  DeclareExportAllDeclaration,
  DeclareExportDeclaration,
  DeclareFunction,
  DeclareHook,
  DeclareInterface,
  DeclareModule,
  DeclareNamespace,
  DeclareOpaqueType,
  DeclareTypeAlias,
  DeclareVariable,
  DeclareEnum,
  EnumDeclaration,
  ESNode,
  ExportAllDeclaration,
  ExportDefaultDeclaration,
  ExportNamedDeclaration,
  ForInStatement,
  ForOfStatement,
  ForStatement,
  FunctionDeclaration,
  FunctionExpression,
  HookDeclaration,
  Identifier,
  ImportAttribute,
  ImportDeclaration,
  InterfaceDeclaration,
  JSXAttribute,
  JSXClosingElement,
  JSXFragment,
  JSXIdentifier,
  JSXMemberExpression,
  JSXNamespacedName,
  JSXOpeningElement,
  JSXTagNameExpression,
  LabeledStatement,
  MatchAsPattern,
  MatchBindingPattern,
  MatchExpressionCase,
  MatchMemberPattern,
  MatchObjectPatternProperty,
  MatchStatementCase,
  MemberExpression,
  MetaProperty,
  NewExpression,
  OpaqueType,
  PrivateIdentifier,
  Program,
  Property,
  RecordDeclaration,
  RecordExpression,
  SatisfiesExpression,
  SwitchStatement,
  TaggedTemplateExpression,
  TypeAlias,
  TypeCastExpression,
  UpdateExpression,
  VariableDeclaration,
  WithStatement,
  DeclareModuleExports,
} from 'flow-estree';
import type {ReferenceImplicitGlobal} from './Reference';
import type {VisitorOptions} from './Visitor';
import type {Scope} from '../scope';

import {ClassVisitor} from './ClassVisitor';
import {ExportVisitor} from './ExportVisitor';
import {ImportVisitor} from './ImportVisitor';
import {PatternVisitor} from './PatternVisitor';
import {ReferenceFlag} from './Reference';
import {ScopeManager} from '../ScopeManager';
import {TypeVisitor} from './TypeVisitor';
import {Visitor} from './Visitor';
import {
  CatchClauseDefinition,
  ComponentNameDefinition,
  EnumDefinition,
  FunctionNameDefinition,
  HookNameDefinition,
  ParameterDefinition,
  RecordNameDefinition,
  VariableDefinition,
} from '../definition';

function getJsxName(name: JSXTagNameExpression): string {
  switch (name.type) {
    case 'JSXIdentifier':
      return name.name;

    case 'JSXNamespacedName':
      return getJsxName(name.namespace);

    case 'JSXMemberExpression':
      return getJsxName(name.object);

    default:
      throw new Error(`Unexpected JSX tag name ${name.type}`);
  }
}

const FBT_NAMES = new Set(['fbt', 'fbs']);

type ReferencerOptions = Readonly<{
  ...VisitorOptions,
  fbtSupport: boolean | null,
  jsxPragma: string | null,
  jsxFragmentName: string | null,
}>;

// Referencing variables and creating bindings.
class Referencer extends Visitor {
  readonly _jsxPragma: string | null;
  readonly _jsxFragmentName: string | null;
  readonly _fbtSupport: boolean | null;
  readonly scopeManager: ScopeManager;

  constructor(
    {
      childVisitorKeys,
      fbtSupport,
      jsxFragmentName,
      jsxPragma,
    }: ReferencerOptions,
    scopeManager: ScopeManager,
  ) {
    super({childVisitorKeys});
    this.scopeManager = scopeManager;
    this._jsxPragma = jsxPragma;
    this._jsxFragmentName = jsxFragmentName;
    this._fbtSupport = fbtSupport;
  }

  currentScope: {
    (): Scope,
    (throwOnNull: true): Scope | null,
    // $FlowFixMe[incompatible-exact]
  } = (dontThrowOnNull?: boolean) => {
    if (dontThrowOnNull !== true) {
      if (this.scopeManager.currentScope == null) {
        throw new Error('Expected there to be a current scope.');
      }
    }
    // $FlowExpectedError[incompatible-type]
    return this.scopeManager.currentScope;
  };

  close(node: ESNode): void {
    while (this.currentScope(true) && node === this.currentScope().block) {
      this.scopeManager.currentScope = this.currentScope().close(
        this.scopeManager,
      );
    }
  }

  referencingDefaultValue(
    pattern: Identifier,
    assignments: Array<AssignmentExpression | AssignmentPattern>,
    maybeImplicitGlobal: ReferenceImplicitGlobal | null,
    init: boolean,
  ): void {
    assignments.forEach(assignment => {
      this.currentScope().referenceValue(
        pattern,
        ReferenceFlag.Write,
        assignment.right,
        maybeImplicitGlobal,
        init,
      );
    });
  }

  _referenceJsxPragma(): void {
    const jsxPragma = this._jsxPragma;
    if (jsxPragma == null) {
      return;
    }
    this.currentScope().indirectlyReferenceValue(jsxPragma);
  }

  _referenceJsxFragment(): void {
    const jsxFragmentName = this._jsxFragmentName;
    if (jsxFragmentName == null) {
      return;
    }
    this.currentScope().indirectlyReferenceValue(jsxFragmentName);
  }

  ///////////////////
  // Visit helpers //
  ///////////////////

  visitClass(node: ClassDeclaration | ClassExpression): void {
    ClassVisitor.visit(this, node);
  }

  visitExpressionTarget(node: ESNode): ESNode {
    let target = node;
    while (target.type === 'NonNullExpression') {
      target = target.expression;
    }
    return target;
  }

  visitForIn(node: ForInStatement | ForOfStatement): void {
    if (node.left.type === 'VariableDeclaration' && node.left.kind !== 'var') {
      this.scopeManager.nestForScope(node);
    }

    if (node.left.type === 'VariableDeclaration') {
      const left = node.left;
      this.visit(left);
      this.visitPattern(
        left.declarations[0].id,
        pattern => {
          this.currentScope().referenceValue(
            pattern,
            ReferenceFlag.Write,
            node.right,
            null,
            true,
          );
        },
        typeAnnotation => {
          this.visitType(typeAnnotation);
        },
      );
    } else {
      this.visitPattern(
        node.left,
        (pattern, info) => {
          const maybeImplicitGlobal = !this.currentScope().isStrict
            ? {
                pattern,
                node,
              }
            : null;
          this.referencingDefaultValue(
            pattern,
            info.assignments,
            maybeImplicitGlobal,
            false,
          );
          this.currentScope().referenceValue(
            pattern,
            ReferenceFlag.Write,
            node.right,
            maybeImplicitGlobal,
            false,
          );
        },
        typeAnnotation => {
          this.visitType(typeAnnotation);
        },
        {processRightHandNodes: true},
      );
    }
    this.visit(node.right);
    this.visit(node.body);

    this.close(node);
  }

  visitFunction(
    node: ArrowFunctionExpression | FunctionDeclaration | FunctionExpression,
  ): void {
    // FunctionDeclaration name is defined in upper scope
    // NOTE: Not referring variableScope. It is intended.
    // Since
    //  in ES5, FunctionDeclaration should be in FunctionBody.
    //  in ES6, FunctionDeclaration should be block scoped.

    switch (node.type) {
      case 'FunctionExpression': {
        if (node.id) {
          // FunctionExpression with name creates its special scope;
          // FunctionExpressionNameScope.
          this.scopeManager.nestFunctionExpressionNameScope(node);
        }
        break;
      }

      case 'FunctionDeclaration': {
        if (node.id != null) {
          const id = node.id;
          // id is defined in upper scope
          this.currentScope().defineIdentifier(
            id,
            new FunctionNameDefinition(id, node),
          );
        }
        break;
      }

      case 'ArrowFunctionExpression': {
        break;
      }
    }

    this.scopeManager.nestFunctionScope(node, false);

    // function type parameters can be referenced by function params, so have to be declared first
    this.visitType(node.typeParameters);
    // Return type may reference type parameters but not function parameters, so visit it before the parameters
    this.visitType(node.returnType);

    // Process parameter declarations.
    for (const param of node.params) {
      // TypeScript `constructor(private x: T = v)`: an ordinary parameter that
      // also declares a class property. Not yet part of the flow-estree types.
      // $FlowFixMe[unclear-type]
      const parameterProperty: any = param;
      if (parameterProperty.type === 'ParameterProperty') {
        this.currentScope().defineIdentifier(
          parameterProperty.key,
          new ParameterDefinition(parameterProperty.key, node, false),
        );
        this.visitType(parameterProperty.typeAnnotation);
        this.visit(parameterProperty.value);
        for (const decorator of parameterProperty.decorators ?? []) {
          this.visit(decorator);
        }
        continue;
      }
      if (param.type === 'Identifier' && param.name === 'this') {
        // `this` parameters don't declare variables, nor can they have default values
        // but will have an annotation
        this.visitType(param.typeAnnotation);
        continue;
      }
      this.visitPattern(
        param,
        (pattern, info) => {
          this.currentScope().defineIdentifier(
            pattern,
            new ParameterDefinition(pattern, node, info.rest),
          );

          this.referencingDefaultValue(pattern, info.assignments, null, true);
        },
        typeAnnotation => {
          this.visitType(typeAnnotation);
        },
        {processRightHandNodes: true},
      );
    }

    // In TypeScript there are a number of function-like constructs which have no body,
    // so check it exists before traversing
    if (node.body) {
      // Skip BlockStatement to prevent creating BlockStatement scope.
      if (node.body.type === 'BlockStatement') {
        this.visitChildren(node.body);
      } else {
        this.visit(node.body);
      }
    }

    this.close(node);
  }

  visitType: (?ESNode) => void = (node): void => {
    if (!node) {
      return;
    }
    TypeVisitor.visit(this, node);
  };

  visitJSXTag(node: JSXOpeningElement | JSXClosingElement): void {
    const rootName = getJsxName(node.name);
    if (this._fbtSupport !== true || !FBT_NAMES.has(rootName)) {
      // <fbt /> does not reference the jsxPragma, but instead references the fbt import
      this._referenceJsxPragma();
    }

    switch (node.name.type) {
      case 'JSXIdentifier':
        if (
          rootName[0].toUpperCase() === rootName[0] ||
          (this._fbtSupport === true && FBT_NAMES.has(rootName))
        ) {
          // lower cased component names are always treated as "intrinsic" names, and are converted to a string,
          // not a variable by JSX transforms:
          // <div /> => React.createElement("div", null)
          this.visit(node.name);
        }
        break;

      case 'JSXMemberExpression':
      case 'JSXNamespacedName':
        // special case for <this.Foo /> - we don't want to create an unclosed
        // and impossible-to-resolve reference to a variable called `this`.
        if (rootName !== 'this') {
          this.visit(node.name);
        }
        break;
    }
  }

  /////////////////////
  // Visit selectors //
  /////////////////////

  ArrowFunctionExpression(node: ArrowFunctionExpression): void {
    this.visitFunction(node);
  }

  AsExpression(node: AsExpression): void {
    this.visit(node.expression);
    this.visitType(node.typeAnnotation);
  }

  SatisfiesExpression(node: SatisfiesExpression): void {
    this.visit(node.expression);
    this.visitType(node.typeAnnotation);
  }

  AssignmentExpression(node: AssignmentExpression): void {
    const left = this.visitExpressionTarget(node.left);
    if (PatternVisitor.isPattern(left)) {
      if (node.operator === '=') {
        this.visitPattern(
          left,
          (pattern, info) => {
            const maybeImplicitGlobal = !this.currentScope().isStrict
              ? {
                  pattern,
                  node,
                }
              : null;
            this.referencingDefaultValue(
              pattern,
              info.assignments,
              maybeImplicitGlobal,
              false,
            );
            this.currentScope().referenceValue(
              pattern,
              ReferenceFlag.Write,
              node.right,
              maybeImplicitGlobal,
              false,
            );
          },
          () => {},
          {processRightHandNodes: true},
        );
      } else if (left.type === 'Identifier') {
        this.currentScope().referenceValue(
          left,
          ReferenceFlag.ReadWrite,
          node.right,
        );
      }
    } else {
      this.visit(left);
    }
    this.visit(node.right);
  }

  BlockStatement(node: BlockStatement): void {
    if (this.scopeManager.isES6()) {
      this.scopeManager.nestBlockScope(node);
    }

    this.visitChildren(node);

    this.close(node);
  }

  BreakStatement(_: BreakStatement): void {
    // don't reference the break statement's label
  }

  CallExpression(node: CallExpression): void {
    this.visitChildren(node, ['typeArguments']);
    this.visitType(node.typeArguments);
  }

  CatchClause(node: CatchClause): void {
    this.scopeManager.nestCatchScope(node);

    if (node.param) {
      const param = node.param;
      this.visitPattern(
        param,
        (pattern, info) => {
          this.currentScope().defineIdentifier(
            pattern,
            new CatchClauseDefinition(param, node),
          );
          this.referencingDefaultValue(pattern, info.assignments, null, true);
        },
        typeAnnotation => {
          this.visitType(typeAnnotation);
        },
        {processRightHandNodes: true},
      );
    }
    this.visit(node.body);

    this.close(node);
  }

  ClassExpression(node: ClassExpression): void {
    this.visitClass(node);
  }

  ClassDeclaration(node: ClassDeclaration): void {
    this.visitClass(node);
  }

  ContinueStatement(_: ContinueStatement): void {
    // don't reference the continue statement's label
  }

  EnumDeclaration(node: EnumDeclaration): void {
    this.currentScope().defineIdentifier(
      node.id,
      new EnumDefinition(node.id, node),
    );

    // Enum body cannot contain identifier references, so no need to visit body.
  }

  ExportAllDeclaration(_: ExportAllDeclaration): void {
    // this defines no local variables
  }

  ExportDefaultDeclaration(node: ExportDefaultDeclaration): void {
    if (node.declaration.type === 'Identifier') {
      ExportVisitor.visit(this, node);
    } else {
      this.visit(node.declaration);
    }
  }

  ExportNamedDeclaration(node: ExportNamedDeclaration): void {
    if (node.declaration) {
      this.visit(node.declaration);
    } else {
      ExportVisitor.visit(this, node);
    }
  }

  ForInStatement(node: ForInStatement): void {
    this.visitForIn(node);
  }

  ForOfStatement(node: ForOfStatement): void {
    this.visitForIn(node);
  }

  ForStatement(node: ForStatement): void {
    // Create ForStatement declaration.
    // NOTE: In ES6, ForStatement dynamically generates per iteration environment. However, this is
    // a static analyzer, we only generate one scope for ForStatement.
    if (
      node.init &&
      node.init.type === 'VariableDeclaration' &&
      node.init.kind !== 'var'
    ) {
      this.scopeManager.nestForScope(node);
    }

    this.visitChildren(node);

    this.close(node);
  }

  ComponentDeclaration(node: ComponentDeclaration): void {
    const id = node.id;
    // id is defined in upper scope
    this.currentScope().defineIdentifier(
      id,
      new ComponentNameDefinition(id, node),
    );

    this.scopeManager.nestComponentScope(node);

    // component type parameters can be referenced by component params, so have to be declared first
    this.visitType(node.typeParameters);
    // Renders type may reference type parameters but not component parameters, so visit it before the parameters
    this.visitType(node.rendersType);

    // Process parameter declarations.
    for (const param of node.params) {
      const paramPattern = (() => {
        switch (param.type) {
          case 'ComponentParameter':
            return param.local;
          case 'RestElement':
            return param;
        }
      })();
      this.visitPattern(
        paramPattern,
        (pattern, info) => {
          this.currentScope().defineIdentifier(
            pattern,
            new ParameterDefinition(pattern, node, info.rest),
          );

          this.referencingDefaultValue(pattern, info.assignments, null, true);
        },
        typeAnnotation => {
          this.visitType(typeAnnotation);
        },
        {processRightHandNodes: true},
      );
    }

    this.visitChildren(node.body);

    this.close(node);
  }

  HookDeclaration(node: HookDeclaration): void {
    const id = node.id;
    // id is defined in upper scope
    this.currentScope().defineIdentifier(id, new HookNameDefinition(id, node));

    this.scopeManager.nestHookScope(node);

    // hook type parameters can be referenced by hook params, so have to be declared first
    this.visitType(node.typeParameters);
    // Return type may reference type parameters but not hook parameters, so visit it before the parameters
    this.visitType(node.returnType);

    // Process parameter declarations.
    for (const param of node.params) {
      this.visitPattern(
        param,
        (pattern, info) => {
          this.currentScope().defineIdentifier(
            pattern,
            new ParameterDefinition(pattern, node, info.rest),
          );

          this.referencingDefaultValue(pattern, info.assignments, null, true);
        },
        typeAnnotation => {
          this.visitType(typeAnnotation);
        },
        {processRightHandNodes: true},
      );
    }

    this.visitChildren(node.body);

    this.close(node);
  }

  FunctionDeclaration(node: FunctionDeclaration): void {
    this.visitFunction(node);
  }

  FunctionExpression(node: FunctionExpression): void {
    this.visitFunction(node);
  }

  Identifier(node: Identifier): void {
    this.currentScope().referenceValue(node);
    this.visitType(node.typeAnnotation);
  }

  ImportAttribute(_: ImportAttribute): void {
    // import assertions are module metadata and thus have no variables to reference
  }

  ImportDeclaration(node: ImportDeclaration): void {
    if (!this.scopeManager.isES6() || !this.scopeManager.isModule()) {
      throw new Error(
        'ImportDeclaration should appear when the mode is ES6 and in the module context.',
      );
    }

    ImportVisitor.visit(this, node);
  }

  JSXAttribute(node: JSXAttribute): void {
    this.visit(node.value);
  }

  JSXClosingElement(node: JSXClosingElement): void {
    /**
     * Note that this was not previously considered to be a reference and that
     * other scope analyzers do not count them either: e.g. TypeScript-eslint
     * https://fburl.com/4q93a3x3
     *
     * We are considering this a reference because it technically includes an
     * identifier that refers to a defined variable. So, if you want to answer:
     * "what are all of the references to this variable?", the closing element
     * should be included.
     */

    this.visitJSXTag(node);
  }

  JSXFragment(node: JSXFragment): void {
    this._referenceJsxPragma();
    this._referenceJsxFragment();
    this.visitChildren(node);
  }

  JSXIdentifier(node: JSXIdentifier): void {
    this.currentScope().referenceValue(node);
  }

  JSXMemberExpression(node: JSXMemberExpression): void {
    this.visit(node.object);
    // we don't ever reference the property as it's always going to be a property on the thing
  }

  JSXNamespacedName(node: JSXNamespacedName): void {
    this.visit(node.namespace);
    // namespace:name
    // the "name" doesn't reference a variable so it should be ignored, like a member expression
  }

  JSXOpeningElement(node: JSXOpeningElement): void {
    this.visitJSXTag(node);

    // the opening tag may also have type args and attributes
    this.visitType(node.typeArguments);

    for (const attr of node.attributes) {
      this.visit(attr);
    }
  }

  LabeledStatement(node: LabeledStatement): void {
    this.visit(node.body);
  }

  MemberExpression(node: MemberExpression): void {
    this.visit(node.object);
    if (node.computed === true) {
      this.visit(node.property);
    }
  }

  MetaProperty(_: MetaProperty): void {
    // meta properties all builtin globals
  }

  NewExpression(node: NewExpression): void {
    this.visitChildren(node, ['typeArguments']);
    this.visitType(node.typeArguments);
  }

  PrivateIdentifier(_: PrivateIdentifier): void {
    // private names can only reference class properties
  }

  Program(node: Program): void {
    this.scopeManager.nestGlobalScope(node);

    if (this.scopeManager.isGlobalReturn()) {
      // Force strictness of GlobalScope to false when using node.js scope.
      this.currentScope().isStrict = false;
      this.scopeManager.nestFunctionScope(node, false);
    }

    if (this.scopeManager.isES6() && this.scopeManager.isModule()) {
      this.scopeManager.nestModuleScope(node);
    }

    if (
      this.scopeManager.isStrictModeSupported() &&
      this.scopeManager.isImpliedStrict()
    ) {
      this.currentScope().isStrict = true;
    }

    this.visitChildren(node);
    this.close(node);
  }

  Property(node: Property): void {
    if (node.computed) {
      this.visit(node.key);
    }

    this.visit(node.value);
  }

  RecordDeclaration(node: RecordDeclaration): void {
    const {id} = node;
    this.currentScope().defineIdentifier(
      id,
      new RecordNameDefinition(id, node),
    );

    this.scopeManager.nestRecordScope(node);

    // visit the type param declarations
    this.visitType(node.typeParameters);
    // then the usages
    node.implements?.forEach(imp => this.visitType(imp));

    for (const element of node.body.elements) {
      switch (element.type) {
        case 'RecordDeclarationProperty': {
          // Skip visiting key, it is not a reference
          this.visitType(element.typeAnnotation);
          if (element.defaultValue) {
            this.visit(element.defaultValue);
          }
          break;
        }
        case 'RecordDeclarationStaticProperty': {
          // Skip visiting key, it is not a reference
          this.visitType(element.typeAnnotation);
          this.visit(element.value);
          break;
        }
        case 'MethodDefinition': {
          this.visitFunction(element.value);
          break;
        }
      }
    }

    this.close(node);
  }

  RecordExpression(node: RecordExpression): void {
    this.visit(node.recordConstructor);
    this.visitType(node.typeArguments);
    this.visit(node.properties);
  }

  SwitchStatement(node: SwitchStatement): void {
    this.visit(node.discriminant);

    if (this.scopeManager.isES6()) {
      this.scopeManager.nestSwitchScope(node);
    }

    for (const switchCase of node.cases) {
      this.visit(switchCase);
    }

    this.close(node);
  }

  TaggedTemplateExpression(node: TaggedTemplateExpression): void {
    this.visit(node.tag);
    this.visitType(node.typeArguments);
    this.visit(node.quasi);
  }

  UpdateExpression(node: UpdateExpression): void {
    const argument = this.visitExpressionTarget(node.argument);
    if (PatternVisitor.isPattern(argument)) {
      this.visitPattern(
        argument,
        pattern => {
          this.currentScope().referenceValue(
            pattern,
            ReferenceFlag.ReadWrite,
            null,
          );
        },
        () => {},
      );
    } else {
      this.visitChildren(node);
    }
  }

  VariableDeclaration(node: VariableDeclaration): void {
    const variableTargetScope =
      node.kind === 'var'
        ? this.currentScope().variableScope
        : this.currentScope();

    for (const decl of node.declarations) {
      const init = decl.init;

      this.visitPattern(
        decl.id,
        (pattern, info) => {
          variableTargetScope.defineIdentifier(
            pattern,
            new VariableDefinition(pattern, decl, node),
          );

          this.referencingDefaultValue(pattern, info.assignments, null, true);
          if (init) {
            this.currentScope().referenceValue(
              pattern,
              ReferenceFlag.Write,
              init,
              null,
              true,
            );
          }
        },
        typeAnnotation => {
          this.visitType(typeAnnotation);
        },
        {processRightHandNodes: true},
      );

      if (decl.init) {
        this.visit(decl.init);
      }
    }
  }

  WithStatement(node: WithStatement): void {
    this.visit(node.object);

    // Then nest scope for WithStatement.
    this.scopeManager.nestWithScope(node);

    this.visit(node.body);

    this.close(node);
  }

  //
  // Type node passthrough visitors
  //

  DeclareClass(node: DeclareClass): void {
    this.visitType(node);
  }

  DeclareVariable(node: DeclareVariable): void {
    this.visitType(node);
  }

  DeclareEnum(node: DeclareEnum): void {
    this.currentScope().defineIdentifier(
      node.id,
      new EnumDefinition(node.id, node),
    );

    // Enum body cannot contain identifier references, so no need to visit body.
  }

  DeclareComponent(node: DeclareComponent): void {
    this.visitType(node);
  }

  DeclareFunction(node: DeclareFunction): void {
    this.visitType(node);
  }

  DeclareHook(node: DeclareHook): void {
    this.visitType(node);
  }

  DeclareModule(node: DeclareModule): void {
    this.visitType(node);
  }

  DeclareModuleExports(node: DeclareModuleExports): void {
    this.visitType(node);
  }

  DeclareNamespace(node: DeclareNamespace): void {
    this.visitType(node);
  }

  DeclareInterface(node: DeclareInterface): void {
    this.visitType(node);
  }

  DeclareTypeAlias(node: DeclareTypeAlias): void {
    this.visitType(node);
  }

  DeclareOpaqueType(node: DeclareOpaqueType): void {
    this.visitType(node);
  }

  DeclareExportAllDeclaration(node: DeclareExportAllDeclaration): void {
    this.visitType(node);
  }

  DeclareExportDeclaration(node: DeclareExportDeclaration): void {
    this.visitType(node);
  }

  InterfaceDeclaration(node: InterfaceDeclaration): void {
    this.visitType(node);
  }

  OpaqueType(node: OpaqueType): void {
    this.visitType(node);
  }

  TypeAlias(node: TypeAlias): void {
    this.visitType(node);
  }

  TypeCastExpression(node: TypeCastExpression): void {
    this.visit(node.expression);
    this.visitType(node.typeAnnotation);
  }

  //
  // Match
  //

  MatchExpressionCase(node: MatchExpressionCase): void {
    this.scopeManager.nestMatchCaseScope(node);
    this.visitChildren(node);
    this.close(node);
  }

  MatchStatementCase(node: MatchStatementCase): void {
    this.scopeManager.nestMatchCaseScope(node);
    this.visitChildren(node);
    this.close(node);
  }

  MatchBindingPattern(node: MatchBindingPattern): void {
    const {id, kind} = node;
    const variableTargetScope =
      kind === 'var' ? this.currentScope().variableScope : this.currentScope();

    variableTargetScope.defineIdentifier(
      id,
      new VariableDefinition(id, node, node),
    );
  }

  MatchMemberPattern(node: MatchMemberPattern): void {
    this.visit(node.base);
    // Skip `node.property`
  }

  MatchObjectPatternProperty(node: MatchObjectPatternProperty): void {
    this.visit(node.pattern);
    // Skip `node.key`
  }

  MatchAsPattern(node: MatchAsPattern): void {
    const {pattern, target} = node;
    this.visit(pattern);
    switch (target.type) {
      case 'Identifier': {
        this.currentScope().defineIdentifier(
          target,
          new VariableDefinition(target, node, node),
        );
        break;
      }
      case 'MatchBindingPattern': {
        this.visit(target);
        break;
      }
    }
  }
}

export type {ReferencerOptions};
export {Referencer};
