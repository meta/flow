
import type { ESNode } from "./ast";

import * as Ast from "./ast";

export class Visitor {
    optional<N extends ESNode>(method: (node: N) => void, node: ?N) {
      if (node != null) {
        method(node);
      }
    }

    readonly block = (node: Ast.BlockStatement | Ast.StaticBlock): void => {
      node.body.forEach(stmt => this.statement(stmt));
    };

    readonly statement = (stmt: Ast.Statement | Ast.ModuleDeclaration): void => {
      match (stmt) {
        {type: 'BlockStatement', ...} as stmt => {
          this.block(stmt);
        }
        {type: 'StaticBlock', ...} as stmt => {
          this.block(stmt);
        }
        {type: 'ExpressionStatement', ...} as stmt => {
          this.expressionStatement(stmt);
        }
        {type: 'IfStatement', ...} as stmt => {
          this.ifStatement(stmt);
        }
        {type: 'ReturnStatement', ...} as stmt => {
          this.returnStatement(stmt);
        }
        {type: 'VariableDeclaration', ...} as stmt => {
          this.variableDeclaration(stmt);
        }
        {type: 'FunctionDeclaration', ...} as stmt => {
          this.functionDeclaration(stmt);
        }
        {type: 'ForStatement', ...} as stmt => {
          this.forStatement(stmt);
        }
        {type: 'WhileStatement', ...} as stmt => {
          this.whileStatement(stmt);
        }
        {type: 'SwitchStatement', ...} as stmt => {
          this.switchStatement(stmt);
        }
        {type: 'TryStatement', ...} as stmt => {
          this.tryStatement(stmt);
        }
        {type: 'ThrowStatement', ...} as stmt => {
          this.throwStatement(stmt);
        }
        {type: 'BreakStatement', ...} as stmt => {
          this.breakStatement(stmt);
        }
        {type: 'ContinueStatement', ...} as stmt => {
          this.continueStatement(stmt);
        }
        {type: 'LabeledStatement', ...} as stmt => {
          this.labeledStatement(stmt);
        }
        {type: 'WithStatement', ...} as stmt => {
          this.withStatement(stmt);
        }
        {type: 'DoWhileStatement', ...} as stmt => {
          this.doWhileStatement(stmt);
        }
        {type: 'ForInStatement', ...} as stmt => {
          this.forInStatement(stmt);
        }
        {type: 'ForOfStatement', ...} as stmt => {
          this.forOfStatement(stmt);
        }
        {type: 'DebuggerStatement', ...} as stmt => {
          this.debuggerStatement(stmt);
        }
        {type: 'EmptyStatement', ...} as stmt => {
          this.emptyStatement(stmt);
        }
        {type: 'ClassDeclaration', ...} as stmt => {
          this.classDeclaration(stmt);
        }
        {type: 'ComponentDeclaration', ...} as stmt => {
          this.componentDeclaration(stmt);
        }
        {type: 'HookDeclaration', ...} as stmt => {
          this.hookDeclaration(stmt);
        }
        {type: 'EnumDeclaration', ...} as stmt => {
          this.enumDeclaration(stmt);
        }
        {type: 'InterfaceDeclaration', ...} as stmt => {
          this.interfaceDeclaration(stmt);
        }
        {type: 'TypeAlias', ...} as stmt => {
          this.typeAlias(stmt);
        }
        {type: 'OpaqueType', ...} as stmt => {
          this.opaqueType(stmt);
        }
        {type: 'DeclareClass', ...} as stmt => {
          this.declareClass(stmt);
        }
        {type: 'DeclareComponent', ...} as stmt => {
          this.declareComponent(stmt);
        }
        {type: 'DeclareHook', ...} as stmt => {
          this.declareHook(stmt);
        }
        {type: 'DeclareVariable', ...} as stmt => {
          this.declareVariable(stmt);
        }
        {type: 'DeclareEnum', ...} as stmt => {
          this.declareEnum(stmt);
        }
        {type: 'DeclareFunction', ...} as stmt => {
          this.declareFunction(stmt);
        }
        {type: 'DeclareModule', ...} as stmt => {
          this.declareModule(stmt);
        }
        {type: 'DeclareNamespace', ...} as stmt => {
          this.declareNamespace(stmt);
        }
        {type: 'DeclareInterface', ...} as stmt => {
          this.declareInterface(stmt);
        }
        {type: 'DeclareTypeAlias', ...} as stmt => {
          this.declareTypeAlias(stmt);
        }
        {type: 'DeclareOpaqueType', ...} as stmt => {
          this.declareOpaqueType(stmt);
        }
        {type: 'DeclareExportDeclaration', ...} as stmt => {
          this.declareExportDeclaration(stmt);
        }
        {type: 'DeclareExportAllDeclaration', ...} as stmt => {
          this.declareExportAllDeclaration(stmt);
        }
        {type: 'DeclareModuleExports', ...} as stmt => {
          this.declareModuleExports(stmt);
        }
        {type: 'MatchStatement', ...} as stmt => {
          this.matchStatement(stmt);
        }
        {type: 'ImportDeclaration', ...} as stmt => {
          this.importDeclaration(stmt);
        }
        {type: 'ExportNamedDeclaration', ...} as stmt => {
          this.exportNamedDeclaration(stmt);
        }
        {type: 'ExportDefaultDeclaration', ...} as stmt => {
          this.exportDefaultDeclaration(stmt);
        }
        {type: 'ExportAllDeclaration', ...} as stmt => {
          this.exportAllDeclaration(stmt);
        }
      }
    };

    importDeclaration(stmt: Ast.ImportDeclaration): void {
      // $FlowFixMe[incompatible-type] TODO
      // $FlowFixMe[prop-missing] TODO
      stmt.specifiers.forEach(specifier => this.importSpecifier(specifier));
      this.expression(stmt.source);
      stmt.assertions.forEach(assertion => this.importAttribute(assertion));
    }

    exportNamedDeclaration(stmt: Ast.ExportNamedDeclaration): void {
      if (stmt.declaration) {
        this.statement(stmt.declaration);
      }
      stmt.specifiers.forEach(specifier => this.exportSpecifier(specifier));
      this.optional(this.expression, stmt.source);
    }

