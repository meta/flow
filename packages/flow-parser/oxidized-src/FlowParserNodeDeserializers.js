/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 *
 * @noflow
 * @format
 * @generated
 */

/*
 * !!! GENERATED FILE !!!
 *
 * Any manual changes to this file will be overwritten.
 * To regenerate: buck run fbcode//flow/rust_port/crates/flow_parser_wasm:codegen
 */

// lint directives to let us do some basic validation of generated files
/* eslint no-undef: 'error', no-unused-vars: ['error', {vars: "local"}], no-redeclare: 'error' */

'use strict';

module.exports = [
  // 0: EmptyStatement
  function () {
    return {
      type: 'EmptyStatement',
      loc: this.addEmptyLoc(),
    };
  },

  // 1: ExpressionStatement
  function () {
    return {
      type: 'ExpressionStatement',
      loc: this.addEmptyLoc(),
      expression: this.deserializeNode(),
      directive: this.deserializeString(),
    };
  },

  // 2: BlockStatement
  function () {
    return {
      type: 'BlockStatement',
      loc: this.addEmptyLoc(),
      body: this.deserializeNodeList(),
    };
  },

  // 3: IfStatement
  function () {
    return {
      type: 'IfStatement',
      loc: this.addEmptyLoc(),
      test: this.deserializeNode(),
      consequent: this.deserializeNode(),
      alternate: this.deserializeNode(),
    };
  },

  // 4: LabeledStatement
  function () {
    return {
      type: 'LabeledStatement',
      loc: this.addEmptyLoc(),
      label: this.deserializeNode(),
      body: this.deserializeNode(),
    };
  },

  // 5: BreakStatement
  function () {
    return {
      type: 'BreakStatement',
      loc: this.addEmptyLoc(),
      label: this.deserializeNode(),
    };
  },

  // 6: ContinueStatement
  function () {
    return {
      type: 'ContinueStatement',
      loc: this.addEmptyLoc(),
      label: this.deserializeNode(),
    };
  },

  // 7: WithStatement
  function () {
    return {
      type: 'WithStatement',
      loc: this.addEmptyLoc(),
      object: this.deserializeNode(),
      body: this.deserializeNode(),
    };
  },

  // 8: SwitchStatement
  function () {
    return {
      type: 'SwitchStatement',
      loc: this.addEmptyLoc(),
      discriminant: this.deserializeNode(),
      cases: this.deserializeNodeList(),
    };
  },

  // 9: ReturnStatement
  function () {
    return {
      type: 'ReturnStatement',
      loc: this.addEmptyLoc(),
      argument: this.deserializeNode(),
    };
  },

  // 10: ThrowStatement
  function () {
    return {
      type: 'ThrowStatement',
      loc: this.addEmptyLoc(),
      argument: this.deserializeNode(),
    };
  },

  // 11: TryStatement
  function () {
    return {
      type: 'TryStatement',
      loc: this.addEmptyLoc(),
      block: this.deserializeNode(),
      handler: this.deserializeNode(),
      finalizer: this.deserializeNode(),
    };
  },

  // 12: WhileStatement
  function () {
    return {
      type: 'WhileStatement',
      loc: this.addEmptyLoc(),
      body: this.deserializeNode(),
      test: this.deserializeNode(),
    };
  },

  // 13: DoWhileStatement
  function () {
    return {
      type: 'DoWhileStatement',
      loc: this.addEmptyLoc(),
      body: this.deserializeNode(),
      test: this.deserializeNode(),
    };
  },

  // 14: ForStatement
  function () {
    return {
      type: 'ForStatement',
      loc: this.addEmptyLoc(),
      init: this.deserializeNode(),
      test: this.deserializeNode(),
      update: this.deserializeNode(),
      body: this.deserializeNode(),
    };
  },

  // 15: ForInStatement
  function () {
    return {
      type: 'ForInStatement',
      loc: this.addEmptyLoc(),
      left: this.deserializeNode(),
      right: this.deserializeNode(),
      body: this.deserializeNode(),
    };
  },

  // 16: ForOfStatement
  function () {
    return {
      type: 'ForOfStatement',
      loc: this.addEmptyLoc(),
      left: this.deserializeNode(),
      right: this.deserializeNode(),
      body: this.deserializeNode(),
      await: this.deserializeBoolean(),
    };
  },

  // 17: DebuggerStatement
  function () {
    return {
      type: 'DebuggerStatement',
      loc: this.addEmptyLoc(),
    };
  },

  // 18: MatchStatement
  function () {
    return {
      type: 'MatchStatement',
      loc: this.addEmptyLoc(),
      argument: this.deserializeNode(),
      cases: this.deserializeNodeList(),
    };
  },

  // 19: FunctionDeclaration
  function () {
    return {
      type: 'FunctionDeclaration',
      loc: this.addEmptyLoc(),
      id: this.deserializeNode(),
      params: this.deserializeNodeList(),
      body: this.deserializeNode(),
      typeParameters: this.deserializeNode(),
      returnType: this.deserializeNode(),
      generator: this.deserializeBoolean(),
      async: this.deserializeBoolean(),
      predicate: this.deserializeNode(),
      expression: this.deserializeBoolean(),
    };
  },

  // 20: VariableDeclaration
  function () {
    return {
      type: 'VariableDeclaration',
      loc: this.addEmptyLoc(),
      kind: this.deserializeString(),
      declarations: this.deserializeNodeList(),
    };
  },

  // 21: VariableDeclarator
  function () {
    return {
      type: 'VariableDeclarator',
      loc: this.addEmptyLoc(),
      init: this.deserializeNode(),
      id: this.deserializeNode(),
    };
  },

  // 22: ClassDeclaration
  function () {
    const node = {
      type: 'ClassDeclaration',
      loc: this.addEmptyLoc(),
    };
    node.id = this.deserializeNode();
    node.typeParameters = this.deserializeNode();
    node.superClass = this.deserializeNode();
    node.implements = this.deserializeNodeList();
    node.body = this.deserializeNode();
    node.superTypeArguments = this.deserializeNode();
    node.decorators = this.deserializeNodeList();
    if (this.deserializeBoolean()) node['abstract'] = true;
    return node;
  },

  // 23: ComponentDeclaration
  function () {
    const node = {
      type: 'ComponentDeclaration',
      loc: this.addEmptyLoc(),
    };
    node.body = this.deserializeNode();
    node.id = this.deserializeNode();
    if (this.deserializeBoolean()) node['implicitDeclare'] = true;
    node.params = this.deserializeNodeList();
    node.rendersType = this.deserializeNode();
    node.typeParameters = this.deserializeNode();
    node.async = this.deserializeBoolean();
    return node;
  },

  // 24: HookDeclaration
  function () {
    return {
      type: 'HookDeclaration',
      loc: this.addEmptyLoc(),
      id: this.deserializeNode(),
      params: this.deserializeNodeList(),
      body: this.deserializeNode(),
      returnType: this.deserializeNode(),
      typeParameters: this.deserializeNode(),
      async: this.deserializeBoolean(),
    };
  },

  // 25: EnumDeclaration
  function () {
    const node = {
      type: 'EnumDeclaration',
      loc: this.addEmptyLoc(),
    };
    node.id = this.deserializeNode();
    node.body = this.deserializeNode();
    if (this.deserializeBoolean()) node['const'] = true;
    return node;
  },

  // 26: InterfaceDeclaration
  function () {
    return {
      type: 'InterfaceDeclaration',
      loc: this.addEmptyLoc(),
      id: this.deserializeNode(),
      typeParameters: this.deserializeNode(),
      extends: this.deserializeNodeList(),
      body: this.deserializeNode(),
    };
  },

  // 27: TypeAlias
  function () {
    return {
      type: 'TypeAlias',
      loc: this.addEmptyLoc(),
      id: this.deserializeNode(),
      typeParameters: this.deserializeNode(),
      right: this.deserializeNode(),
    };
  },

  // 28: RecordDeclaration
  function () {
    return {
      type: 'RecordDeclaration',
      loc: this.addEmptyLoc(),
      id: this.deserializeNode(),
      typeParameters: this.deserializeNode(),
      implements: this.deserializeNodeList(),
      body: this.deserializeNode(),
    };
  },

  // 29: ImportDeclaration
  function () {
    return {
      type: 'ImportDeclaration',
      loc: this.addEmptyLoc(),
      specifiers: this.deserializeNodeList(),
      source: this.deserializeNode(),
      importKind: this.deserializeString(),
      attributes: this.deserializeNodeList(),
      phase: this.deserializeString(),
    };
  },

  // 30: ImportDefaultSpecifier
  function () {
    return {
      type: 'ImportDefaultSpecifier',
      loc: this.addEmptyLoc(),
      local: this.deserializeNode(),
    };
  },

  // 31: ImportNamespaceSpecifier
  function () {
    return {
      type: 'ImportNamespaceSpecifier',
      loc: this.addEmptyLoc(),
      local: this.deserializeNode(),
    };
  },

  // 32: ImportSpecifier
  function () {
    return {
      type: 'ImportSpecifier',
      loc: this.addEmptyLoc(),
      imported: this.deserializeNode(),
      local: this.deserializeNode(),
      importKind: this.deserializeString(),
    };
  },

  // 33: ImportAttribute
  function () {
    return {
      type: 'ImportAttribute',
      loc: this.addEmptyLoc(),
      key: this.deserializeNode(),
      value: this.deserializeNode(),
    };
  },

  // 34: ImportEqualsDeclaration
  function () {
    return {
      type: 'ImportEqualsDeclaration',
      loc: this.addEmptyLoc(),
      id: this.deserializeNode(),
      moduleReference: this.deserializeNode(),
      importKind: this.deserializeString(),
      isExport: this.deserializeBoolean(),
    };
  },

  // 35: ExternalModuleReference
  function () {
    return {
      type: 'ExternalModuleReference',
      loc: this.addEmptyLoc(),
      expression: this.deserializeNode(),
    };
  },

  // 36: ExportNamedDeclaration
  function () {
    const node = {
      type: 'ExportNamedDeclaration',
      loc: this.addEmptyLoc(),
    };
    node.declaration = this.deserializeNode();
    node.specifiers = this.deserializeNodeList();
    node.source = this.deserializeNode();
    node.exportKind = this.deserializeString();
    {
      const value = this.deserializeNodeList();
      if (value.length > 0) node['attributes'] = value;
    }
    return node;
  },

  // 37: ExportDefaultDeclaration
  function () {
    const node = {
      type: 'ExportDefaultDeclaration',
      loc: this.addEmptyLoc(),
    };
    node.declaration = this.deserializeNode();
    {
      const value = this.deserializeString();
      if (value != null) node['exportKind'] = value;
    }
    return node;
  },

  // 38: ExportAllDeclaration
  function () {
    const node = {
      type: 'ExportAllDeclaration',
      loc: this.addEmptyLoc(),
    };
    node.source = this.deserializeNode();
    node.exported = this.deserializeNode();
    node.exportKind = this.deserializeString();
    {
      const value = this.deserializeNodeList();
      if (value.length > 0) node['attributes'] = value;
    }
    return node;
  },

  // 39: ExportSpecifier
  function () {
    return {
      type: 'ExportSpecifier',
      loc: this.addEmptyLoc(),
      exported: this.deserializeNode(),
      local: this.deserializeNode(),
      exportKind: this.deserializeString(),
    };
  },

  // 40: ExportNamespaceSpecifier
  function () {
    return {
      type: 'ExportNamespaceSpecifier',
      loc: this.addEmptyLoc(),
      exported: this.deserializeNode(),
    };
  },

  // 41: ExportAssignment
  function () {
    return {
      type: 'ExportAssignment',
      loc: this.addEmptyLoc(),
      expression: this.deserializeNode(),
    };
  },

  // 42: ThisExpression
  function () {
    return {
      type: 'ThisExpression',
      loc: this.addEmptyLoc(),
    };
  },

  // 43: Super
  function () {
    return {
      type: 'Super',
      loc: this.addEmptyLoc(),
    };
  },

  // 44: ArrayExpression
  function () {
    return {
      type: 'ArrayExpression',
      loc: this.addEmptyLoc(),
      elements: this.deserializeNodeList(),
      trailingComma: this.deserializeBoolean(),
    };
  },

  // 45: ObjectExpression
  function () {
    return {
      type: 'ObjectExpression',
      loc: this.addEmptyLoc(),
      properties: this.deserializeNodeList(),
    };
  },

  // 46: FunctionExpression
  function () {
    return {
      type: 'FunctionExpression',
      loc: this.addEmptyLoc(),
      id: this.deserializeNode(),
      params: this.deserializeNodeList(),
      body: this.deserializeNode(),
      typeParameters: this.deserializeNode(),
      returnType: this.deserializeNode(),
      generator: this.deserializeBoolean(),
      async: this.deserializeBoolean(),
      predicate: this.deserializeNode(),
      expression: this.deserializeBoolean(),
    };
  },

  // 47: ArrowFunctionExpression
  function () {
    return {
      type: 'ArrowFunctionExpression',
      loc: this.addEmptyLoc(),
      params: this.deserializeNodeList(),
      body: this.deserializeNode(),
      typeParameters: this.deserializeNode(),
      returnType: this.deserializeNode(),
      generator: this.deserializeBoolean(),
      async: this.deserializeBoolean(),
      id: this.deserializeNode(),
      predicate: this.deserializeNode(),
      expression: this.deserializeBoolean(),
    };
  },

  // 48: SequenceExpression
  function () {
    return {
      type: 'SequenceExpression',
      loc: this.addEmptyLoc(),
      expressions: this.deserializeNodeList(),
    };
  },

  // 49: UnaryExpression
  function () {
    return {
      type: 'UnaryExpression',
      loc: this.addEmptyLoc(),
      operator: this.deserializeString(),
      argument: this.deserializeNode(),
      prefix: this.deserializeBoolean(),
    };
  },

  // 50: BinaryExpression
  function () {
    return {
      type: 'BinaryExpression',
      loc: this.addEmptyLoc(),
      left: this.deserializeNode(),
      right: this.deserializeNode(),
      operator: this.deserializeString(),
    };
  },

  // 51: LogicalExpression
  function () {
    return {
      type: 'LogicalExpression',
      loc: this.addEmptyLoc(),
      left: this.deserializeNode(),
      right: this.deserializeNode(),
      operator: this.deserializeString(),
    };
  },

  // 52: ConditionalExpression
  function () {
    return {
      type: 'ConditionalExpression',
      loc: this.addEmptyLoc(),
      test: this.deserializeNode(),
      alternate: this.deserializeNode(),
      consequent: this.deserializeNode(),
    };
  },

  // 53: UpdateExpression
  function () {
    return {
      type: 'UpdateExpression',
      loc: this.addEmptyLoc(),
      operator: this.deserializeString(),
      argument: this.deserializeNode(),
      prefix: this.deserializeBoolean(),
    };
  },

  // 54: AssignmentExpression
  function () {
    return {
      type: 'AssignmentExpression',
      loc: this.addEmptyLoc(),
      operator: this.deserializeString(),
      left: this.deserializeNode(),
      right: this.deserializeNode(),
    };
  },

  // 55: MemberExpression
  function () {
    return {
      type: 'MemberExpression',
      loc: this.addEmptyLoc(),
      object: this.deserializeNode(),
      property: this.deserializeNode(),
      computed: this.deserializeBoolean(),
      optional: this.deserializeBoolean(),
    };
  },

  // 56: OptionalMemberExpression
  function () {
    return {
      type: 'OptionalMemberExpression',
      loc: this.addEmptyLoc(),
      object: this.deserializeNode(),
      property: this.deserializeNode(),
      computed: this.deserializeBoolean(),
      optional: this.deserializeBoolean(),
    };
  },

  // 57: CallExpression
  function () {
    return {
      type: 'CallExpression',
      loc: this.addEmptyLoc(),
      callee: this.deserializeNode(),
      typeArguments: this.deserializeNode(),
      arguments: this.deserializeNodeList(),
      optional: this.deserializeBoolean(),
    };
  },

  // 58: OptionalCallExpression
  function () {
    return {
      type: 'OptionalCallExpression',
      loc: this.addEmptyLoc(),
      callee: this.deserializeNode(),
      typeArguments: this.deserializeNode(),
      arguments: this.deserializeNodeList(),
      optional: this.deserializeBoolean(),
    };
  },

  // 59: NewExpression
  function () {
    return {
      type: 'NewExpression',
      loc: this.addEmptyLoc(),
      callee: this.deserializeNode(),
      typeArguments: this.deserializeNode(),
      arguments: this.deserializeNodeList(),
    };
  },

  // 60: YieldExpression
  function () {
    return {
      type: 'YieldExpression',
      loc: this.addEmptyLoc(),
      argument: this.deserializeNode(),
      delegate: this.deserializeBoolean(),
    };
  },

  // 61: AwaitExpression
  function () {
    return {
      type: 'AwaitExpression',
      loc: this.addEmptyLoc(),
      argument: this.deserializeNode(),
    };
  },

  // 62: ImportExpression
  function () {
    return {
      type: 'ImportExpression',
      loc: this.addEmptyLoc(),
      source: this.deserializeNode(),
      options: this.deserializeNode(),
      phase: this.deserializeString(),
    };
  },

  // 63: MetaProperty
  function () {
    return {
      type: 'MetaProperty',
      loc: this.addEmptyLoc(),
      meta: this.deserializeNode(),
      property: this.deserializeNode(),
    };
  },

  // 64: TaggedTemplateExpression
  function () {
    const node = {
      type: 'TaggedTemplateExpression',
      loc: this.addEmptyLoc(),
    };
    node.tag = this.deserializeNode();
    node.quasi = this.deserializeNode();
    if (this.deserializeBoolean()) {
      node['typeArguments'] = this.deserializeNode();
    }
    return node;
  },

  // 65: TemplateLiteral
  function () {
    return {
      type: 'TemplateLiteral',
      loc: this.addEmptyLoc(),
      quasis: this.deserializeNodeList(),
      expressions: this.deserializeNodeList(),
    };
  },

  // 66: TemplateElement
  function () {
    return {
      type: 'TemplateElement',
      loc: this.addEmptyLoc(),
      tail: this.deserializeBoolean(),
      value: {
        cooked: this.deserializeString(),
        raw: this.deserializeString(),
      },
    };
  },

  // 67: TypeCastExpression
  function () {
    return {
      type: 'TypeCastExpression',
      loc: this.addEmptyLoc(),
      expression: this.deserializeNode(),
      typeAnnotation: this.deserializeNode(),
    };
  },

  // 68: AsExpression
  function () {
    return {
      type: 'AsExpression',
      loc: this.addEmptyLoc(),
      expression: this.deserializeNode(),
      typeAnnotation: this.deserializeNode(),
    };
  },

  // 69: SatisfiesExpression
  function () {
    return {
      type: 'SatisfiesExpression',
      loc: this.addEmptyLoc(),
      expression: this.deserializeNode(),
      typeAnnotation: this.deserializeNode(),
    };
  },

  // 70: AsConstExpression
  function () {
    return {
      type: 'AsConstExpression',
      loc: this.addEmptyLoc(),
      expression: this.deserializeNode(),
    };
  },

  // 71: NonNullExpression
  function () {
    return {
      type: 'NonNullExpression',
      loc: this.addEmptyLoc(),
      expression: this.deserializeNode(),
    };
  },

  // 72: MatchExpression
  function () {
    return {
      type: 'MatchExpression',
      loc: this.addEmptyLoc(),
      argument: this.deserializeNode(),
      cases: this.deserializeNodeList(),
    };
  },

  // 73: RecordExpression
  function () {
    return {
      type: 'RecordExpression',
      loc: this.addEmptyLoc(),
      recordConstructor: this.deserializeNode(),
      typeArguments: this.deserializeNode(),
      properties: this.deserializeNode(),
    };
  },

  // 74: RecordExpressionProperties
  function () {
    return {
      type: 'RecordExpressionProperties',
      loc: this.addEmptyLoc(),
      properties: this.deserializeNodeList(),
    };
  },

  // 75: Literal
  // Custom encoding: valueKind discriminant + type-specific data
  function () {
    const loc = this.addEmptyLoc();
    // 0=null, 1=boolean, 2=number, 3=string
    const valueKind = this.next();
    let value = null;
    if (valueKind === 1) value = this.deserializeBoolean();
    else if (valueKind === 2) value = this.deserializeNumber();
    else if (valueKind === 3) value = this.deserializeString();
    const literalType = this.deserializeString();
    const raw = this.deserializeString();
    const bigint = this.deserializeString();
    const regexPattern = this.deserializeString();
    const regexFlags = this.deserializeString();
    const node = {type: 'Literal', loc, value, raw, literalType};
    if (bigint != null) {
      node.bigint = bigint;
      try {
        node.value = typeof BigInt === 'function' ? BigInt(bigint) : null;
      } catch (e) {
        node.value = null;
      }
    }
    if (regexPattern != null) {
      const flags = regexFlags ?? '';
      node.regex = {pattern: regexPattern, flags};
      try {
        node.value = new RegExp(regexPattern, flags);
      } catch (e) {
        node.value = null;
      }
    }
    return node;
  },

  // 76: Identifier
  function () {
    return {
      type: 'Identifier',
      loc: this.addEmptyLoc(),
      name: this.deserializeString(),
      typeAnnotation: this.deserializeNode(),
      optional: this.deserializeBoolean(),
    };
  },

  // 77: PrivateIdentifier
  function () {
    return {
      type: 'PrivateIdentifier',
      loc: this.addEmptyLoc(),
      name: this.deserializeString(),
      typeAnnotation: this.deserializeNode(),
      optional: this.deserializeBoolean(),
    };
  },

  // 78: ObjectPattern
  function () {
    const node = {
      type: 'ObjectPattern',
      loc: this.addEmptyLoc(),
    };
    node.properties = this.deserializeNodeList();
    node.typeAnnotation = this.deserializeNode();
    if (this.deserializeBoolean()) node['optional'] = true;
    return node;
  },

  // 79: ArrayPattern
  function () {
    const node = {
      type: 'ArrayPattern',
      loc: this.addEmptyLoc(),
    };
    node.elements = this.deserializeNodeList();
    node.typeAnnotation = this.deserializeNode();
    if (this.deserializeBoolean()) node['optional'] = true;
    return node;
  },

  // 80: RestElement
  function () {
    return {
      type: 'RestElement',
      loc: this.addEmptyLoc(),
      argument: this.deserializeNode(),
    };
  },

  // 81: AssignmentPattern
  function () {
    return {
      type: 'AssignmentPattern',
      loc: this.addEmptyLoc(),
      left: this.deserializeNode(),
      right: this.deserializeNode(),
    };
  },

  // 82: Property
  function () {
    return {
      type: 'Property',
      loc: this.addEmptyLoc(),
      key: this.deserializeNode(),
      value: this.deserializeNode(),
      kind: this.deserializeString(),
      method: this.deserializeBoolean(),
      shorthand: this.deserializeBoolean(),
      computed: this.deserializeBoolean(),
    };
  },

  // 83: SpreadElement
  function () {
    return {
      type: 'SpreadElement',
      loc: this.addEmptyLoc(),
      argument: this.deserializeNode(),
    };
  },

  // 84: ClassExpression
  function () {
    const node = {
      type: 'ClassExpression',
      loc: this.addEmptyLoc(),
    };
    node.id = this.deserializeNode();
    node.typeParameters = this.deserializeNode();
    node.superClass = this.deserializeNode();
    node.implements = this.deserializeNodeList();
    node.body = this.deserializeNode();
    node.superTypeArguments = this.deserializeNode();
    node.decorators = this.deserializeNodeList();
    if (this.deserializeBoolean()) node['abstract'] = true;
    return node;
  },

  // 85: ClassBody
  function () {
    return {
      type: 'ClassBody',
      loc: this.addEmptyLoc(),
      body: this.deserializeNodeList(),
    };
  },

  // 86: ClassImplements
  function () {
    return {
      type: 'ClassImplements',
      loc: this.addEmptyLoc(),
      id: this.deserializeNode(),
      typeArguments: this.deserializeNode(),
    };
  },

  // 87: MethodDefinition
  function () {
    return {
      type: 'MethodDefinition',
      loc: this.addEmptyLoc(),
      key: this.deserializeNode(),
      value: this.deserializeNode(),
      kind: this.deserializeString(),
      static: this.deserializeBoolean(),
      computed: this.deserializeBoolean(),
      decorators: this.deserializeNodeList(),
      override: this.deserializeBoolean(),
      tsAccessibility: this.deserializeString(),
    };
  },

  // 88: PropertyDefinition
  function () {
    return {
      type: 'PropertyDefinition',
      loc: this.addEmptyLoc(),
      key: this.deserializeNode(),
      value: this.deserializeNode(),
      typeAnnotation: this.deserializeNode(),
      computed: this.deserializeBoolean(),
      static: this.deserializeBoolean(),
      variance: this.deserializeNode(),
      tsAccessibility: this.deserializeString(),
      declare: this.deserializeBoolean(),
      optional: this.deserializeBoolean(),
      override: this.deserializeBoolean(),
      decorators: this.deserializeNodeList(),
    };
  },

  // 89: StaticBlock
  function () {
    return {
      type: 'StaticBlock',
      loc: this.addEmptyLoc(),
      body: this.deserializeNodeList(),
    };
  },

  // 90: Decorator
  function () {
    return {
      type: 'Decorator',
      loc: this.addEmptyLoc(),
      expression: this.deserializeNode(),
    };
  },

  // 91: ParameterProperty
  function () {
    return {
      type: 'ParameterProperty',
      loc: this.addEmptyLoc(),
      key: this.deserializeNode(),
      value: this.deserializeNode(),
      typeAnnotation: this.deserializeNode(),
      computed: this.deserializeBoolean(),
      static: this.deserializeBoolean(),
      variance: this.deserializeNode(),
      tsAccessibility: this.deserializeString(),
      declare: this.deserializeBoolean(),
      optional: this.deserializeBoolean(),
      decorators: this.deserializeNodeList(),
    };
  },

  // 92: DeclareMethodDefinition
  function () {
    const node = {
      type: 'DeclareMethodDefinition',
      loc: this.addEmptyLoc(),
    };
    node.key = this.deserializeNode();
    node.value = this.deserializeNode();
    node.static = this.deserializeBoolean();
    node.optional = this.deserializeBoolean();
    node.computed = this.deserializeBoolean();
    node.kind = this.deserializeString();
    node.override = this.deserializeBoolean();
    {
      const value = this.deserializeString();
      if (value != null) node['tsAccessibility'] = value;
    }
    return node;
  },

  // 93: AbstractMethodDefinition
  function () {
    const node = {
      type: 'AbstractMethodDefinition',
      loc: this.addEmptyLoc(),
    };
    node.key = this.deserializeNode();
    node.value = this.deserializeNode();
    node.computed = this.deserializeBoolean();
    if (this.deserializeBoolean()) node['override'] = true;
    {
      const value = this.deserializeString();
      if (value != null) node['tsAccessibility'] = value;
    }
    return node;
  },

  // 94: AbstractPropertyDefinition
  function () {
    const node = {
      type: 'AbstractPropertyDefinition',
      loc: this.addEmptyLoc(),
    };
    node.key = this.deserializeNode();
    node.value = this.deserializeNode();
    node.typeAnnotation = this.deserializeNode();
    node.computed = this.deserializeBoolean();
    node.variance = this.deserializeNode();
    if (this.deserializeBoolean()) node['override'] = true;
    {
      const value = this.deserializeString();
      if (value != null) node['tsAccessibility'] = value;
    }
    return node;
  },

  // 95: SwitchCase
  function () {
    return {
      type: 'SwitchCase',
      loc: this.addEmptyLoc(),
      test: this.deserializeNode(),
      consequent: this.deserializeNodeList(),
    };
  },

  // 96: CatchClause
  function () {
    return {
      type: 'CatchClause',
      loc: this.addEmptyLoc(),
      param: this.deserializeNode(),
      body: this.deserializeNode(),
    };
  },

  // 97: ComponentParameter
  function () {
    return {
      type: 'ComponentParameter',
      loc: this.addEmptyLoc(),
      name: this.deserializeNode(),
      local: this.deserializeNode(),
      shorthand: this.deserializeBoolean(),
    };
  },

  // 98: DeclareVariable
  function () {
    const node = {
      type: 'DeclareVariable',
      loc: this.addEmptyLoc(),
    };
    node.declarations = this.deserializeNodeList();
    node.kind = this.deserializeString();
    if (this.deserializeBoolean()) node['implicitDeclare'] = true;
    return node;
  },

  // 99: DeclareFunction
  function () {
    const node = {
      type: 'DeclareFunction',
      loc: this.addEmptyLoc(),
    };
    node.id = this.deserializeNode();
    {
      const value = this.deserializeNode();
      if (value != null) node['typeAnnotation'] = value;
    }
    if (this.deserializeBoolean()) node['implicitDeclare'] = true;
    node.predicate = this.deserializeNode();
    return node;
  },

  // 100: DeclareClass
  function () {
    const node = {
      type: 'DeclareClass',
      loc: this.addEmptyLoc(),
    };
    node.id = this.deserializeNode();
    node.typeParameters = this.deserializeNode();
    node.extends = this.deserializeNodeList();
    node.implements = this.deserializeNodeList();
    node.mixins = this.deserializeNodeList();
    node.body = this.deserializeNode();
    if (this.deserializeBoolean()) node['implicitDeclare'] = true;
    if (this.deserializeBoolean()) node['abstract'] = true;
    return node;
  },

  // 101: DeclareComponent
  function () {
    const node = {
      type: 'DeclareComponent',
      loc: this.addEmptyLoc(),
    };
    node.id = this.deserializeNode();
    node.params = this.deserializeNodeList();
    node.rest = this.deserializeNode();
    node.rendersType = this.deserializeNode();
    node.typeParameters = this.deserializeNode();
    if (this.deserializeBoolean()) node['implicitDeclare'] = true;
    return node;
  },

  // 102: DeclareHook
  function () {
    const node = {
      type: 'DeclareHook',
      loc: this.addEmptyLoc(),
    };
    node.id = this.deserializeNode();
    if (this.deserializeBoolean()) node['implicitDeclare'] = true;
    return node;
  },

  // 103: DeclareModule
  function () {
    return {
      type: 'DeclareModule',
      loc: this.addEmptyLoc(),
      id: this.deserializeNode(),
      body: this.deserializeNode(),
    };
  },

  // 104: DeclareModuleExports
  function () {
    return {
      type: 'DeclareModuleExports',
      loc: this.addEmptyLoc(),
      typeAnnotation: this.deserializeNode(),
    };
  },

  // 105: DeclareExportDeclaration
  function () {
    const node = {
      type: 'DeclareExportDeclaration',
      loc: this.addEmptyLoc(),
    };
    node.default = this.deserializeBoolean();
    node.declaration = this.deserializeNode();
    node.specifiers = this.deserializeNodeList();
    node.source = this.deserializeNode();
    if (this.deserializeBoolean()) node['implicitDeclare'] = true;
    return node;
  },

  // 106: DeclareExportAllDeclaration
  function () {
    const node = {
      type: 'DeclareExportAllDeclaration',
      loc: this.addEmptyLoc(),
    };
    node.source = this.deserializeNode();
    if (this.deserializeBoolean()) node['implicitDeclare'] = true;
    return node;
  },

  // 107: DeclareNamespace
  function () {
    const node = {
      type: 'DeclareNamespace',
      loc: this.addEmptyLoc(),
    };
    node.global = this.deserializeBoolean();
    node.id = this.deserializeNode();
    node.body = this.deserializeNode();
    if (this.deserializeBoolean()) node['implicitDeclare'] = true;
    node.keyword = this.deserializeString();
    return node;
  },

  // 108: DeclareInterface
  function () {
    const node = {
      type: 'DeclareInterface',
      loc: this.addEmptyLoc(),
    };
    node.id = this.deserializeNode();
    node.typeParameters = this.deserializeNode();
    node.body = this.deserializeNode();
    node.extends = this.deserializeNodeList();
    if (this.deserializeBoolean()) node['implicitDeclare'] = true;
    return node;
  },

  // 109: DeclareTypeAlias
  function () {
    const node = {
      type: 'DeclareTypeAlias',
      loc: this.addEmptyLoc(),
    };
    node.id = this.deserializeNode();
    node.typeParameters = this.deserializeNode();
    node.right = this.deserializeNode();
    if (this.deserializeBoolean()) node['implicitDeclare'] = true;
    return node;
  },

  // 110: DeclareEnum
  function () {
    const node = {
      type: 'DeclareEnum',
      loc: this.addEmptyLoc(),
    };
    node.id = this.deserializeNode();
    node.body = this.deserializeNode();
    if (this.deserializeBoolean()) node['implicitDeclare'] = true;
    if (this.deserializeBoolean()) node['const'] = true;
    return node;
  },

  // 111: TypeAnnotation
  function () {
    return {
      type: 'TypeAnnotation',
      loc: this.addEmptyLoc(),
      typeAnnotation: this.deserializeNode(),
    };
  },

  // 112: Variance
  function () {
    return {
      type: 'Variance',
      loc: this.addEmptyLoc(),
      kind: this.deserializeString(),
    };
  },

  // 113: AnyTypeAnnotation
  function () {
    return {
      type: 'AnyTypeAnnotation',
      loc: this.addEmptyLoc(),
    };
  },

  // 114: MixedTypeAnnotation
  function () {
    return {
      type: 'MixedTypeAnnotation',
      loc: this.addEmptyLoc(),
    };
  },

  // 115: EmptyTypeAnnotation
  function () {
    return {
      type: 'EmptyTypeAnnotation',
      loc: this.addEmptyLoc(),
    };
  },

  // 116: VoidTypeAnnotation
  function () {
    return {
      type: 'VoidTypeAnnotation',
      loc: this.addEmptyLoc(),
    };
  },

  // 117: NullLiteralTypeAnnotation
  function () {
    return {
      type: 'NullLiteralTypeAnnotation',
      loc: this.addEmptyLoc(),
    };
  },

  // 118: SymbolTypeAnnotation
  function () {
    return {
      type: 'SymbolTypeAnnotation',
      loc: this.addEmptyLoc(),
    };
  },

  // 119: NumberTypeAnnotation
  function () {
    return {
      type: 'NumberTypeAnnotation',
      loc: this.addEmptyLoc(),
    };
  },

  // 120: BigIntTypeAnnotation
  function () {
    return {
      type: 'BigIntTypeAnnotation',
      loc: this.addEmptyLoc(),
    };
  },

  // 121: StringTypeAnnotation
  function () {
    return {
      type: 'StringTypeAnnotation',
      loc: this.addEmptyLoc(),
    };
  },

  // 122: BooleanTypeAnnotation
  function () {
    return {
      type: 'BooleanTypeAnnotation',
      loc: this.addEmptyLoc(),
    };
  },

  // 123: NullableTypeAnnotation
  function () {
    return {
      type: 'NullableTypeAnnotation',
      loc: this.addEmptyLoc(),
      typeAnnotation: this.deserializeNode(),
    };
  },

  // 124: ArrayTypeAnnotation
  function () {
    return {
      type: 'ArrayTypeAnnotation',
      loc: this.addEmptyLoc(),
      elementType: this.deserializeNode(),
    };
  },

  // 125: IndexedAccessType
  function () {
    return {
      type: 'IndexedAccessType',
      loc: this.addEmptyLoc(),
      objectType: this.deserializeNode(),
      indexType: this.deserializeNode(),
    };
  },

  // 126: OptionalIndexedAccessType
  function () {
    return {
      type: 'OptionalIndexedAccessType',
      loc: this.addEmptyLoc(),
      objectType: this.deserializeNode(),
      indexType: this.deserializeNode(),
      optional: this.deserializeBoolean(),
    };
  },

  // 127: UnionTypeAnnotation
  function () {
    return {
      type: 'UnionTypeAnnotation',
      loc: this.addEmptyLoc(),
      types: this.deserializeNodeList(),
    };
  },

  // 128: IntersectionTypeAnnotation
  function () {
    return {
      type: 'IntersectionTypeAnnotation',
      loc: this.addEmptyLoc(),
      types: this.deserializeNodeList(),
    };
  },

  // 129: KeyofTypeAnnotation
  function () {
    return {
      type: 'KeyofTypeAnnotation',
      loc: this.addEmptyLoc(),
      argument: this.deserializeNode(),
    };
  },

  // 130: TypeOperator
  function () {
    return {
      type: 'TypeOperator',
      loc: this.addEmptyLoc(),
      operator: this.deserializeString(),
      typeAnnotation: this.deserializeNode(),
    };
  },

  // 131: StringLiteralTypeAnnotation
  function () {
    return {
      type: 'StringLiteralTypeAnnotation',
      loc: this.addEmptyLoc(),
      value: this.deserializeString(),
      raw: this.deserializeString(),
    };
  },

  // 132: NumberLiteralTypeAnnotation
  function () {
    return {
      type: 'NumberLiteralTypeAnnotation',
      loc: this.addEmptyLoc(),
      value: this.deserializeNumber(),
      raw: this.deserializeString(),
    };
  },

  // 133: BigIntLiteralTypeAnnotation
  // Custom encoding: BigInt value constructed inline at deserialize time
  function () {
    const loc = this.addEmptyLoc();
    this.deserializeNode();
    const raw = this.deserializeString();
    const bigint = this.deserializeString();
    let value = null;
    if (bigint != null && typeof BigInt === 'function') {
      try {
        value = BigInt(bigint);
      } catch (e) {
        value = null;
      }
    }
    return {type: 'BigIntLiteralTypeAnnotation', loc, value, raw, bigint};
  },

  // 134: BooleanLiteralTypeAnnotation
  function () {
    return {
      type: 'BooleanLiteralTypeAnnotation',
      loc: this.addEmptyLoc(),
      value: this.deserializeBoolean(),
      raw: this.deserializeString(),
    };
  },

  // 135: ExistsTypeAnnotation
  function () {
    return {
      type: 'ExistsTypeAnnotation',
      loc: this.addEmptyLoc(),
    };
  },

  // 136: UnknownTypeAnnotation
  function () {
    return {
      type: 'UnknownTypeAnnotation',
      loc: this.addEmptyLoc(),
    };
  },

  // 137: NeverTypeAnnotation
  function () {
    return {
      type: 'NeverTypeAnnotation',
      loc: this.addEmptyLoc(),
    };
  },

  // 138: UndefinedTypeAnnotation
  function () {
    return {
      type: 'UndefinedTypeAnnotation',
      loc: this.addEmptyLoc(),
    };
  },

  // 139: GenericTypeAnnotation
  function () {
    return {
      type: 'GenericTypeAnnotation',
      loc: this.addEmptyLoc(),
      id: this.deserializeNode(),
      typeArguments: this.deserializeNode(),
    };
  },

  // 140: QualifiedTypeIdentifier
  function () {
    return {
      type: 'QualifiedTypeIdentifier',
      loc: this.addEmptyLoc(),
      qualification: this.deserializeNode(),
      id: this.deserializeNode(),
    };
  },

  // 141: QualifiedTypeofIdentifier
  function () {
    return {
      type: 'QualifiedTypeofIdentifier',
      loc: this.addEmptyLoc(),
      qualification: this.deserializeNode(),
      id: this.deserializeNode(),
    };
  },

  // 142: TypeofTypeAnnotation
  function () {
    const node = {
      type: 'TypeofTypeAnnotation',
      loc: this.addEmptyLoc(),
    };
    node.argument = this.deserializeNode();
    {
      const value = this.deserializeNode();
      if (value != null) node['typeArguments'] = value;
    }
    return node;
  },

  // 143: ImportType
  function () {
    return {
      type: 'ImportType',
      loc: this.addEmptyLoc(),
      source: this.deserializeNode(),
    };
  },

  // 144: TupleTypeAnnotation
  function () {
    return {
      type: 'TupleTypeAnnotation',
      loc: this.addEmptyLoc(),
      elementTypes: this.deserializeNodeList(),
      inexact: this.deserializeBoolean(),
    };
  },

  // 145: TupleTypeLabeledElement
  function () {
    return {
      type: 'TupleTypeLabeledElement',
      loc: this.addEmptyLoc(),
      label: this.deserializeNode(),
      elementType: this.deserializeNode(),
      variance: this.deserializeNode(),
      optional: this.deserializeBoolean(),
    };
  },

  // 146: TupleTypeSpreadElement
  function () {
    return {
      type: 'TupleTypeSpreadElement',
      loc: this.addEmptyLoc(),
      label: this.deserializeNode(),
      typeAnnotation: this.deserializeNode(),
    };
  },

  // 147: ObjectTypeAnnotation
  function () {
    return {
      type: 'ObjectTypeAnnotation',
      loc: this.addEmptyLoc(),
      properties: this.deserializeNodeList(),
      indexers: this.deserializeNodeList(),
      callProperties: this.deserializeNodeList(),
      internalSlots: this.deserializeNodeList(),
      inexact: this.deserializeBoolean(),
      exact: this.deserializeBoolean(),
    };
  },

  // 148: ObjectTypeProperty
  function () {
    const node = {
      type: 'ObjectTypeProperty',
      loc: this.addEmptyLoc(),
    };
    node.key = this.deserializeNode();
    node.value = this.deserializeNode();
    node.method = this.deserializeBoolean();
    node.optional = this.deserializeBoolean();
    node.static = this.deserializeBoolean();
    node.proto = this.deserializeBoolean();
    node.variance = this.deserializeNode();
    node.kind = this.deserializeString();
    if (this.deserializeBoolean()) node['computed'] = true;
    if (this.deserializeBoolean()) node['abstract'] = true;
    if (this.deserializeBoolean()) node['override'] = true;
    {
      const value = this.deserializeString();
      if (value != null) node['tsAccessibility'] = value;
    }
    {
      const value = this.deserializeNode();
      if (value != null) node['init'] = value;
    }
    return node;
  },

  // 149: ObjectTypeSpreadProperty
  function () {
    return {
      type: 'ObjectTypeSpreadProperty',
      loc: this.addEmptyLoc(),
      argument: this.deserializeNode(),
    };
  },

  // 150: ObjectTypeIndexer
  function () {
    return {
      type: 'ObjectTypeIndexer',
      loc: this.addEmptyLoc(),
      id: this.deserializeNode(),
      key: this.deserializeNode(),
      value: this.deserializeNode(),
      static: this.deserializeBoolean(),
      variance: this.deserializeNode(),
      optional: this.deserializeBoolean(),
    };
  },

  // 151: ObjectTypeCallProperty
  function () {
    return {
      type: 'ObjectTypeCallProperty',
      loc: this.addEmptyLoc(),
      value: this.deserializeNode(),
      static: this.deserializeBoolean(),
    };
  },

  // 152: ObjectTypeMappedTypeProperty
  function () {
    return {
      type: 'ObjectTypeMappedTypeProperty',
      loc: this.addEmptyLoc(),
      keyTparam: this.deserializeNode(),
      propType: this.deserializeNode(),
      sourceType: this.deserializeNode(),
      nameType: this.deserializeNode(),
      variance: this.deserializeNode(),
      varianceOp: this.deserializeString(),
      optional: this.deserializeString(),
    };
  },

  // 153: ObjectTypeInternalSlot
  function () {
    return {
      type: 'ObjectTypeInternalSlot',
      loc: this.addEmptyLoc(),
      id: this.deserializeNode(),
      optional: this.deserializeBoolean(),
      static: this.deserializeBoolean(),
      method: this.deserializeBoolean(),
      value: this.deserializeNode(),
    };
  },

  // 154: FunctionTypeParam
  function () {
    return {
      type: 'FunctionTypeParam',
      loc: this.addEmptyLoc(),
      name: this.deserializeNode(),
      typeAnnotation: this.deserializeNode(),
      optional: this.deserializeBoolean(),
    };
  },

  // 155: TypePredicate
  function () {
    return {
      type: 'TypePredicate',
      loc: this.addEmptyLoc(),
      parameterName: this.deserializeNode(),
      typeAnnotation: this.deserializeNode(),
      kind: this.deserializeString(),
    };
  },

  // 156: ConditionalTypeAnnotation
  function () {
    return {
      type: 'ConditionalTypeAnnotation',
      loc: this.addEmptyLoc(),
      checkType: this.deserializeNode(),
      extendsType: this.deserializeNode(),
      trueType: this.deserializeNode(),
      falseType: this.deserializeNode(),
    };
  },

  // 157: InferTypeAnnotation
  function () {
    return {
      type: 'InferTypeAnnotation',
      loc: this.addEmptyLoc(),
      typeParameter: this.deserializeNode(),
    };
  },

  // 158: InterfaceExtends
  function () {
    return {
      type: 'InterfaceExtends',
      loc: this.addEmptyLoc(),
      id: this.deserializeNode(),
      typeArguments: this.deserializeNode(),
    };
  },

  // 159: InterfaceTypeAnnotation
  function () {
    return {
      type: 'InterfaceTypeAnnotation',
      loc: this.addEmptyLoc(),
      extends: this.deserializeNodeList(),
      body: this.deserializeNode(),
    };
  },

  // 160: TypeParameterDeclaration
  function () {
    return {
      type: 'TypeParameterDeclaration',
      loc: this.addEmptyLoc(),
      params: this.deserializeNodeList(),
    };
  },

  // 161: TypeParameter
  function () {
    return {
      type: 'TypeParameter',
      loc: this.addEmptyLoc(),
      name: this.deserializeString(),
      const: this.deserializeBoolean(),
      bound: this.deserializeNode(),
      variance: this.deserializeNode(),
      default: this.deserializeNode(),
      usesExtendsBound: this.deserializeBoolean(),
    };
  },

  // 162: TypeParameterInstantiation
  function () {
    return {
      type: 'TypeParameterInstantiation',
      loc: this.addEmptyLoc(),
      params: this.deserializeNodeList(),
    };
  },

  // 163: ComponentTypeAnnotation
  function () {
    return {
      type: 'ComponentTypeAnnotation',
      loc: this.addEmptyLoc(),
      params: this.deserializeNodeList(),
      rest: this.deserializeNode(),
      rendersType: this.deserializeNode(),
      typeParameters: this.deserializeNode(),
    };
  },

  // 164: ComponentTypeParameter
  function () {
    return {
      type: 'ComponentTypeParameter',
      loc: this.addEmptyLoc(),
      name: this.deserializeNode(),
      typeAnnotation: this.deserializeNode(),
      optional: this.deserializeBoolean(),
    };
  },

  // 165: DeclaredPredicate
  function () {
    return {
      type: 'DeclaredPredicate',
      loc: this.addEmptyLoc(),
      value: this.deserializeNode(),
    };
  },

  // 166: InferredPredicate
  function () {
    return {
      type: 'InferredPredicate',
      loc: this.addEmptyLoc(),
    };
  },

  // 167: JSXElement
  function () {
    return {
      type: 'JSXElement',
      loc: this.addEmptyLoc(),
      openingElement: this.deserializeNode(),
      children: this.deserializeNodeList(),
      closingElement: this.deserializeNode(),
    };
  },

  // 168: JSXFragment
  function () {
    return {
      type: 'JSXFragment',
      loc: this.addEmptyLoc(),
      openingFragment: this.deserializeNode(),
      children: this.deserializeNodeList(),
      closingFragment: this.deserializeNode(),
    };
  },

  // 169: JSXOpeningElement
  function () {
    return {
      type: 'JSXOpeningElement',
      loc: this.addEmptyLoc(),
      name: this.deserializeNode(),
      attributes: this.deserializeNodeList(),
      selfClosing: this.deserializeBoolean(),
      typeArguments: this.deserializeNode(),
    };
  },

  // 170: JSXOpeningFragment
  function () {
    return {
      type: 'JSXOpeningFragment',
      loc: this.addEmptyLoc(),
    };
  },

  // 171: JSXClosingElement
  function () {
    return {
      type: 'JSXClosingElement',
      loc: this.addEmptyLoc(),
      name: this.deserializeNode(),
    };
  },

  // 172: JSXClosingFragment
  function () {
    return {
      type: 'JSXClosingFragment',
      loc: this.addEmptyLoc(),
    };
  },

  // 173: JSXAttribute
  function () {
    return {
      type: 'JSXAttribute',
      loc: this.addEmptyLoc(),
      name: this.deserializeNode(),
      value: this.deserializeNode(),
    };
  },

  // 174: JSXSpreadAttribute
  function () {
    return {
      type: 'JSXSpreadAttribute',
      loc: this.addEmptyLoc(),
      argument: this.deserializeNode(),
    };
  },

  // 175: JSXEmptyExpression
  function () {
    return {
      type: 'JSXEmptyExpression',
      loc: this.addEmptyLoc(),
    };
  },

  // 176: JSXExpressionContainer
  function () {
    return {
      type: 'JSXExpressionContainer',
      loc: this.addEmptyLoc(),
      expression: this.deserializeNode(),
    };
  },

  // 177: JSXSpreadChild
  function () {
    return {
      type: 'JSXSpreadChild',
      loc: this.addEmptyLoc(),
      expression: this.deserializeNode(),
    };
  },

  // 178: JSXText
  function () {
    return {
      type: 'JSXText',
      loc: this.addEmptyLoc(),
      value: this.deserializeString(),
      raw: this.deserializeString(),
    };
  },

  // 179: JSXMemberExpression
  function () {
    return {
      type: 'JSXMemberExpression',
      loc: this.addEmptyLoc(),
      object: this.deserializeNode(),
      property: this.deserializeNode(),
    };
  },

  // 180: JSXNamespacedName
  function () {
    return {
      type: 'JSXNamespacedName',
      loc: this.addEmptyLoc(),
      namespace: this.deserializeNode(),
      name: this.deserializeNode(),
    };
  },

  // 181: JSXIdentifier
  function () {
    return {
      type: 'JSXIdentifier',
      loc: this.addEmptyLoc(),
      name: this.deserializeString(),
    };
  },

  // 182: BabelEnumBooleanBody
  function () {
    return {
      type: 'EnumBooleanBody',
      loc: this.addEmptyLoc(),
      members: this.deserializeNodeList(),
      explicitType: this.deserializeBoolean(),
      hasUnknownMembers: this.deserializeBoolean(),
    };
  },

  // 183: EnumBooleanMember
  function () {
    return {
      type: 'EnumBooleanMember',
      loc: this.addEmptyLoc(),
      id: this.deserializeNode(),
      init: this.deserializeNode(),
    };
  },

  // 184: BabelEnumNumberBody
  function () {
    return {
      type: 'EnumNumberBody',
      loc: this.addEmptyLoc(),
      members: this.deserializeNodeList(),
      explicitType: this.deserializeBoolean(),
      hasUnknownMembers: this.deserializeBoolean(),
    };
  },

  // 185: EnumNumberMember
  function () {
    return {
      type: 'EnumNumberMember',
      loc: this.addEmptyLoc(),
      id: this.deserializeNode(),
      init: this.deserializeNode(),
    };
  },

  // 186: BabelEnumStringBody
  function () {
    return {
      type: 'EnumStringBody',
      loc: this.addEmptyLoc(),
      members: this.deserializeNodeList(),
      explicitType: this.deserializeBoolean(),
      hasUnknownMembers: this.deserializeBoolean(),
    };
  },

  // 187: EnumStringMember
  function () {
    return {
      type: 'EnumStringMember',
      loc: this.addEmptyLoc(),
      id: this.deserializeNode(),
      init: this.deserializeNode(),
    };
  },

  // 188: BabelEnumSymbolBody
  function () {
    return {
      type: 'EnumSymbolBody',
      loc: this.addEmptyLoc(),
      members: this.deserializeNodeList(),
      hasUnknownMembers: this.deserializeBoolean(),
    };
  },

  // 189: EnumDefaultedMember
  function () {
    return {
      type: 'EnumDefaultedMember',
      loc: this.addEmptyLoc(),
      id: this.deserializeNode(),
    };
  },

  // 190: BabelEnumBigIntBody
  function () {
    return {
      type: 'EnumBigIntBody',
      loc: this.addEmptyLoc(),
      members: this.deserializeNodeList(),
      explicitType: this.deserializeBoolean(),
      hasUnknownMembers: this.deserializeBoolean(),
    };
  },

  // 191: EnumBigIntMember
  function () {
    return {
      type: 'EnumBigIntMember',
      loc: this.addEmptyLoc(),
      id: this.deserializeNode(),
      init: this.deserializeNode(),
    };
  },

  // 192: RecordDeclarationBody
  function () {
    return {
      type: 'RecordDeclarationBody',
      loc: this.addEmptyLoc(),
      elements: this.deserializeNodeList(),
    };
  },

  // 193: RecordDeclarationProperty
  function () {
    return {
      type: 'RecordDeclarationProperty',
      loc: this.addEmptyLoc(),
      key: this.deserializeNode(),
      typeAnnotation: this.deserializeNode(),
      defaultValue: this.deserializeNode(),
    };
  },

  // 194: RecordDeclarationStaticProperty
  function () {
    return {
      type: 'RecordDeclarationStaticProperty',
      loc: this.addEmptyLoc(),
      key: this.deserializeNode(),
      typeAnnotation: this.deserializeNode(),
      value: this.deserializeNode(),
    };
  },

  // 195: RecordDeclarationImplements
  function () {
    return {
      type: 'RecordDeclarationImplements',
      loc: this.addEmptyLoc(),
      id: this.deserializeNode(),
      typeArguments: this.deserializeNode(),
    };
  },

  // 196: MatchExpressionCase
  function () {
    return {
      type: 'MatchExpressionCase',
      loc: this.addEmptyLoc(),
      pattern: this.deserializeNode(),
      body: this.deserializeNode(),
      guard: this.deserializeNode(),
    };
  },

  // 197: MatchStatementCase
  function () {
    return {
      type: 'MatchStatementCase',
      loc: this.addEmptyLoc(),
      pattern: this.deserializeNode(),
      body: this.deserializeNode(),
      guard: this.deserializeNode(),
    };
  },

  // 198: MatchWildcardPattern
  function () {
    return {
      type: 'MatchWildcardPattern',
      loc: this.addEmptyLoc(),
    };
  },

  // 199: MatchLiteralPattern
  function () {
    return {
      type: 'MatchLiteralPattern',
      loc: this.addEmptyLoc(),
      literal: this.deserializeNode(),
    };
  },

  // 200: MatchUnaryPattern
  function () {
    return {
      type: 'MatchUnaryPattern',
      loc: this.addEmptyLoc(),
      operator: this.deserializeString(),
      argument: this.deserializeNode(),
    };
  },

  // 201: MatchObjectPattern
  function () {
    return {
      type: 'MatchObjectPattern',
      loc: this.addEmptyLoc(),
      properties: this.deserializeNodeList(),
      rest: this.deserializeNode(),
    };
  },

  // 202: MatchInstancePattern
  function () {
    return {
      type: 'MatchInstancePattern',
      loc: this.addEmptyLoc(),
      targetConstructor: this.deserializeNode(),
      properties: this.deserializeNode(),
    };
  },

  // 203: MatchInstanceObjectPattern
  function () {
    return {
      type: 'MatchInstanceObjectPattern',
      loc: this.addEmptyLoc(),
      properties: this.deserializeNodeList(),
      rest: this.deserializeNode(),
    };
  },

  // 204: MatchOrPattern
  function () {
    return {
      type: 'MatchOrPattern',
      loc: this.addEmptyLoc(),
      patterns: this.deserializeNodeList(),
    };
  },

  // 205: MatchAsPattern
  function () {
    return {
      type: 'MatchAsPattern',
      loc: this.addEmptyLoc(),
      pattern: this.deserializeNode(),
      target: this.deserializeNode(),
    };
  },

  // 206: MatchIdentifierPattern
  function () {
    return {
      type: 'MatchIdentifierPattern',
      loc: this.addEmptyLoc(),
      id: this.deserializeNode(),
    };
  },

  // 207: MatchMemberPattern
  function () {
    return {
      type: 'MatchMemberPattern',
      loc: this.addEmptyLoc(),
      base: this.deserializeNode(),
      property: this.deserializeNode(),
    };
  },

  // 208: MatchBindingPattern
  function () {
    return {
      type: 'MatchBindingPattern',
      loc: this.addEmptyLoc(),
      id: this.deserializeNode(),
      kind: this.deserializeString(),
    };
  },

  // 209: MatchArrayPattern
  function () {
    return {
      type: 'MatchArrayPattern',
      loc: this.addEmptyLoc(),
      elements: this.deserializeNodeList(),
      rest: this.deserializeNode(),
    };
  },

  // 210: MatchObjectPatternProperty
  function () {
    return {
      type: 'MatchObjectPatternProperty',
      loc: this.addEmptyLoc(),
      key: this.deserializeNode(),
      pattern: this.deserializeNode(),
      shorthand: this.deserializeBoolean(),
    };
  },

  // 211: MatchRestPattern
  function () {
    return {
      type: 'MatchRestPattern',
      loc: this.addEmptyLoc(),
      argument: this.deserializeNode(),
    };
  },

  // 212: Program
  function () {
    return {
      type: 'Program',
      loc: this.addEmptyLoc(),
      body: this.deserializeNodeList(),
    };
  },

  // 213: InterpreterDirective
  function () {
    return {
      type: 'InterpreterDirective',
      loc: this.addEmptyLoc(),
      value: this.deserializeString(),
    };
  },

  // 214: FunctionTypeAnnotation
  function () {
    return {
      type: 'FunctionTypeAnnotation',
      loc: this.addEmptyLoc(),
      params: this.deserializeNodeList(),
      this: this.deserializeNode(),
      returnType: this.deserializeNode(),
      rest: this.deserializeNode(),
      typeParameters: this.deserializeNode(),
    };
  },

  // 215: RendersType
  function () {
    return {
      type: 'RendersType',
      loc: this.addEmptyLoc(),
      argument: this.deserializeNode(),
    };
  },

  // 216: RendersMaybeType
  function () {
    return {
      type: 'RendersMaybeType',
      loc: this.addEmptyLoc(),
      argument: this.deserializeNode(),
    };
  },

  // 217: RendersStarType
  function () {
    return {
      type: 'RendersStarType',
      loc: this.addEmptyLoc(),
      argument: this.deserializeNode(),
    };
  },

  // 218: TemplateLiteralTypeAnnotation
  function () {
    return {
      type: 'TemplateLiteralTypeAnnotation',
      loc: this.addEmptyLoc(),
      quasis: this.deserializeNodeList(),
      types: this.deserializeNodeList(),
    };
  },

  // 219: NamespaceExportDeclaration
  function () {
    return {
      type: 'NamespaceExportDeclaration',
      loc: this.addEmptyLoc(),
      id: this.deserializeNode(),
    };
  },

  // 220: DeclareClassExtendsCall
  function () {
    return {
      type: 'DeclareClassExtendsCall',
      loc: this.addEmptyLoc(),
      callee: this.deserializeNode(),
      argument: this.deserializeNode(),
    };
  },

  // 221: OpaqueType
  function () {
    return {
      type: 'OpaqueType',
      loc: this.addEmptyLoc(),
      id: this.deserializeNode(),
      typeParameters: this.deserializeNode(),
      impltype: this.deserializeNode(),
      lowerBound: this.deserializeNode(),
      upperBound: this.deserializeNode(),
      supertype: this.deserializeNode(),
    };
  },

  // 222: DeclareOpaqueType
  function () {
    const node = {
      type: 'DeclareOpaqueType',
      loc: this.addEmptyLoc(),
    };
    node.id = this.deserializeNode();
    node.typeParameters = this.deserializeNode();
    node.impltype = this.deserializeNode();
    node.lowerBound = this.deserializeNode();
    node.upperBound = this.deserializeNode();
    node.supertype = this.deserializeNode();
    if (this.deserializeBoolean()) node['implicitDeclare'] = true;
    return node;
  },

  // 223: ESTreeEnumBody
  function () {
    return {
      type: 'EnumBody',
      loc: this.addEmptyLoc(),
      members: this.deserializeNodeList(),
      explicitType: this.deserializeString(),
      hasUnknownMembers: this.deserializeBoolean(),
    };
  },

  // 224: HookTypeAnnotation
  function () {
    return {
      type: 'HookTypeAnnotation',
      loc: this.addEmptyLoc(),
      params: this.deserializeNodeList(),
      returnType: this.deserializeNode(),
      rest: this.deserializeNode(),
      typeParameters: this.deserializeNode(),
    };
  },

  // 225: ConstructorTypeAnnotation
  function () {
    return {
      type: 'ConstructorTypeAnnotation',
      loc: this.addEmptyLoc(),
      abstract: this.deserializeBoolean(),
      params: this.deserializeNodeList(),
      returnType: this.deserializeNode(),
      rest: this.deserializeNode(),
      typeParameters: this.deserializeNode(),
    };
  },

  // 226: ObjectTypePrivateField
  function () {
    return {
      type: 'ObjectTypePrivateField',
      loc: this.addEmptyLoc(),
      key: this.deserializeNode(),
    };
  },

  // 227: TupleTypeElement
  function () {
    return {
      type: 'TupleTypeElement',
      loc: this.addEmptyLoc(),
      elementType: this.deserializeNode(),
      optional: this.deserializeBoolean(),
    };
  },

  // 228: DeclareComponentAmbient
  function () {
    const node = {
      type: 'DeclareComponent',
      loc: this.addEmptyLoc(),
    };
    node.body = this.deserializeNode();
    node.id = this.deserializeNode();
    if (this.deserializeBoolean()) node['implicitDeclare'] = true;
    node.params = this.deserializeNodeList();
    node.rest = this.deserializeNode();
    node.rendersType = this.deserializeNode();
    node.typeParameters = this.deserializeNode();
    node.async = this.deserializeBoolean();
    return node;
  },

  // 229: ThisTypeAnnotation
  function () {
    return {
      type: 'ThisTypeAnnotation',
      loc: this.addEmptyLoc(),
    };
  },

  // 230: ChainExpression
  function () {
    return {
      type: 'ChainExpression',
      loc: this.addEmptyLoc(),
      expression: this.deserializeNode(),
    };
  },

  // 231: BabelExpressionStatement
  function () {
    return {
      type: 'ExpressionStatement',
      loc: this.addEmptyLoc(),
      expression: this.deserializeNode(),
    };
  },

  // 232: BabelBlockStatement
  function () {
    return {
      type: 'BlockStatement',
      loc: this.addEmptyLoc(),
      body: this.deserializeNodeList(),
      directives: this.deserializeNodeList(),
    };
  },

  // 233: BabelDirective
  function () {
    return {
      type: 'Directive',
      loc: this.addEmptyLoc(),
      value: this.deserializeNode(),
    };
  },

  // 234: BabelDirectiveLiteral
  function () {
    const loc = this.addEmptyLoc();
    const value = this.deserializeString();
    const rawValue = this.deserializeString();
    const raw = this.deserializeString();
    return {type: 'DirectiveLiteral', loc, value, extra: {rawValue, raw}};
  },

  // 235: BabelObjectProperty
  function () {
    return {
      type: 'ObjectProperty',
      loc: this.addEmptyLoc(),
      computed: this.deserializeBoolean(),
      key: this.deserializeNode(),
      value: this.deserializeNode(),
      method: this.deserializeBoolean(),
      shorthand: this.deserializeBoolean(),
    };
  },

  // 236: BabelObjectMethod
  function () {
    const node = {
      type: 'ObjectMethod',
      loc: this.addEmptyLoc(),
    };
    node.kind = this.deserializeString();
    node.method = this.deserializeBoolean();
    node.computed = this.deserializeBoolean();
    node.key = this.deserializeNode();
    node.id = this.deserializeNode();
    node.params = this.deserializeNodeList();
    node.body = this.deserializeNode();
    node.async = this.deserializeBoolean();
    node.generator = this.deserializeBoolean();
    {
      const value = this.deserializeNode();
      if (value != null) node['returnType'] = value;
    }
    {
      const value = this.deserializeNode();
      if (value != null) node['typeParameters'] = value;
    }
    if (this.deserializeBoolean()) {
      node['variance'] = this.deserializeNode();
    }
    return node;
  },

  // 237: BabelClassMethod
  function () {
    const node = {
      type: 'ClassMethod',
      loc: this.addEmptyLoc(),
    };
    node.kind = this.deserializeString();
    node.computed = this.deserializeBoolean();
    node.static = this.deserializeBoolean();
    node.key = this.deserializeNode();
    node.id = this.deserializeNode();
    node.params = this.deserializeNodeList();
    node.body = this.deserializeNode();
    node.async = this.deserializeBoolean();
    node.generator = this.deserializeBoolean();
    {
      const value = this.deserializeNode();
      if (value != null) node['returnType'] = value;
    }
    {
      const value = this.deserializeNode();
      if (value != null) node['typeParameters'] = value;
    }
    {
      const value = this.deserializeNode();
      if (value != null) node['predicate'] = value;
    }
    return node;
  },

  // 238: BabelClassPrivateMethod
  function () {
    const node = {
      type: 'ClassPrivateMethod',
      loc: this.addEmptyLoc(),
    };
    node.kind = this.deserializeString();
    node.static = this.deserializeBoolean();
    node.key = this.deserializeNode();
    node.id = this.deserializeNode();
    node.params = this.deserializeNodeList();
    node.body = this.deserializeNode();
    node.async = this.deserializeBoolean();
    node.generator = this.deserializeBoolean();
    {
      const value = this.deserializeNode();
      if (value != null) node['returnType'] = value;
    }
    {
      const value = this.deserializeNode();
      if (value != null) node['typeParameters'] = value;
    }
    {
      const value = this.deserializeNode();
      if (value != null) node['predicate'] = value;
    }
    return node;
  },

  // 239: BabelClassProperty
  function () {
    const node = {
      type: 'ClassProperty',
      loc: this.addEmptyLoc(),
    };
    node.key = this.deserializeNode();
    node.value = this.deserializeNode();
    {
      const value = this.deserializeNode();
      if (value != null) node['typeAnnotation'] = value;
    }
    node.static = this.deserializeBoolean();
    node.variance = this.deserializeNode();
    if (this.deserializeBoolean()) node['declare'] = true;
    if (this.deserializeBoolean()) node['optional'] = true;
    node.computed = this.deserializeBoolean();
    return node;
  },

  // 240: BabelClassPrivateProperty
  function () {
    const node = {
      type: 'ClassPrivateProperty',
      loc: this.addEmptyLoc(),
    };
    node.key = this.deserializeNode();
    node.value = this.deserializeNode();
    {
      const value = this.deserializeNode();
      if (value != null) node['typeAnnotation'] = value;
    }
    node.static = this.deserializeBoolean();
    node.variance = this.deserializeNode();
    return node;
  },

  // 241: BabelPrivateName
  function () {
    return {
      type: 'PrivateName',
      loc: this.addEmptyLoc(),
      id: this.deserializeNode(),
    };
  },

  // 242: BabelStringLiteral
  function () {
    const loc = this.addEmptyLoc();
    const value = this.deserializeString();
    const rawValue = value;
    const raw = this.deserializeString();
    return {type: 'StringLiteral', loc, value, extra: {rawValue, raw}};
  },

  // 243: BabelNumericLiteral
  function () {
    const loc = this.addEmptyLoc();
    const value = this.deserializeNumber();
    const raw = this.deserializeString();
    return {type: 'NumericLiteral', loc, value, extra: {rawValue: value, raw}};
  },

  // 244: BabelBigIntLiteral
  function () {
    const loc = this.addEmptyLoc();
    const value = this.deserializeString();
    const rawValue = value;
    const raw = this.deserializeString();
    return {type: 'BigIntLiteral', loc, value, extra: {rawValue, raw}};
  },

  // 245: BabelBooleanLiteral
  function () {
    return {
      type: 'BooleanLiteral',
      loc: this.addEmptyLoc(),
      value: this.deserializeBoolean(),
    };
  },

  // 246: BabelNullLiteral
  function () {
    return {
      type: 'NullLiteral',
      loc: this.addEmptyLoc(),
    };
  },

  // 247: BabelRegExpLiteral
  function () {
    const loc = this.addEmptyLoc();
    const raw = this.deserializeString();
    const pattern = this.deserializeString();
    const flags = this.deserializeString();
    return {type: 'RegExpLiteral', loc, extra: {raw}, pattern, flags};
  },

  // 248: BabelImport
  function () {
    return {
      type: 'Import',
      loc: this.addEmptyLoc(),
    };
  },

  // 249: BabelTupleTypeAnnotation
  function () {
    return {
      type: 'TupleTypeAnnotation',
      loc: this.addEmptyLoc(),
      types: this.deserializeNodeList(),
    };
  },

  // 250: BabelStringLiteralTypeAnnotation
  function () {
    const loc = this.addEmptyLoc();
    const value = this.deserializeString();
    const rawValue = value;
    const raw = this.deserializeString();
    return {type: 'StringLiteralTypeAnnotation', loc, value, extra: {rawValue, raw}};
  },

  // 251: BabelNumberLiteralTypeAnnotation
  function () {
    const loc = this.addEmptyLoc();
    const value = this.deserializeNumber();
    const raw = this.deserializeString();
    return {type: 'NumberLiteralTypeAnnotation', loc, value, extra: {rawValue: value, raw}};
  },

  // 252: BabelBigIntLiteralTypeAnnotation
  function () {
    const loc = this.addEmptyLoc();
    const bigint = this.deserializeString();
    const raw = this.deserializeString();
    let value = null;
    if (bigint != null && typeof BigInt === 'function') {
      try {
        value = BigInt(bigint);
      } catch (error) {
        value = null;
      }
    }
    return {type: 'BigIntLiteralTypeAnnotation', loc, value, extra: {rawValue: bigint, raw}};
  },

  // 253: BabelBooleanLiteralTypeAnnotation
  function () {
    return {
      type: 'BooleanLiteralTypeAnnotation',
      loc: this.addEmptyLoc(),
      value: this.deserializeBoolean(),
    };
  },

  // 254: BabelJSXText
  function () {
    const loc = this.addEmptyLoc();
    const value = this.deserializeString();
    const rawValue = value;
    const raw = this.deserializeString();
    return {type: 'JSXText', loc, value, extra: {rawValue, raw}};
  },

  // 255: BabelClassDeclaration
  function () {
    const node = {
      type: 'ClassDeclaration',
      loc: this.addEmptyLoc(),
    };
    node.id = this.deserializeNode();
    {
      const value = this.deserializeNode();
      if (value != null) node['typeParameters'] = value;
    }
    node.superClass = this.deserializeNode();
    {
      const value = this.deserializeNodeList();
      if (value.length > 0) node['implements'] = value;
    }
    node.body = this.deserializeNode();
    {
      const value = this.deserializeNode();
      if (value != null) node['superTypeParameters'] = value;
    }
    {
      const value = this.deserializeNodeList();
      if (value.length > 0) node['decorators'] = value;
    }
    return node;
  },

  // 256: BabelClassExpression
  function () {
    const node = {
      type: 'ClassExpression',
      loc: this.addEmptyLoc(),
    };
    node.id = this.deserializeNode();
    {
      const value = this.deserializeNode();
      if (value != null) node['typeParameters'] = value;
    }
    node.superClass = this.deserializeNode();
    {
      const value = this.deserializeNodeList();
      if (value.length > 0) node['implements'] = value;
    }
    node.body = this.deserializeNode();
    {
      const value = this.deserializeNode();
      if (value != null) node['superTypeParameters'] = value;
    }
    {
      const value = this.deserializeNodeList();
      if (value.length > 0) node['decorators'] = value;
    }
    return node;
  },

  // 257: BabelFunctionDeclaration
  function () {
    const node = {
      type: 'FunctionDeclaration',
      loc: this.addEmptyLoc(),
    };
    node.id = this.deserializeNode();
    node.params = this.deserializeNodeList();
    node.body = this.deserializeNode();
    {
      const value = this.deserializeNode();
      if (value != null) node['typeParameters'] = value;
    }
    {
      const value = this.deserializeNode();
      if (value != null) node['returnType'] = value;
    }
    node.generator = this.deserializeBoolean();
    node.async = this.deserializeBoolean();
    {
      const value = this.deserializeNode();
      if (value != null) node['predicate'] = value;
    }
    if (this.deserializeBoolean()) node['__componentDeclaration'] = true;
    if (this.deserializeBoolean()) node['__hookDeclaration'] = true;
    return node;
  },

  // 258: BabelFunctionExpression
  function () {
    const node = {
      type: 'FunctionExpression',
      loc: this.addEmptyLoc(),
    };
    node.id = this.deserializeNode();
    node.params = this.deserializeNodeList();
    node.body = this.deserializeNode();
    {
      const value = this.deserializeNode();
      if (value != null) node['typeParameters'] = value;
    }
    {
      const value = this.deserializeNode();
      if (value != null) node['returnType'] = value;
    }
    node.generator = this.deserializeBoolean();
    node.async = this.deserializeBoolean();
    {
      const value = this.deserializeNode();
      if (value != null) node['predicate'] = value;
    }
    return node;
  },

  // 259: BabelArrowFunctionExpression
  function () {
    const node = {
      type: 'ArrowFunctionExpression',
      loc: this.addEmptyLoc(),
    };
    node.params = this.deserializeNodeList();
    node.body = this.deserializeNode();
    {
      const value = this.deserializeNode();
      if (value != null) node['typeParameters'] = value;
    }
    {
      const value = this.deserializeNode();
      if (value != null) node['returnType'] = value;
    }
    node.async = this.deserializeBoolean();
    node.id = this.deserializeNode();
    {
      const value = this.deserializeNode();
      if (value != null) node['predicate'] = value;
    }
    return node;
  },

  // 260: BabelMemberExpression
  function () {
    return {
      type: 'MemberExpression',
      loc: this.addEmptyLoc(),
      object: this.deserializeNode(),
      property: this.deserializeNode(),
      computed: this.deserializeBoolean(),
    };
  },

  // 261: BabelArrayExpression
  function () {
    return {
      type: 'ArrayExpression',
      loc: this.addEmptyLoc(),
      elements: this.deserializeNodeList(),
    };
  },

  // 262: BabelRestElement
  function () {
    const node = {
      type: 'RestElement',
      loc: this.addEmptyLoc(),
    };
    node.argument = this.deserializeNode();
    if (this.deserializeBoolean()) {
      node['typeAnnotation'] = this.deserializeNode();
    }
    return node;
  },

  // 263: BabelOpaqueType
  function () {
    return {
      type: 'OpaqueType',
      loc: this.addEmptyLoc(),
      id: this.deserializeNode(),
      typeParameters: this.deserializeNode(),
      impltype: this.deserializeNode(),
      supertype: this.deserializeNode(),
    };
  },

  // 264: BabelDeclareOpaqueType
  function () {
    const node = {
      type: 'DeclareOpaqueType',
      loc: this.addEmptyLoc(),
    };
    node.id = this.deserializeNode();
    node.typeParameters = this.deserializeNode();
    node.impltype = this.deserializeNode();
    node.supertype = this.deserializeNode();
    if (this.deserializeBoolean()) node['implicitDeclare'] = true;
    return node;
  },

  // 265: BabelExportAllDeclaration
  function () {
    const node = {
      type: 'ExportAllDeclaration',
      loc: this.addEmptyLoc(),
    };
    node.source = this.deserializeNode();
    node.exportKind = this.deserializeString();
    {
      const value = this.deserializeNodeList();
      if (value.length > 0) node['attributes'] = value;
    }
    return node;
  },

  // 266: BabelDeclareVariable
  function () {
    return {
      type: 'DeclareVariable',
      loc: this.addEmptyLoc(),
      id: this.deserializeNode(),
    };
  },

  // 267: BabelIdentifier
  function () {
    const node = {
      type: 'Identifier',
      loc: this.addEmptyLoc(),
    };
    node.name = this.deserializeString();
    {
      const value = this.deserializeNode();
      if (value != null) node['typeAnnotation'] = value;
    }
    if (this.deserializeBoolean()) node['optional'] = true;
    return node;
  },

  // 268: BabelObjectPattern
  function () {
    const node = {
      type: 'ObjectPattern',
      loc: this.addEmptyLoc(),
    };
    node.properties = this.deserializeNodeList();
    {
      const value = this.deserializeNode();
      if (value != null) node['typeAnnotation'] = value;
    }
    if (this.deserializeBoolean()) {
      node['optional'] = this.deserializeBoolean();
    }
    return node;
  },

  // 269: BabelArrayPattern
  function () {
    const node = {
      type: 'ArrayPattern',
      loc: this.addEmptyLoc(),
    };
    node.elements = this.deserializeNodeList();
    {
      const value = this.deserializeNode();
      if (value != null) node['typeAnnotation'] = value;
    }
    if (this.deserializeBoolean()) {
      node['optional'] = this.deserializeBoolean();
    }
    return node;
  },

  // 270: BabelCallExpression
  function () {
    const node = {
      type: 'CallExpression',
      loc: this.addEmptyLoc(),
    };
    node.callee = this.deserializeNode();
    {
      const value = this.deserializeNode();
      if (value != null) node['typeArguments'] = value;
    }
    node.arguments = this.deserializeNodeList();
    if (this.deserializeBoolean()) node['optional'] = true;
    return node;
  },

  // 271: BabelOptionalCallExpression
  function () {
    const node = {
      type: 'OptionalCallExpression',
      loc: this.addEmptyLoc(),
    };
    node.callee = this.deserializeNode();
    {
      const value = this.deserializeNode();
      if (value != null) node['typeArguments'] = value;
    }
    node.arguments = this.deserializeNodeList();
    node.optional = this.deserializeBoolean();
    return node;
  },

  // 272: BabelImportDeclaration
  function () {
    const node = {
      type: 'ImportDeclaration',
      loc: this.addEmptyLoc(),
    };
    node.specifiers = this.deserializeNodeList();
    node.source = this.deserializeNode();
    node.importKind = this.deserializeString();
    {
      const value = this.deserializeNodeList();
      if (value.length > 0) node['attributes'] = value;
    }
    {
      const value = this.deserializeString();
      if (value != null) node['phase'] = value;
    }
    return node;
  },

  // 273: BabelExportNamedDeclaration
  function () {
    const node = {
      type: 'ExportNamedDeclaration',
      loc: this.addEmptyLoc(),
    };
    {
      const value = this.deserializeNode();
      if (value != null) node['declaration'] = value;
    }
    node.specifiers = this.deserializeNodeList();
    node.source = this.deserializeNode();
    node.exportKind = this.deserializeString();
    {
      const value = this.deserializeNodeList();
      if (value.length > 0) node['attributes'] = value;
    }
    return node;
  },

  // 274: BabelJSXOpeningElement
  function () {
    return {
      type: 'JSXOpeningElement',
      loc: this.addEmptyLoc(),
      name: this.deserializeNode(),
      attributes: this.deserializeNodeList(),
      selfClosing: this.deserializeBoolean(),
    };
  },

  // 275: BabelTypeofTypeAnnotation
  function () {
    return {
      type: 'TypeofTypeAnnotation',
      loc: this.addEmptyLoc(),
      argument: this.deserializeNode(),
    };
  },

  // 276: BabelEnumRuntime
  function () {
    this.addEmptyLoc();
    return this.deserializeEnumRuntime();
  },

  // 277: BabelProgram
  function () {
    return {
      type: 'Program',
      loc: this.addEmptyLoc(),
      body: this.deserializeNodeList(),
      directives: this.deserializeNodeList(),
      sourceType: this.deserializeString(),
      interpreter: this.deserializeNode(),
    };
  },

  // 278: BabelFile
  function () {
    return {
      type: 'File',
      loc: this.addEmptyLoc(),
      program: this.deserializeNode(),
      comments: this.deserializeComments(),
    };
  },

  // 279: BabelGenericTypeAnnotation
  function () {
    return {
      type: 'GenericTypeAnnotation',
      loc: this.addEmptyLoc(),
      id: this.deserializeNode(),
      typeParameters: this.deserializeNode(),
    };
  },

  // 280: BabelInterfaceExtends
  function () {
    return {
      type: 'InterfaceExtends',
      loc: this.addEmptyLoc(),
      id: this.deserializeNode(),
      typeParameters: this.deserializeNode(),
    };
  },

  // 281: BabelClassImplements
  function () {
    return {
      type: 'ClassImplements',
      loc: this.addEmptyLoc(),
      id: this.deserializeNode(),
      typeParameters: this.deserializeNode(),
    };
  },
];
