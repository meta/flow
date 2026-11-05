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
  ComponentTypeAnnotation,
  ComponentTypeParameter,
  DeclareClass,
  DeclareClassExtendsCall,
  DeclaredPredicate,
  DeclareExportDeclaration,
  DeclareComponent,
  DeclareFunction,
  DeclareHook,
  DeclareInterface,
  DeclareModule,
  DeclareModuleExports,
  DeclareNamespace,
  DeclareOpaqueType,
  DeclareTypeAlias,
  DeclareVariable,
  ESNode,
  FunctionTypeAnnotation,
  FunctionTypeParam,
  HookTypeAnnotation,
  GenericTypeAnnotation,
  Identifier,
  InterfaceDeclaration,
  ObjectTypeIndexer,
  ObjectTypeInternalSlot,
  ObjectTypePrivateField,
  ObjectTypeProperty,
  ObjectTypeMappedTypeProperty,
  OpaqueType,
  QualifiedTypeIdentifier,
  QualifiedTypeofIdentifier,
  TypeAlias,
  TupleTypeElement,
  TypeofTypeAnnotation,
  TypeParameter,
} from 'flow-estree';
import type {Referencer} from './Referencer';

import {Visitor} from './Visitor';
import {
  ComponentNameDefinition,
  ClassNameDefinition,
  FunctionNameDefinition,
  HookNameDefinition,
  NamespaceNameDefinition,
  TypeDefinition,
  TypeParameterDefinition,
  VariableDefinition,
} from '../definition';

class TypeVisitor extends Visitor {
  readonly _referencer: Referencer;

  constructor(referencer: Referencer) {
    super(referencer);
    this._referencer = referencer;
  }

  static visit(referencer: Referencer, node: ESNode): void {
    const typeReferencer = new TypeVisitor(referencer);
    typeReferencer.visit(node);
  }

  ///////////////////
  // Visit helpers //
  ///////////////////

  createTypeDefinition(
    node:
      | DeclareTypeAlias
      | DeclareOpaqueType
      | DeclareInterface
      | TypeAlias
      | OpaqueType
      | InterfaceDeclaration,
  ): void {
    this._referencer
      .currentScope()
      .defineIdentifier(node.id, new TypeDefinition(node.id, node));
  }

  maybeCreateTypeScope(
    node:
      | ComponentTypeAnnotation
      | DeclareTypeAlias
      | DeclareOpaqueType
      | DeclareInterface
      | DeclareClass
      | DeclareComponent
      | FunctionTypeAnnotation
      | HookTypeAnnotation
      | TypeAlias
      | OpaqueType
      | InterfaceDeclaration,
  ): boolean {
    if (
      node.typeParameters &&
      node.typeParameters.params &&
      node.typeParameters.params.length !== 0
    ) {
      this._referencer.scopeManager.nestTypeScope(node);
      return true;
    }

    return false;
  }

  visitTypeAlias(node: DeclareTypeAlias | TypeAlias): void {
    this.createTypeDefinition(node);

    const hasTypeScope = this.maybeCreateTypeScope(node);

    this.visit(node.typeParameters);
    this.visit(node.right);

    if (hasTypeScope) {
      this._referencer.close(node);
    }
  }

  visitOpaqueType(node: DeclareOpaqueType | OpaqueType): void {
    this.createTypeDefinition(node);

    const hasTypeScope = this.maybeCreateTypeScope(node);

    this.visit(node.typeParameters);
    this.visit(node.impltype);
    this.visit(node.supertype);

    if (hasTypeScope) {
      this._referencer.close(node);
    }
  }

  visitInterfaceDeclaration(
    node: DeclareInterface | InterfaceDeclaration,
  ): void {
    this.createTypeDefinition(node);

    const hasTypeScope = this.maybeCreateTypeScope(node);

    this.visit(node.typeParameters);
    this.visitArray(node.extends);
    this.visit(node.body);

    if (hasTypeScope) {
      this._referencer.close(node);
    }
  }

  /////////////////////
  // Visit selectors //
  /////////////////////

  DeclareClass(node: DeclareClass): void {
    this._referencer
      .currentScope()
      .defineIdentifier(node.id, new ClassNameDefinition(node.id, node));

    const hasTypeScope = this.maybeCreateTypeScope(node);

    this.visit(node.typeParameters);
    this.visitArray(node.extends);
    this.visitArray(node.implements);
    this.visitArray(node.mixins);
    this.visit(node.body);

    if (hasTypeScope) {
      this._referencer.close(node);
    }
  }

  DeclareClassExtendsCall(node: DeclareClassExtendsCall): void {
    this.visit(node.callee);
    this.visit(node.argument);
  }

  DeclaredPredicate(_: DeclaredPredicate): void {
    // Declared predicates are complicated - they can technically reference external
    // **values** and they can also reference the function type parameters.
    // These are rarely written by hand, and only usually in type declaration code - so
    // we just ignore them for simplicity's sake.
  }