    exportDefaultDeclaration(stmt: Ast.ExportDefaultDeclaration): void {
      // $FlowFixMe[incompatible-type] TODO
      this.statement(stmt.declaration);
    }

    exportAllDeclaration(stmt: Ast.ExportAllDeclaration): void {
      this.expression(stmt.source);
      this.optional(this.identifier, stmt.exported);
    }

    importSpecifier(specifier: Ast.ImportSpecifier): void {
      this.identifier(specifier.imported);
      this.identifier(specifier.local);
    }

    exportSpecifier(specifier: Ast.ExportSpecifier): void {
      this.identifier(specifier.exported);
      this.identifier(specifier.local);
    }
    readonly expression = (expr: Ast.Expression): void => {
      match (expr) {
        {type: 'ThisExpression', ...} as expr => {
          this.thisExpression(expr);
        }
        {type: 'ArrayExpression', ...} as expr => {
          this.arrayExpression(expr);
        }
        {type: 'ObjectExpression', ...} as expr => {
          this.objectExpression(expr);
        }
        {type: 'FunctionExpression', ...} as expr => {
          this.functionExpression(expr);
        }
        {type: 'ArrowFunctionExpression', ...} as expr => {
          this.arrowFunctionExpression(expr);
        }
        {type: 'YieldExpression', ...} as expr => {
          this.yieldExpression(expr);
        }
        {type: 'Literal', ...} as expr => {
          this.literal(expr);
        }
        {type: 'UnaryExpression', ...} as expr => {
          this.unaryExpression(expr);
        }
        {type: 'UpdateExpression', ...} as expr => {
          this.updateExpression(expr);
        }
        {type: 'BinaryExpression', ...} as expr => {
          this.binaryExpression(expr);
        }
        {type: 'AssignmentExpression', ...} as expr => {
          this.assignmentExpression(expr);
        }
        {type: 'LogicalExpression', ...} as expr => {
          this.logicalExpression(expr);
        }
        {type: 'MemberExpression', ...} as expr => {
          this.memberExpression(expr);
        }
        {type: 'ConditionalExpression', ...} as expr => {
          this.conditionalExpression(expr);
        }
        {type: 'CallExpression', ...} as expr => {
          this.callExpression(expr);
        }
        {type: 'NewExpression', ...} as expr => {
          this.newExpression(expr);
        }
        {type: 'SequenceExpression', ...} as expr => {
          this.sequenceExpression(expr);
        }
        {type: 'TemplateLiteral', ...} as expr => {
          this.templateLiteral(expr);
        }
        {type: 'TaggedTemplateExpression', ...} as expr => {
          this.taggedTemplateExpression(expr);
        }
        {type: 'ClassExpression', ...} as expr => {
          this.classExpression(expr);
        }
        {type: 'MetaProperty', ...} as expr => {
          this.metaProperty(expr);
        }
        {type: 'Identifier', ...} as expr => {
          this.identifier(expr);
        }
        {type: 'AwaitExpression', ...} as expr => {
          this.awaitExpression(expr);
        }
        {type: 'ImportExpression', ...} as expr => {
          this.importExpression(expr);
        }
        {type: 'ChainExpression', ...} as expr => {
          this.chainExpression(expr);
        }
        {type: 'TypeCastExpression', ...} as expr => {
          this.typeCastExpression(expr);
        }
        {type: 'AsExpression', ...} as expr => {
          this.asExpression(expr);
        }
        {type: 'AsConstExpression', ...} as expr => {
          this.asConstExpression(expr);
        }
        {type: 'MatchExpression', ...} as expr => {
          this.matchExpression(expr);
        }
        {type: 'JSXFragment', ...} as expr => {
          this.jsxFragment(expr);
        }
        {type: 'JSXElement', ...} as expr => {
          this.jsxElement(expr);
        }
      }
    };

    hole(): void {}

    arrayElement(element: Ast.SpreadElement | Ast.Expression | void): void {
      match (element) {
        undefined => {
          this.hole();
        }
        {type: 'SpreadElement', ...} as element => {
          this.spreadElement(element);
        }
        _ as element => {
          this.expression(element);
        }
      }
    }

    thisExpression(expr: Ast.ThisExpression): void {}

    arrayExpression(expr: Ast.ArrayExpression): void {
      console.log('ArrayExpression elements:', expr.elements);
      expr.elements.forEach(element => {
        this.arrayElement(element);
      });
    }

    objectMember(prop: Ast.ObjectProperty | Ast.SpreadElement): void {
      if (prop.type === 'Property') {
        this.objectMember(prop);
      } else if (prop.type === 'SpreadElement') {
        this.spreadElement(prop);
      }
    }

    objectProperty(prop: Ast.ObjectProperty): void {
      this.expression(prop.key);
      this.expression(prop.value);
    }

    objectExpression(expr: Ast.ObjectExpression): void {
      expr.properties.forEach(prop => {
        this.objectMember(prop);
      });
    }

    functionExpression(expr: Ast.FunctionExpression): void {
      console.log('FunctionExpression id:', expr.id);
      expr.params.forEach(param => console.log('Function parameter:', param));
      this.block(expr.body);
    }

    arrowFunctionExpression(expr: Ast.ArrowFunctionExpression): void {
      this.lambda(expr);
    }

    lambda(func: Ast.AFunction | Ast.HookDeclaration): void {
      this.optional(this.bindingIdentifier, func.id);
      this.optional(this.typeParameters, func.typeParameters);
      this.optional(this.typeAnnotation, func.returnType?.typeAnnotation);
      func.params.forEach(param => this.functionParameter(param));
      if (func.body.type === 'BlockStatement') {
        this.block(func.body);
      } else {
        this.expression(func.body);
      }
    }

    functionParameter(param: Ast.FunctionParameter): void {
      if (param.type === 'AssignmentPattern') {
        this.assignmentPattern(param);
      } else if (param.type === 'RestElement') {
        this.restElement(param);
      } else {
        this.bindingPattern(param);
      }
    }

    yieldExpression(expr: Ast.YieldExpression): void {
      this.optional(this.expression, expr.argument);
    }

    literal(expr: Ast.Literal): void {}

    unaryExpression(expr: Ast.UnaryExpression): void {
      this.expression(expr.argument);
    }

    updateExpression(expr: Ast.UpdateExpression): void {
      this.expression(expr.argument);
    }

