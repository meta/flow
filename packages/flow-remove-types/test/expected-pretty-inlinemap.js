/*  */
// @nolint

// Regular import
import {
  Something,
} from 'some-module';

// Regular import with types only
import {
} from 'some-module';

// Mixed default and named type only imports
import DefaultImport, {
} from 'some-module';

// Import types

// Typed function
async function test(x, y /*.*/ /*.*/ , z /*.*/ /*.*/ = 123) {
  // Typed expression
  return await (x);
}

// Interface

// Exported interface

// Interface extends

// Implements interface
class Bar extends Other /*.*/ {
  // Class Property with default value
  answer = 42;

  // Class Property with default value and variance
covariant = 42;

  // Class Property
  prop;

  // Class Property with variance
propCo;

  method() {
    return;
  }
}

// Class expression implements interface
var SomeClass = class BazClass {
  prop;

  method() {
    return;
  }
};

// Parametric class
class Wrapper {
  get() {
    return this.value;
  }

  map() {
    // do something
  }
}

// Extends Parametric class
class StringWrapper extends Wrapper {
  // ...
}

// Declare class

// Declare function

// Declare interface

// Declare module

// Declare type alias

// Declare variable

// Type alias

// Export type

// Export type *

// Regular export
export { Wrapper };

// Exported type alias

// Object with types within
var someObj = {
  objMethod() {
    // do nothing.
  }
}

// Example from README
import SomeClassImport from 'some-module'

export class MyClass extends SomeClassImport {

  value

  constructor(value) {
    this.value = value
  }

  get() {
    return this.value
  }

}

// Test async/await functions
async function asyncFunction(input) {
  return await t;
}

// Test read-only data

// Test covariant type variant class with constraint and default.
export class TestClassWithDefault {

  constructor() {}
}

var newline_arrow = () => 42;

var newline_arrow_2 = ()=>42;

// Test calling a function with explicit type arguments
doSomething(3);
doSomething(3);

// Test invoking a constructor with explicit type arguments
new Event();

// Test type union and intersection syntax with leading "operator"
var union;
var intersection;

// Test generic async arrow funcion
const asyncArrow = async() => {};

// Comment type annotations are preserved
var X /*: {
  version: string,
} */ = { version: '42'};

function method(param /*: string */) /*: number */ {
  // ...
}

// declared class fields
class MyClassWithDeclare {
}

// Comment type includes are not emptied out
class MyClassWithComment {
  /*:: prop: string; */
}

// Inferred predicate
function inferredPredicateWithType(arg) {
  return !!arg;
}

function inferredPredicateWithoutType(arg) {
  return !!arg;
}

// Type guards
function typeGuardFunction(x) {
  return typeof x === "boolean";
}

const typeGuardArrow = (x) => (typeof x === "boolean");

function typeGuardInComments(x /*: mixed */) /*: x is boolean */ {
  return typeof x === "boolean";
}

function typeAssertsFunction1(x) {
  if (typeof x !== "boolean") throw new Error;
}

function typeAssertsFunction2(x) {
  if (!x) throw new Error;
}

// Test function with default type parameter
function functionWithDefault() {}

// Opaque types

// Declare export

// `this` params

function thisParam1 () {}
function thisParam2 ( ...a) {}
function thisParam3 (
 ...a) {}
function thisParam4 (
) {}
function thisParam5 (
   ...a) {}
function thisParam6(
) {}
function thisParam7(
  a
) {}

function thisParam8(
  a
) {
  function thisParam9( a) {}
}

const thisConst1 = function() {}
const thisConst2 = function( ...a) {}
const thisConst3 = function(...a) {}
const thisConst4 = function(
) {}
const thisConst5 = function(a,) {}

// `as` cast
const asAny = 'any';
const asArray = [1, 2, 3];
const asBigIntLiteral = 1n;
const asBigInt = 1n;
const asBooleanLiteral = true;
const asBoolean = true;
const asComponent = (() => {});
const asComponentGeneric = (() => {});
const asComponentGenericWithDefault = (() => {});
const asEmpty = {};
const asExists = 'exists';
const asFunction = (() => {});
const asFunctionGeneric = (() => {});
const asFunctionGenericWithDefault = (() => {});
const asKeyof = 'a';
const asMixed = 'mixed';
const asNullable = null;
const asNullLiteral = null;
const asNumberLiteral = 1;
const asNumber = 1;
const asObject = { a: 'a' };
const asParametrizedGeneric = 'generic';
const asStringLiteral = 'literal';
const asString = 'string';
const asSymbol = Symbol('symbol');
const asTuple = ['a', 1];
const asTypeof = 'typeof';
const asUnion = 'union';
const asVoid = undefined;

const asConditional = 'conditional';

const asInterface = { a: 'a', b: 1 };

const asInfer = 'infer';

const asIntersection = { a: 'a', b: 1 };

const asIndexed = 'indexed';

// `as const`
's';
['s'];

// chained `as`
const chain1 = '1';
const chain2 = '1';

const templateLiteral = `prefix-${42}`;
const asTemplateLiteral = templateLiteral;

export /*.*/ class AbstractBase {

  concrete(value) {
    return value;
  }

  static create() {
    return 1;
  }
}

 class AbstractDerived extends AbstractBase {
}

class ConcreteDerived extends AbstractDerived {
 /*.*/ property = 42;
 uninitialized;

 method(value) {
    return value;
  }

  static /*.*/ create() {
    return 2;
  }

 ['computed'](value) {
    return value;
  }

 get result() {
    return this.property;
  }

 set result(value) {
    this.property = value;
  }
}

const AbstractExpression = class {
};

export default class {
}


class ModifierNames {
  abstract = 'abstract';
  override() {
    return 'override';
  }
}

const satisfiesObject = {value: 42} /*.*/ /*.*/;
const satisfiesGeneric = [];
const satisfiesFunction = ((value) => value);
const satisfiesConst = {value: 42};
const satisfiesChain = 42;
const satisfiesTemplate = 'prefix-42';
const satisfiesPredicate = ((value) => true);
const asPredicate = ((value) => true);

class OptionalFields extends ConcreteDerived {
  optional;
  initialized = 42;
  ['question?'] /*.*/ /*.*/;
  [true ? 'first' : 'second'];
  #private;
  static optionalStatic;
 optionalReadonly;
 property;
}

export { Something as InlineFirst} from 'some-module';
export {Something as InlineLast,} from 'some-module';
export {Something as InlineTrailing,} from 'some-module';
export {Something as InlineMiddle, SomeOtherThing as InlineOther} from 'some-module';
export {
  Something as InlineCommented,
 /*.*/} from 'some-module';
export {} from 'side-effect-module';

const type = 42;
export {type as ContextualType};
export {Something as type} from 'some-module';



// Type-only default exports are erased without leaving a runtime wrapper.

//# sourceMappingURL=data:application/json;charset=utf-8;base64,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