  DeclareExportDeclaration(node: DeclareExportDeclaration): void {
    if (node.declaration) {
      const declaration = node.declaration;
      this.visit(declaration);

      // `declare export` variables are to be considered used by default
      // non-`declare` exported names are handled natively by ESLint's rule
      // as this is flow-specific syntax, we just handle it here for portability
      for (const variable of this._referencer.scopeManager.getDeclaredVariables(
        declaration,
      )) {
        variable.eslintUsed = true;
      }
    } else {
      for (const specifier of node.specifiers) {
        if (specifier.type === 'ExportNamespaceSpecifier') {
          continue;
        }
        if (specifier.exportKind === 'type') {
          this._referencer.currentScope().referenceType(specifier.local);
        } else {
          this._referencer.currentScope().referenceValue(specifier.local);
        }
        // also ignore the exported name
      }
    }
  }

  DeclareModuleExports(node: DeclareModuleExports): void {
    this.visit(node.typeAnnotation);
  }

  DeclareComponent(node: DeclareComponent): void {
    this._referencer
      .currentScope()
      .defineIdentifier(node.id, new ComponentNameDefinition(node.id, node));

    const hasTypeScope = this.maybeCreateTypeScope(node);

    this.visit(node.typeParameters);
    this.visitArray(node.params);
    this.visit(node.rest);
    this.visit(node.rendersType);

    if (hasTypeScope) {
      this._referencer.close(node);
    }
  }

  DeclareFunction(node: DeclareFunction): void {
    const id = node.id;
    if (id != null) {
      this._referencer
        .currentScope()
        .defineIdentifier(id, new FunctionNameDefinition(id, node));
      this.visit(id.typeAnnotation);
    } else {
      this.visit(node.typeAnnotation);
    }
    this.visit(node.predicate);
  }

  ObjectTypePrivateField(node: ObjectTypePrivateField): void {
    this.visit(node.key);
  }

  TupleTypeElement(node: TupleTypeElement): void {
    this.visit(node.elementType);
  }

  DeclareHook(node: DeclareHook): void {
    this._referencer
      .currentScope()
      .defineIdentifier(node.id, new HookNameDefinition(node.id, node));

    // the function type is stored as an annotation on the ID
    this.visit(node.id.typeAnnotation);
  }

  DeclareInterface(node: DeclareInterface): void {
    this.visitInterfaceDeclaration(node);
  }

  DeclareModule(node: DeclareModule): void {
    this._referencer.scopeManager.nestDeclareModuleScope(node);

    // Do not visit 'id', since module name is neither a reference nor a
    // definition that can be referenced.
    this.visit(node.body);

    this._referencer.close(node);
  }

  DeclareNamespace(node: DeclareNamespace): void {
    // `declare global {}` augments the global scope; it binds no name.
    if (node.global !== true) {
      this._referencer
        .currentScope()
        .defineIdentifier(node.id, new NamespaceNameDefinition(node.id, node));
    }

    this._referencer.scopeManager.nestDeclareNamespaceScope(node);
    this.visit(node.body);
    this._referencer.close(node);
  }

  DeclareOpaqueType(node: DeclareOpaqueType): void {
    this.visitOpaqueType(node);
  }

  DeclareTypeAlias(node: DeclareTypeAlias): void {
    this.visitTypeAlias(node);
  }

  DeclareVariable(node: DeclareVariable): void {
    for (const decl of node.declarations) {
      this.visitPattern(
        decl.id,
        pattern => {
          this._referencer
            .currentScope()
            .defineIdentifier(
              pattern,
              new VariableDefinition(pattern, decl, node),
            );
        },
        typeAnnotation => {
          this.visit(typeAnnotation);
        },
      );
    }
  }

  FunctionTypeAnnotation(node: FunctionTypeAnnotation): void {
    const hasTypeScope = this.maybeCreateTypeScope(node);

    this.visit(node.typeParameters);
    this.visit(node.this);
    this.visitArray(node.params);
    this.visit(node.returnType);
    this.visit(node.rest);

    if (hasTypeScope) {
      this._referencer.close(node);
    }
  }

  FunctionTypeParam(node: FunctionTypeParam): void {
    // Do not visit 'name' child to prevent name from being treated as a reference.
    // e.g. 'foo' is a parameter name in a type that should not be treated like a
    // definition or reference in `type T = (foo: string) => void`.
    this.visit(node.typeAnnotation);
  }

  HookTypeAnnotation(node: HookTypeAnnotation): void {
    const hasTypeScope = this.maybeCreateTypeScope(node);

    this.visit(node.typeParameters);
    this.visitArray(node.params);
    this.visit(node.returnType);
    this.visit(node.rest);

    if (hasTypeScope) {
      this._referencer.close(node);
    }
  }