    privateIdentifier(expr: Ast.PrivateIdentifier): void {}

    binaryExpression(expr: Ast.BinaryExpression): void {
      if (expr.left.type === 'PrivateIdentifier') {
        this.privateIdentifier(expr.left);
      } else {
        this.expression(expr.left);
      }
      this.expression(expr.right);
    }

    readonly bindingIdentifier = (id: Ast.Identifier) => {};

    bindingMemberExpression(expr: Ast.MemberExpression): void {
      this.memberExpression(expr);
    }

    readonly bindingPattern = (pat: Ast.BindingName): void => {
      this.optional(this.typeAnnotation, pat.typeAnnotation?.typeAnnotation);
      match (pat) {
        {type: 'Identifier', ...} as pat => {
          this.bindingIdentifier(pat);
        }
        {type: 'ArrayPattern', ...} as pat => {
          this.arrayPattern(pat);
        }
        {type: 'ObjectPattern', ...} as pat => {
          this.objectPattern(pat);
        }
      }
    };

    arrayPattern(pat: Ast.ArrayPattern): void {
      pat.elements.forEach(element => {
        if (element != null) {
          this.destructuringPattern(element);
        }
      });
    }

    destructuringPattern(pat: Ast.DestructuringPattern): void {
      if (pat.type === 'AssignmentPattern') {
        this.assignmentPattern(pat);
      } else if (pat.type === 'MemberExpression') {
        this.bindingMemberExpression(pat);
      } else if (pat.type === 'RestElement') {
        this.restElement(pat);
      } else {
        this.bindingPattern(pat);
      }
    }

    destructuringProperty(prop: Ast.DestructuringObjectProperty) {
      this.expression(prop.key);
      this.destructuringPattern(prop.value);
    }

    objectPattern(pat: Ast.ObjectPattern): void {
      pat.properties.forEach(prop => {
        if (prop.type === 'Property') {
          this.destructuringProperty(prop);
        } else if (prop.type === 'RestElement') {
          this.restElement(prop);
        }
      });
    }

    assignmentPattern(expr: Ast.AssignmentPattern): void {
      this.bindingPattern(expr.left);
      this.expression(expr.right);
    }

    assignmentExpression(expr: Ast.AssignmentExpression): void {
      if (expr.left.type === 'MemberExpression') {
        this.bindingMemberExpression(expr.left);
      } else {
        this.bindingPattern(expr.left);
      }
      this.expression(expr.right);
    }

    logicalExpression(expr: Ast.LogicalExpression): void {
      this.expression(expr.left);
      this.expression(expr.right);
    }

    super(sup: Ast.Super): void {}

    memberExpression(expr: Ast.MemberExpression): void {
      if (expr.object.type === 'Super') {
        this.super(expr.object);
      } else {
        this.expression(expr.object);
      }
      if (expr.property.type === 'PrivateIdentifier') {
        this.privateIdentifier(expr.property);
      } else {
        this.expression(expr.property);
      }
    }

    conditionalExpression(expr: Ast.ConditionalExpression): void {
      this.expression(expr.test);
      this.expression(expr.consequent);
      this.expression(expr.alternate);
    }

    argument(expr: Ast.Expression | Ast.SpreadElement): void {
      match (expr) {
        {type: 'SpreadElement', ...} as expr => {
          this.spreadElement(expr);
        }
        _ as expr => {
          this.expression(expr);
        }
      }
    }

    spreadElement(expr: Ast.SpreadElement): void {
      this.expression(expr.argument);
    }

    callExpression(expr: Ast.CallExpression): void {
      this.baseCallExpression(expr);
    }

    newExpression(expr: Ast.NewExpression): void {
      this.baseCallExpression(expr);
    }

    baseCallExpression(expr: Ast.NewExpression | Ast.CallExpression): void {
      if (expr.callee.type === 'Super') {
        this.super(expr.callee);
      } else {
        this.expression(expr.callee);
      }
      this.optional(this.typeArguments, expr.typeArguments);
      expr.arguments.forEach(arg => this.argument(arg));
    }

    sequenceExpression(expr: Ast.SequenceExpression): void {
      expr.expressions.forEach(exp => this.expression(exp));
    }

    templateLiteral(expr: Ast.TemplateLiteral): void {
      expr.expressions.forEach(exp => this.expression(exp));
    }

    taggedTemplateExpression(expr: Ast.TaggedTemplateExpression): void {
      this.expression(expr.tag);
      this.templateLiteral(expr.quasi);
    }

    classExpression(expr: Ast.ClassExpression): void {
      this.class(expr);
    }

    typeIdentifier(expr: Ast.Identifier): void {}

    class(expr: Ast.ClassExpression | Ast.ClassDeclaration): void {
      this.optional(this.bindingIdentifier, expr.id);
      this.optional(this.typeParameters, expr.typeParameters);
      this.optional(this.expression, expr.superClass);
      expr.superTypeParameters?.params.forEach(param =>
        this.typeAnnotation(param),
      );
      expr.implements?.forEach(impl => {
        this.typeIdentifier(impl.id);
        impl.typeParameters?.params.forEach(param => this.typeAnnotation(param));
      });
      expr.body.body.forEach(member => this.classMember(member));
    }

    classMember(member: Ast.ClassMember): void {
      match (member) {
        {type: 'MethodDefinition', ...} as method => {
          this.methodDefinition(method);
        }
        {type: 'PropertyDefinition', ...} as property => {
          this.propertyDefinition(property);
        }
        {type: 'StaticBlock', ...} as block => {
          this.block(block);
        }
      }
    }

    classKey(
      key: Ast.ClassPropertyNameNonComputed | Ast.ClassPropertyNameComputed,
    ): void {
      if (key.type === 'PrivateIdentifier') {
        this.privateIdentifier(key);
      } else if (key.type === 'Identifier') {
        this.bindingIdentifier(key);
      } else {
        this.expression(key);
      }
    }

    methodDefinition(method: Ast.MethodDefinition): void {
      this.classKey(method.key);
      this.lambda(method.value);
    }

    propertyDefinition(property: Ast.PropertyDefinition): void {
      this.classKey(property.key);
      this.optional(this.typeAnnotation, property.typeAnnotation?.typeAnnotation);
      this.optional(this.expression, property.value);
    }

