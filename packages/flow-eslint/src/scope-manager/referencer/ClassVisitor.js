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
  AbstractMethodDefinition,
  AbstractPropertyDefinition,
  ClassBody,
  ClassDeclaration,
  ClassExpression,
  DeclareMethodDefinition,
  ESNode,
  Identifier,
  MethodDefinition,
  ObjectTypeIndexer,
  PropertyDefinition,
  StaticBlock,
} from 'flow-estree';
import type {Referencer} from './Referencer';

import {ClassNameDefinition} from '../definition';
import {TypeVisitor} from './TypeVisitor';
import {Visitor} from './Visitor';

class ClassVisitor extends Visitor {
  readonly _classNode: ClassDeclaration | ClassExpression;
  readonly _referencer: Referencer;

  constructor(
    referencer: Referencer,
    node: ClassDeclaration | ClassExpression,
  ) {
    super(referencer);
    this._referencer = referencer;
    this._classNode = node;
  }

  static visit(
    referencer: Referencer,
    node: ClassDeclaration | ClassExpression,
  ): void {
    const classVisitor = new ClassVisitor(referencer, node);
    classVisitor.visitClass(node);
  }

  visit(node: ?ESNode): void {
    // make sure we only handle the nodes we are designed to handle
    if (node && node.type in this) {
      super.visit(node);
    } else {
      this._referencer.visit(node);
    }
  }

  ///////////////////
  // Visit helpers //
  ///////////////////

  visitClass(node: ClassDeclaration | ClassExpression): void {
    if (node.type === 'ClassDeclaration' && node.id) {
      const id = node.id;
      this._referencer
        .currentScope()
        .defineIdentifier(id, new ClassNameDefinition(id, node));
    }

    node.decorators?.forEach(d => this._referencer.visit(d));

    this._referencer.scopeManager.nestClassScope(node);

    if (node.id) {
      const id = node.id;
      // define the class name again inside the new scope
      // references to the class should not resolve directly to the parent class
      this._referencer
        .currentScope()
        .defineIdentifier(id, new ClassNameDefinition(id, node));
    }

    this._referencer.visit(node.superClass);

    // visit the type param declarations
    this.visitType(node.typeParameters);
    // then the usages
    this.visitType(node.superTypeArguments);
    node.implements?.forEach(imp => this.visitType(imp));

    this.visit(node.body);

    this._referencer.close(node);
  }

  visitPropertyDefinition(node: PropertyDefinition): void {
    this.visitProperty(node);
    this.visitType(node.typeAnnotation);
  }

  visitProperty(node: PropertyDefinition): void {
    node.decorators?.forEach(d => this._referencer.visit(d));

    if (node.computed) {
      this._referencer.visit(node.key);
    }

    if (node.value) {
      const value = node.value;
      if (node.type === 'PropertyDefinition') {
        this._referencer.scopeManager.nestClassFieldInitializerScope(value);
      }

      this._referencer.visit(value);

      if (node.type === 'PropertyDefinition') {
        this._referencer.close(value);
      }
    }
  }

  visitMethod(node: MethodDefinition): void {
    node.decorators?.forEach(d => this._referencer.visit(d));

    if (node.computed) {
      this._referencer.visit(node.key);
    }

    this._referencer.visitFunction(node.value);
  }

  visitStaticBlock(node: StaticBlock): void {
    this._referencer.scopeManager.nestClassStaticBlockScope(node);
    this._referencer.visitChildren(node);
    this._referencer.close(node);
  }

  visitType: (?ESNode) => void = (node): void => {
    if (!node) {
      return;
    }
    TypeVisitor.visit(this._referencer, node);
  };

  /////////////////////
  // Visit selectors //
  /////////////////////

  ClassBody(node: ClassBody): void {
    // this is here on purpose so that this visitor explicitly declares visitors
    // for all nodes it cares about (see the instance visit method above)
    this.visitChildren(node);
  }

  PropertyDefinition(node: PropertyDefinition): void {
    this.visitPropertyDefinition(node);
  }

  MethodDefinition(node: MethodDefinition): void {
    this.visitMethod(node);
  }

  AbstractMethodDefinition(node: AbstractMethodDefinition): void {
    if (node.computed) {
      this._referencer.visit(node.key);
    }
    this.visitType(node.value);
  }

  AbstractPropertyDefinition(node: AbstractPropertyDefinition): void {
    if (node.computed) {
      this._referencer.visit(node.key);
    }
    this.visitType(node.typeAnnotation);
    this.visitType(node.variance);
  }

  DeclareMethodDefinition(node: DeclareMethodDefinition): void {
    if (node.computed) {
      this._referencer.visit(node.key);
    }
    this.visitType(node.value);
  }

  ObjectTypeIndexer(node: ObjectTypeIndexer): void {
    this.visitType(node);
  }

  StaticBlock(node: StaticBlock): void {
    this.visitStaticBlock(node);
  }

  Identifier(node: Identifier): void {
    this._referencer.visit(node);
  }

  PrivateIdentifier(): void {
    // intentionally skip
  }
}

export {ClassVisitor};