  ComponentTypeAnnotation(node: ComponentTypeAnnotation): void {
    const hasTypeScope = this.maybeCreateTypeScope(node);

    this.visit(node.typeParameters);
    this.visitArray(node.params);
    this.visit(node.rest);
    this.visit(node.rendersType);

    if (hasTypeScope) {
      this._referencer.close(node);
    }
  }

  ComponentTypeParameter(node: ComponentTypeParameter): void {
    // Do not visit 'name' child to prevent name from being treated as a reference.
    // e.g. 'foo' is a parameter name in a type that should not be treated like a
    // definition or reference in `type T = component(foo: string)`.
    this.visit(node.typeAnnotation);
  }

  GenericTypeAnnotation(node: GenericTypeAnnotation): void {
    this.visit(node.id);
    this.visit(node.typeArguments);
  }

  Identifier(node: Identifier): void {
    this._referencer.currentScope().referenceType(node);
  }

  InterfaceDeclaration(node: InterfaceDeclaration): void {
    this.visitInterfaceDeclaration(node);
  }

  ObjectTypeIndexer(node: ObjectTypeIndexer): void {
    // Do not visit 'id' child to prevent id from being treated as a reference.
    // e.g. 'foo' is an unreferenceable name for the indexer parameter in
    // `type T = { [foo: string]: number }`.
    this.visit(node.key);
    this.visit(node.value);
    this.visit(node.variance);
  }

  ObjectTypeInternalSlot(node: ObjectTypeInternalSlot): void {
    // Do not visit 'id' child to prevent id from being treated as a reference.
    // e.g. 'foo' is an internal slot name in `type T = { [[foo]]: number }`.
    this.visit(node.value);
  }

  ObjectTypeProperty(node: ObjectTypeProperty): void {
    // Do not visit 'key' child if it is an identifier to prevent key being treated as a reference.
    // e.g. 'foo' is a property name in `type T = { foo: string }`.
    if (node.key.type !== 'Identifier') {
      this.visit(node.key);
    }

    this.visit(node.value);
    this.visit(node.variance);
  }

  ObjectTypeMappedTypeProperty(node: ObjectTypeMappedTypeProperty): void {
    this._referencer.scopeManager.nestTypeScope(node);

    // This will create a type defintion for the `key` property.
    this.visit(node.keyTparam);

    // Visit remaining properties.
    this.visit(node.propType);
    this.visit(node.sourceType);
    this.visit(node.nameType);
    this.visit(node.variance);

    this._referencer.close(node);
  }

  OpaqueType(node: OpaqueType): void {
    this.visitOpaqueType(node);
  }

  QualifiedTypeIdentifier(node: QualifiedTypeIdentifier): void {
    // Only the first component of a qualified type identifier is a reference,
    // e.g. 'Foo' in `type T = Foo.Bar.Baz`.
    let currentNode = node.qualification;
    while (currentNode.type === 'QualifiedTypeIdentifier') {
      currentNode = currentNode.qualification;
    }

    // qualified names *usually* only reference values like
    //     import * as Foo from 'foo';
    //     type T = Foo.Bar;
    // however, it is possible for a module to do something like
    //     class Class { ... }
    //     export default { Class }
    // meaning this is also valid
    //     import type Foo from 'foo';
    //     type T = Foo.Class;
    if (currentNode.type === 'Identifier') {
      this._referencer.currentScope().referenceDualValueType(currentNode);
    }
  }

  QualifiedTypeofIdentifier(node: QualifiedTypeofIdentifier): void {
    // Only the first component of a qualified type identifier is a reference,
    // e.g. 'Foo' in `type T = Foo.Bar.Baz`.
    let currentNode = node.qualification;
    while (currentNode.type === 'QualifiedTypeofIdentifier') {
      currentNode = currentNode.qualification;
    }

    if (currentNode.type === 'Identifier') {
      this._referencer.currentScope().referenceDualValueType(currentNode);
    }
  }

  TypeAlias(node: TypeAlias): void {
    this.visitTypeAlias(node);
  }

  TypeofTypeAnnotation(node: TypeofTypeAnnotation): void {
    let currentNode = node.argument;
    while (currentNode.type === 'QualifiedTypeofIdentifier') {
      currentNode = currentNode.qualification;
    }
    if (currentNode.type === 'Identifier') {
      // typeof annotations can only reference values!
      this._referencer.currentScope().referenceValue(currentNode);
    }
    this.visit(node.typeArguments);
  }

  TypeParameter(node: TypeParameter): void {
    const def = new TypeParameterDefinition(node);
    this._referencer.currentScope().defineIdentifier(def.name, def);

    this.visit(node.bound);
    this.visit(node.variance);
    this.visit(node.default);
  }
}

export {TypeVisitor};