    metaProperty(expr: Ast.MetaProperty): void {
      this.identifier(expr.meta);
    }

    readonly identifier = (expr: Ast.Identifier): void => {};

    awaitExpression(expr: Ast.AwaitExpression): void {
      this.expression(expr.argument);
    }

    importExpression(expr: Ast.ImportExpression): void {
      this.expression(expr.source);
      expr.attributes?.forEach(attr => this.importAttribute(attr));
    }

    importAttribute(attr: Ast.ImportAttribute): void {
      this.identifier(attr.key);
    }

    chainExpression(expr: Ast.ChainExpression): void {
      this.expression(expr.expression);
    }

    typeCastExpression(expr: Ast.TypeCastExpression): void {
      this.expression(expr.expression);
      this.typeAnnotation(expr.typeAnnotation.typeAnnotation);
    }

    asExpression(expr: Ast.AsExpression): void {
      this.expression(expr.expression);
      this.typeAnnotation(expr.typeAnnotation);
    }

    asConstExpression(expr: Ast.AsConstExpression): void {
      this.expression(expr.expression);
    }

    matchExpression(expr: Ast.MatchExpression): void {
      this.expression(expr.argument);
      expr.cases.forEach(caseExpr => {
        this.matchPattern(caseExpr.pattern);
        this.optional(this.expression, caseExpr.guard);
        this.expression(caseExpr.body);
      });
    }

    matchPattern(pattern: Ast.MatchPattern): void {
      match (pattern) {
        {type: 'MatchOrPattern', ...} as pat => {
          this.matchOrPattern(pat);
        }
        {type: 'MatchAsPattern', ...} as pat => {
          this.matchAsPattern(pat);
        }
        {type: 'MatchWildcardPattern', ...} as pat => {
          this.matchWildcardPattern(pat);
        }
        {type: 'MatchLiteralPattern', ...} as pat => {
          this.matchLiteralPattern(pat);
        }
        {type: 'MatchUnaryPattern', ...} as pat => {
          this.matchUnaryPattern(pat);
        }
        {type: 'MatchIdentifierPattern', ...} as pat => {
          this.matchIdentifierPattern(pat);
        }
        {type: 'MatchMemberPattern', ...} as pat => {
          this.matchMemberPattern(pat);
        }
        {type: 'MatchBindingPattern', ...} as pat => {
          this.matchBindingPattern(pat);
        }
        {type: 'MatchObjectPattern', ...} as pat => {
          this.matchObjectPattern(pat);
        }
        {type: 'MatchArrayPattern', ...} as pat => {
          this.matchArrayPattern(pat);
        }
      }
    }

    matchOrPattern(pat: Ast.MatchOrPattern): void {
      pat.patterns.forEach(subPattern => this.matchPattern(subPattern));
    }

    matchAsPattern(pat: Ast.MatchAsPattern): void {
      this.matchPattern(pat.pattern);
      if (pat.target.type === 'Identifier') {
        this.bindingIdentifier(pat.target);
      } else {
        this.matchBindingPattern(pat.target);
      }
    }

    matchWildcardPattern(pat: Ast.MatchWildcardPattern): void {}

    matchLiteralPattern(pat: Ast.MatchLiteralPattern): void {}

    matchUnaryPattern(pat: Ast.MatchUnaryPattern): void {}

    matchIdentifierPattern(pat: Ast.MatchIdentifierPattern): void {
      this.identifier(pat.id);
    }

    matchMemberPattern(pat: Ast.MatchMemberPattern): void {
      this.matchPattern(pat.base);
      this.expression(pat.property);
    }

    readonly matchBindingPattern = (pat: Ast.MatchBindingPattern) => {
      this.bindingIdentifier(pat.id);
    };

    matchObjectPattern(pat: Ast.MatchObjectPattern): void {
      pat.properties.forEach(prop => {
        this.matchObjectPatternProperty(prop);
      });
      this.optional(this.matchRestPattern, pat.rest);
    }

    matchObjectPatternProperty(prop: Ast.MatchObjectPatternProperty): void {
      this.expression(prop.key);
      this.matchPattern(prop.pattern);
    }

    matchArrayPattern(pat: Ast.MatchArrayPattern): void {
      pat.elements.forEach(element => this.matchPattern(element));
      this.optional(this.matchRestPattern, pat.rest);
    }

    readonly matchRestPattern = (pat: Ast.MatchRestPattern): void => {
      this.optional(this.matchBindingPattern, pat.argument);
    };

    jsxFragment(expr: Ast.JSXFragment): void {
      expr.children.forEach(child => this.jsxChild(child));
    }

    jsxChild(child: Ast.JSXChild): void {
      match (child) {
        {type: 'JSXElement', ...} as jsxElement => {
          this.jsxElement(jsxElement);
        }
        {type: 'JSXFragment', ...} as jsxFragment => {
          this.jsxFragment(jsxFragment);
        }
        {type: 'JSXText', ...} as jsxText => {
          this.jsxText(jsxText);
        }
        {type: 'JSXSpreadChild', ...} as jsxSpreadChild => {
          this.jsxSpreadChild(jsxSpreadChild);
        }
        {type: 'JSXEmptyExpression', ...} as jsxEmptyExpression => {
          this.jsxEmptyExpression(jsxEmptyExpression);
        }
        {type: 'JSXExpressionContainer', ...} as jsxExpressionContainer => {
          this.jsxExpressionContainer(jsxExpressionContainer);
        }
      }
    }

    jsxExpression(expr: Ast.JSXExpression): void {
      if (expr.type === 'JSXEmptyExpression') {
        this.jsxEmptyExpression(expr);
      } else {
        this.jsxExpressionContainer(expr);
      }
    }

    jsxEmptyExpression(expr: Ast.JSXEmptyExpression): void {}

    jsxExpressionContainer(expr: Ast.JSXExpressionContainer): void {
      if (expr.expression.type === 'JSXEmptyExpression') {
        this.jsxEmptyExpression(expr.expression);
      } else {
        this.expression(expr.expression);
      }
    }

    jsxText(expr: Ast.JSXText): void {}

    jsxSpreadChild(expr: Ast.JSXSpreadChild): void {
      this.expression(expr.expression);
    }

