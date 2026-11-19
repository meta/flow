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

import type {ESNode} from 'flow-estree';
// $FlowExpectedError[cannot-resolve-module]
import esquery from 'esquery';

//
// Base Atoms
//
interface Atom {
  // type: string;
}
interface SubjectSelectorAtom extends Atom {
  subject?: ?boolean;
}
interface NthSelectorAtom extends SubjectSelectorAtom {
  index: NumericLiteralAtom;
}
interface BinarySelectorAtom extends SubjectSelectorAtom {
  // type: 'child' | 'sibling' | 'adjacent' | 'descendant';
  left: SubjectSelector;
  right: SubjectSelector;
}
interface MultiSelectorAtom extends SubjectSelectorAtom {
  selectors: Array<SubjectSelector>;
}
interface LiteralAtom extends Atom {
  type: 'literal';
  // value: string | number;
}

//
// Literals
//
interface StringLiteralAtom extends LiteralAtom {
  value: string;
}
interface NumericLiteralAtom extends LiteralAtom {
  value: number;
}
interface RegExpLiteralAtom extends Atom {
  type: 'regexp';
  value: RegExp;
}

//
// Atoms
//
interface FieldAtom extends Atom {
  type: 'field';
  name: string;
}
interface TypeAtom extends Atom {
  type: 'type';
  value: string;
}
interface SequenceAtom extends MultiSelectorAtom {
  type: 'compound';
}
interface IdentifierAtom extends SubjectSelectorAtom {
  type: 'identifier';
  value: string;
}
interface WildcardAtom extends SubjectSelectorAtom {
  type: 'wildcard';
  value: '*';
}
interface AttributeAtom extends SubjectSelectorAtom {
  type: 'attribute';
  name: string;
  operator?: ?('=' | '!=' | '>' | '<' | '>=' | '<=');
  value?: ?(Literal | RegExpLiteralAtom | TypeAtom);
}
interface NthChildAtom extends NthSelectorAtom {
  type: 'nth-child';
}
interface NthLastChildAtom extends NthSelectorAtom {
  type: 'nth-last-child';
}
interface DescendantAtom extends BinarySelectorAtom {
  type: 'descendant';
}
interface ChildAtom extends BinarySelectorAtom {
  type: 'child';
}
interface SiblingAtom extends BinarySelectorAtom {
  type: 'sibling';
}
interface AdjacentAtom extends BinarySelectorAtom {
  type: 'adjacent';
}
interface NegationAtom extends MultiSelectorAtom {
  type: 'not';
}
interface MatchesAtom extends MultiSelectorAtom {
  type: 'matches';
}
interface HasAtom extends MultiSelectorAtom {
  type: 'has';
}
interface ClassAtom extends Atom {
  type: 'class';
  name: 'declaration' | 'expression' | 'function' | 'pattern' | 'statement';
}

//
// Unions
//
export type Selector =
  | FieldAtom
  | TypeAtom
  | SequenceAtom
  | IdentifierAtom
  | WildcardAtom
  | AttributeAtom
  | NthChildAtom
  | NthLastChildAtom
  | DescendantAtom
  | ChildAtom
  | SiblingAtom
  | AdjacentAtom
  | NegationAtom
  | MatchesAtom
  | HasAtom
  | ClassAtom;
type MultiSelector = SequenceAtom | NegationAtom | MatchesAtom | HasAtom;
type BinarySelector = DescendantAtom | ChildAtom | SiblingAtom | AdjacentAtom;
type NthSelector = NthChildAtom | NthLastChildAtom;
type SubjectSelector =
  | NthSelector
  | BinarySelector
  | MultiSelector
  | IdentifierAtom
  | WildcardAtom
  | AttributeAtom;
type Literal = StringLiteralAtom | NumericLiteralAtom;

export type ESQueryOptions = Readonly<{
  visitorKeys?: Readonly<{[key: string]: ReadonlyArray<string>}>,
  fallback?: (node: ESNode) => ReadonlyArray<string>,
}>;

/** Parse a selector and return its AST. */
export const parse = esquery.parse as (selector: string) => Selector;
/** From a JS AST and a selector AST, collect all JS AST nodes that match the selector. */
export const match = esquery.match as (
  ast: ESNode,
  selector: ?Selector,
  options?: ESQueryOptions,
) => Array<ESNode>;
/** Given a `node` and its ancestors, determine if `node` is matched by `selector`. */
export const matches = esquery.matches as (
  node: ?ESNode,
  selector: ?Selector,
  ancestry?: Array<ESNode>,
  options?: ESQueryOptions,
) => boolean;
/** Query the code AST using the selector string. */
export const query = esquery.query as (
  ast: ESNode,
  selector: string,
  options?: ESQueryOptions,
) => Array<ESNode>;