    jsxElement(expr: Ast.JSXElement): void {
      this.jsxTag(expr.openingElement.name);
      expr.openingElement.attributes.forEach(attr => this.jsxMember(attr));
      expr.children.forEach(child => this.jsxChild(child));
    }

    jsxTag(tag: Ast.JSXTagNameExpression): void {
      if (tag.type === 'JSXIdentifier') {
        this.jsxIdentifier(tag);
      } else if (tag.type === 'JSXMemberExpression') {
        this.jsxMemberExpression(tag);
      } else if (tag.type === 'JSXNamespacedName') {
        this.jsxNamespacedName(tag);
      }
    }

    jsxMember(member: Ast.JSXAttribute | Ast.JSXSpreadAttribute): void {
      if (member.type === 'JSXAttribute') {
        this.jsxAttribute(member);
      } else if (member.type === 'JSXSpreadAttribute') {
        this.jsxSpreadAttribute(member);
      }
    }

    jsxIdentifier(tag: Ast.JSXIdentifier): void {}

    jsxMemberExpression(expr: Ast.JSXMemberExpression): void {
      this.jsxTag(expr.object);
      this.jsxIdentifier(expr.property);
    }

    jsxNamespacedName(name: Ast.JSXNamespacedName): void {
      this.jsxIdentifier(name.namespace);
      this.jsxIdentifier(name.name);
    }

    jsxAttribute(attr: Ast.JSXAttribute): void {
      this.jsxIdentifier(attr.name);
      if (attr.value != null) {
        if (attr.value.type === 'Literal') {
          this.literal(attr.value);
        } else {
          this.jsxExpression(attr.value);
        }
      }
    }

    jsxSpreadAttribute(attr: Ast.JSXSpreadAttribute): void {
      this.expression(attr.argument);
    }

    expressionStatement(stmt: Ast.ExpressionStatement): void {
      this.expression(stmt.expression);
    }

    ifStatement(stmt: Ast.IfStatement): void {
      this.statement(stmt.consequent);
      this.optional(this.statement, stmt.alternate);
    }

    returnStatement(stmt: Ast.ReturnStatement): void {
      this.optional(this.expression, stmt.argument);
    }

    variableDeclarator(decl: Ast.VariableDeclarator): void {
      this.bindingPattern(decl.id);
      this.optional(this.expression, decl.init);
    }

    variableDeclaration(stmt: Ast.VariableDeclaration): void {
      stmt.declarations.forEach(decl => {
        this.variableDeclarator(decl);
      });
    }

    functionDeclaration(stmt: Ast.FunctionDeclaration): void {
      this.lambda(stmt);
    }

    forStatement(stmt: Ast.ForStatement): void {
      if (stmt.init != null) {
        if (stmt.init.type === 'VariableDeclaration') {
          this.variableDeclaration(stmt.init);
        } else {
          this.expression(stmt.init);
        }
      }
      this.optional(this.expression, stmt.test);
      this.optional(this.expression, stmt.update);
      this.statement(stmt.body);
    }

    whileStatement(stmt: Ast.WhileStatement): void {
      this.expression(stmt.test);
      this.statement(stmt.body);
    }

    switchStatement(stmt: Ast.SwitchStatement): void {
      this.expression(stmt.discriminant);
      stmt.cases.forEach(caseStmt => {
        this.optional(this.expression, caseStmt.test);
        caseStmt.consequent.forEach(conseq => this.statement(conseq));
      });
    }

    tryStatement(stmt: Ast.TryStatement): void {
      this.block(stmt.block);
      if (stmt.handler) {
        const handler = stmt.handler;
        this.optional(this.bindingPattern, handler.param);
        this.block(handler.body);
      }
      this.optional(this.block, stmt.finalizer);
    }

    throwStatement(stmt: Ast.ThrowStatement): void {
      this.expression(stmt.argument);
    }

    readonly label = (id: Ast.Identifier): void => {};

    breakStatement(stmt: Ast.BreakStatement): void {
      this.optional(this.label, stmt.label);
    }

    continueStatement(stmt: Ast.ContinueStatement): void {
      this.optional(this.label, stmt.label);
    }

    labeledStatement(stmt: Ast.LabeledStatement): void {
      this.label(stmt.label);
      this.statement(stmt.body);
    }

    withStatement(stmt: Ast.WithStatement): void {
      this.expression(stmt.object);
      this.statement(stmt.body);
    }

    doWhileStatement(stmt: Ast.DoWhileStatement): void {
      this.statement(stmt.body);
      this.expression(stmt.test);
    }

    forInStatement(stmt: Ast.ForInStatement): void {
      if (stmt.left.type === 'VariableDeclaration') {
        this.variableDeclaration(stmt.left);
      } else if (stmt.left.type === 'MemberExpression') {
        this.bindingMemberExpression(stmt.left);
      } else {
        this.bindingPattern(stmt.left);
      }
      this.expression(stmt.right);
      this.statement(stmt.body);
    }

    forOfStatement(stmt: Ast.ForOfStatement): void {
      if (stmt.left.type === 'VariableDeclaration') {
        this.variableDeclaration(stmt.left);
      } else if (stmt.left.type === 'MemberExpression') {
        this.bindingMemberExpression(stmt.left);
      } else {
        this.bindingPattern(stmt.left);
      }
      this.expression(stmt.right);
      this.statement(stmt.body);
    }

    debuggerStatement(stmt: Ast.DebuggerStatement): void {}

    emptyStatement(stmt: Ast.EmptyStatement): void {}

    classDeclaration(stmt: Ast.ClassDeclaration): void {
      this.class(stmt);
    }

    componentDeclaration(stmt: Ast.ComponentDeclaration): void {
      this.bindingIdentifier(stmt.id);
      this.optional(this.typeParameters, stmt.typeParameters);
      stmt.params.forEach(param => {
        if (param.type === 'RestElement') {
          this.restElement(param);
        } else {
          this.componentParameter(param);
        }
      });
      this.optional(this.rendersType, stmt.rendersType);
      this.block(stmt.body);
    }

    restElement(param: Ast.RestElement): void {
      if (param.argument.type === 'AssignmentPattern') {
        this.assignmentPattern(param.argument);
      } else if (param.argument.type === 'RestElement') {
        this.restElement(param.argument);
      } else {
        this.bindingPattern(param.argument);
      }
    }

    componentParameter(param: Ast.ComponentParameter): void {
      if (param.local.type === 'AssignmentPattern') {
        this.assignmentPattern(param.local);
      } else {
        this.bindingPattern(param.local);
      }
    }

    hookDeclaration(stmt: Ast.HookDeclaration): void {
      this.lambda(stmt);
    }

    enumDeclaration(stmt: Ast.EnumDeclaration | Ast.DeclareEnum): void {
      this.bindingIdentifier(stmt.id);
    }

    interfaceDeclaration(
      stmt: Ast.InterfaceDeclaration | Ast.DeclareInterface,
    ): void {
      this.bindingIdentifier(stmt.id);
      this.objectTypeAnnotation(stmt.body);
    }

    bindingTypeIdentifier(id: Ast.Identifier): void {
      this.bindingIdentifier(id);
    }

    typeAlias(stmt: Ast.TypeAlias | Ast.DeclareTypeAlias): void {
      this.bindingTypeIdentifier(stmt.id);
      this.optional(this.typeParameters, stmt.typeParameters);
      this.typeAnnotation(stmt.right);
    }

    opaqueType(stmt: Ast.OpaqueType): void {
      this.bindingTypeIdentifier(stmt.id);
      this.optional(this.typeParameters, stmt.typeParameters);
      this.optional(this.typeAnnotation, stmt.supertype);
      this.typeAnnotation(stmt.impltype);
    }

    qualifiedTypeIdentifier(
      id: Ast.Identifier | Ast.QualifiedTypeIdentifier,
    ): void {
      if (id.type === 'Identifier') {
        this.typeIdentifier(id);
      } else {
        this.qualifiedTypeIdentifier(id.qualification);
        this.typeIdentifier(id.id);
      }
    }

    declareClass(stmt: Ast.DeclareClass): void {
      this.bindingIdentifier(stmt.id);
      this.optional(this.typeParameters, stmt.typeParameters);
      stmt.extends?.forEach(ext => {
        this.qualifiedTypeIdentifier(ext.id);
        this.optional(this.typeArguments, ext.typeParameters);
      });
      stmt.implements?.forEach(impl => {
        this.typeIdentifier(impl.id);
        this.optional(this.typeArguments, impl.typeParameters);
      });
      stmt.mixins?.forEach(ext => {
        this.qualifiedTypeIdentifier(ext.id);
        this.optional(this.typeArguments, ext.typeParameters);
      });
      this.objectTypeAnnotation(stmt.body);
    }

    declareComponent(stmt: Ast.DeclareComponent): void {
      this.bindingIdentifier(stmt.id);
      this.optional(this.typeParameters, stmt.typeParameters);
      stmt.params.forEach(param => {
        if (param.name?.type === 'Identifier') {
          this.bindingIdentifier(param.name);
        }
      });
      this.optional(this.rendersType, stmt.rendersType);
    }

    declareLambda(stmt: Ast.DeclareFunction | Ast.DeclareHook): void {
      this.bindingIdentifier(stmt.id);
      this.typeAnnotation(stmt.id.typeAnnotation.typeAnnotation);
    }

    declareHook(stmt: Ast.DeclareHook): void {
      this.declareLambda(stmt);
    }

    declareVariable(stmt: Ast.DeclareVariable): void {
      this.bindingIdentifier(stmt.id);
      this.optional(this.typeAnnotation, stmt.id.typeAnnotation?.typeAnnotation);
    }

    declareEnum(stmt: Ast.DeclareEnum): void {
      this.enumDeclaration(stmt);
    }

    declareFunction(stmt: Ast.DeclareFunction): void {
      this.declareLambda(stmt);
    }

    declareModule(stmt: Ast.DeclareModule): void {
      if (stmt.id.type === 'Identifier') {
        this.bindingIdentifier(stmt.id);
      }
      this.block(stmt.body);
    }

    declareNamespace(stmt: Ast.DeclareNamespace): void {
      if (stmt.id.type === 'Identifier') {
        this.bindingIdentifier(stmt.id);
      }
      this.block(stmt.body);
    }

    declareInterface(stmt: Ast.DeclareInterface): void {
      this.interfaceDeclaration(stmt);
    }

    declareTypeAlias(stmt: Ast.DeclareTypeAlias): void {
      this.typeAlias(stmt);
    }

    declareOpaqueType(stmt: Ast.DeclareOpaqueType): void {
      this.bindingTypeIdentifier(stmt.id);
      this.optional(this.typeParameters, stmt.typeParameters);
      this.optional(this.typeAnnotation, stmt.supertype);
      this.optional(this.typeAnnotation, stmt.impltype);
    }

    declareExportDeclaration(stmt: Ast.DeclareExportDeclaration): void {
      if (stmt.declaration !== null) {
        match (stmt.declaration) {
          {type: 'DeclareVariable', ...} as type => {
            this.declareVariable(type);
          }
          {type: 'DeclareFunction', ...} as type => {
            this.declareFunction(type);
          }
          {type: 'DeclareClass', ...} as type => {
            this.declareClass(type);
          }
          {type: 'DeclareComponent', ...} as type => {
            this.declareComponent(type);
          }
          {type: 'DeclareHook', ...} as type => {
            this.declareHook(type);
          }
          {type: 'DeclareInterface', ...} as type => {
            this.declareInterface(type);
          }
          {type: 'DeclareOpaqueType', ...} as type => {
            this.declareOpaqueType(type);
          }
          {type: 'DeclareTypeAlias', ...} as type => {
            this.declareTypeAlias(type);
          }
          {type: 'DeclareEnum', ...} as type => {
            this.declareEnum(type);
          }
          _ as type => {
            this.typeAnnotation(type);
          }
        }
      } else {
        stmt.specifiers.forEach(spec => {
          this.identifier(spec.local);
          this.identifier(spec.exported);
        });
      }
    }

    declareExportAllDeclaration(stmt: Ast.DeclareExportAllDeclaration): void {}

    declareModuleExports(stmt: Ast.DeclareModuleExports): void {
      this.typeAnnotation(stmt.typeAnnotation.typeAnnotation);
    }

    matchStatement(stmt: Ast.MatchStatement): void {
      this.expression(stmt.argument);
      stmt.cases.forEach(caseStmt => {
        this.matchPattern(caseStmt.pattern);
        this.optional(this.expression, caseStmt.guard);
        this.block(caseStmt.body);
      });
    }
    readonly typeAnnotation = (typeAnnotation: Ast.TypeAnnotationType): void => {
      match (typeAnnotation) {
        {type: 'NumberTypeAnnotation', ...} as type => {
        }
        {type: 'BigIntTypeAnnotation', ...} as type => {
        }
        {type: 'EmptyTypeAnnotation', ...} as type => {
        }
        {type: 'StringTypeAnnotation', ...} as type => {
        }
        {type: 'BooleanTypeAnnotation', ...} as type => {
        }
        {type: 'NumberLiteralTypeAnnotation', ...} as type => {
        }
        {type: 'BigIntLiteralTypeAnnotation', ...} as type => {
        }
        {type: 'StringLiteralTypeAnnotation', ...} as type => {
        }
        {type: 'BooleanLiteralTypeAnnotation', ...} as type => {
        }
        {type: 'NullLiteralTypeAnnotation', ...} as type => {
        }
        {type: 'AnyTypeAnnotation', ...} as type => {
        }
        {type: 'VoidTypeAnnotation', ...} as type => {
        }
        {type: 'SymbolTypeAnnotation', ...} as type => {
        }
        {type: 'ThisTypeAnnotation', ...} as type => {
        }
        {type: 'MixedTypeAnnotation', ...} as type => {
        }
        {type: 'GenericTypeAnnotation', ...} as type => {
          this.genericTypeAnnotation(type);
        }
        {type: 'NullableTypeAnnotation', ...} as type => {
          this.nullableTypeAnnotation(type);
        }
        {type: 'ExistsTypeAnnotation', ...} as type => {
          this.existsTypeAnnotation(type);
        }
        {type: 'QualifiedTypeIdentifier', ...} as type => {
          this.qualifiedTypeIdentifier(type);
        }
        {type: 'QualifiedTypeofIdentifier', ...} as type => {
          this.qualifiedTypeofIdentifier(type);
        }
        {type: 'TupleTypeSpreadElement', ...} as type => {
          this.tupleTypeSpreadElement(type);
        }
        {type: 'TupleTypeLabeledElement', ...} as type => {
          this.tupleTypeLabeledElement(type);
        }
        {type: 'HookTypeAnnotation', ...} as type => {
          this.hookTypeAnnotation(type);
        }
        {type: 'ComponentTypeAnnotation', ...} as type => {
          this.componentTypeAnnotation(type);
        }
        {type: 'ArrayTypeAnnotation', ...} as type => {
          this.arrayTypeAnnotation(type);
        }
        {type: 'ObjectTypeAnnotation', ...} as type => {
          this.objectTypeAnnotation(type);
        }
        {type: 'FunctionTypeAnnotation', ...} as type => {
          this.functionTypeAnnotation(type);
        }
        {type: 'UnionTypeAnnotation', ...} as type => {
          this.unionTypeAnnotation(type);
        }
        {type: 'IntersectionTypeAnnotation', ...} as type => {
          this.intersectionTypeAnnotation(type);
        }
        {type: 'TupleTypeAnnotation', ...} as type => {
          this.tupleTypeAnnotation(type);
        }
        {type: 'TypeofTypeAnnotation', ...} as type => {
          this.typeofTypeAnnotation(type);
        }
        {type: 'KeyofTypeAnnotation', ...} as type => {
          this.keyofTypeAnnotation(type);
        }
        {type: 'IndexedAccessType', ...} as type => {
          this.indexedAccessType(type);
        }
        {type: 'OptionalIndexedAccessType', ...} as type => {
          this.optionalIndexedAccessType(type);
        }
        {type: 'TypePredicate', ...} as type => {
          this.typePredicate(type);
        }
        {type: 'InferTypeAnnotation', ...} as type => {
          this.inferTypeAnnotation(type);
        }
        {type: 'ConditionalTypeAnnotation', ...} as type => {
          this.conditionalTypeAnnotation(type);
        }
        {type: 'InterfaceTypeAnnotation', ...} as type => {
          this.interfaceTypeAnnotation(type);
        }
        {type: 'TypeOperator', ...} as type => {
          this.typeOperator(type);
        }
      }
    };

    nullableTypeAnnotation(type: Ast.NullableTypeAnnotation): void {
      this.typeAnnotation(type.typeAnnotation);
    }

    existsTypeAnnotation(type: Ast.ExistsTypeAnnotation): void {}

    qualifiedTypeofIdentifier(
      id: Ast.Identifier | Ast.QualifiedTypeofIdentifier,
    ): void {
      if (id.type === 'Identifier') {
        this.identifier(id);
      } else {
        this.qualifiedTypeofIdentifier(id.qualification);
        this.typeIdentifier(id.id);
      }
    }

    tupleTypeSpreadElement(type: Ast.TupleTypeSpreadElement): void {
      this.optional(this.identifier, type.label);
      this.typeAnnotation(type.typeAnnotation);
    }

    tupleTypeLabeledElement(type: Ast.TupleTypeLabeledElement): void {
      this.identifier(type.label);
      this.typeAnnotation(type.elementType);
    }

    hookTypeAnnotation(type: Ast.HookTypeAnnotation): void {
      type.params.forEach(param => this.functionTypeParam(param));
      this.typeAnnotation(type.returnType);
      this.optional(this.functionTypeParam, type.rest);
      this.optional(this.typeParameters, type.typeParameters);
    }

    componentTypeAnnotation(type: Ast.ComponentTypeAnnotation): void {
      type.params.forEach(param => this.componentTypeParam(param));
      this.optional(this.componentTypeParam, type.rest);
      this.optional(this.typeParameters, type.typeParameters);
      this.optional(this.rendersType, type.rendersType);
    }

    genericTypeAnnotation(type: Ast.GenericTypeAnnotation): void {
      this.qualifiedTypeIdentifier(type.id);
      this.optional(this.typeArguments, type.typeParameters);
    }

    arrayTypeAnnotation(type: Ast.ArrayTypeAnnotation): void {
      this.typeAnnotation(type.elementType);
    }

    objectTypeAnnotation(type: Ast.ObjectTypeAnnotation): void {
      type.properties.forEach(prop => this.objectTypeMember(prop));
      type.indexers.forEach(indexer => this.objectTypeIndexer(indexer));
      type.callProperties.forEach(callProp =>
        this.objectTypeCallProperty(callProp),
      );
      type.internalSlots.forEach(slot => this.objectTypeInternalSlot(slot));
    }

    functionTypeAnnotation(type: Ast.FunctionTypeAnnotation): void {
      console.log('Handling FunctionTypeAnnotation');
      type.params.forEach(param => this.functionTypeParam(param));
      this.typeAnnotation(type.returnType);
      this.optional(this.functionTypeParam, type.rest);
      this.optional(this.typeParameters, type.typeParameters);
      this.optional(this.functionTypeParam, type.this);
    }

    unionTypeAnnotation(type: Ast.UnionTypeAnnotation): void {
      type.types.forEach(t => this.typeAnnotation(t));
    }

    intersectionTypeAnnotation(type: Ast.IntersectionTypeAnnotation): void {
      type.types.forEach(t => this.typeAnnotation(t));
    }

    tupleTypeAnnotation(type: Ast.TupleTypeAnnotation): void {
      type.types.forEach(t => this.typeAnnotation(t));
    }

    typeofTypeAnnotation(type: Ast.TypeofTypeAnnotation): void {
      this.qualifiedTypeofIdentifier(type.argument);
      this.optional(this.typeArguments, type.typeArguments);
    }

    keyofTypeAnnotation(type: Ast.KeyofTypeAnnotation): void {
      this.typeAnnotation(type.argument);
    }

    indexedAccessType(type: Ast.IndexedAccessType): void {
      this.typeAnnotation(type.objectType);
      this.typeAnnotation(type.indexType);
    }

    optionalIndexedAccessType(type: Ast.OptionalIndexedAccessType): void {
      this.typeAnnotation(type.objectType);
      this.typeAnnotation(type.indexType);
    }

    typePredicate(type: Ast.TypePredicate): void {
      this.identifier(type.parameterName);
      this.optional(this.typeAnnotation, type.typeAnnotation);
    }

    inferTypeAnnotation(type: Ast.InferTypeAnnotation): void {
      this.typeParameter(type.typeParameter);
    }

    conditionalTypeAnnotation(type: Ast.ConditionalTypeAnnotation): void {
      this.typeAnnotation(type.checkType);
      this.typeAnnotation(type.extendsType);
      this.typeAnnotation(type.trueType);
      this.typeAnnotation(type.falseType);
    }

    interfaceTypeAnnotation(type: Ast.InterfaceTypeAnnotation): void {
      this.objectTypeAnnotation(type.body);
      type.extends.forEach(ext => this.interfaceExtends(ext));
    }

    typeOperator(type: Ast.TypeOperator): void {
      this.typeAnnotation(type.typeAnnotation);
    }

    objectTypeMember(
      prop:
        | Ast.ObjectTypeProperty
        | Ast.ObjectTypeMappedTypeProperty
        | Ast.ObjectTypeSpreadProperty,
    ): void {
      match (prop) {
        {type: 'ObjectTypeMappedTypeProperty', ...} as type => {
          this.objectTypeMappedTypeProperty(type);
        }
        {type: 'ObjectTypeSpreadProperty', ...} as type => {
          this.objectTypeSpreadProperty(type);
        }
        {type: 'ObjectTypeProperty', ...} as type => {
          this.objectTypeProperty(type);
        }
      }
    }

    objectTypeMappedTypeProperty(type: Ast.ObjectTypeMappedTypeProperty): void {
      this.typeParameter(type.keyTparam);
      this.typeAnnotation(type.propType);
      this.typeAnnotation(type.sourceType);
    }

    objectTypeSpreadProperty(type: Ast.ObjectTypeSpreadProperty): void {
      this.typeAnnotation(type.argument);
    }

    objectTypeProperty(type: Ast.ObjectTypeProperty): void {
      this.expression(type.key);
      this.typeAnnotation(type.value);
    }

    objectTypeIndexer(indexer: Ast.ObjectTypeIndexer): void {
      this.optional(this.identifier, indexer.id);
      this.typeAnnotation(indexer.key);
      this.typeAnnotation(indexer.value);
    }

    objectTypeCallProperty(callProp: Ast.ObjectTypeCallProperty): void {
      this.functionTypeAnnotation(callProp.value);
    }

    objectTypeInternalSlot(slot: Ast.ObjectTypeInternalSlot): void {
      this.identifier(slot.id);
      this.typeAnnotation(slot.value.typeAnnotation);
    }

    readonly functionTypeParam = (param: Ast.FunctionTypeParam): void => {
      this.optional(this.bindingIdentifier, param.name);
      this.typeAnnotation(param.typeAnnotation);
    };

    readonly componentTypeParam = (param: Ast.ComponentTypeParameter): void => {
      if (param.name?.type === 'Identifier') {
        this.bindingIdentifier(param.name);
      }
      this.typeAnnotation(param.typeAnnotation);
    };

    interfaceExtends(ext: Ast.InterfaceExtends): void {
      this.qualifiedTypeIdentifier(ext.id);
      this.optional(this.typeArguments, ext.typeParameters);
    }

    readonly typeParameters = (params: Ast.TypeParameterDeclaration): void => {
      params.params.forEach(param => this.typeParameter(param));
    };

    readonly typeArguments = (params: Ast.TypeParameterInstantiation): void => {
      params.params.forEach(param => this.typeAnnotation(param));
    };

    typeParameter(param: Ast.TypeParameter): void {
      this.optional(this.typeAnnotation, param.bound?.typeAnnotation);
      this.optional(this.typeAnnotation, param.default);
    }

    readonly rendersType = (type: Ast.RendersType): void => {
      this.typeAnnotation(type.typeAnnotation);
    };
  }

  export class AccVisitor<Acc> extends Visitor {
    #acc: Acc;

    constructor(acc: Acc) {
      super();
      this.#acc = acc;
    }

    getAcc(): Acc {
      return this.#acc;
    }

    setAcc(acc: Acc): void {
      this.#acc = acc;
    }

    updateAcc(f: (acc: Acc) => Acc): void {
      this.setAcc(f(this.getAcc()));
    }

    eval(f: (visitor: this) => void): Acc {
      f(this);
      return this.getAcc();
    }
  }
