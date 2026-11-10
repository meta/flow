import type {WithStrKey, WithNumKey, WithIndexer, WithComputedIface, Mixed} from './computed_property_key';
import type {WithOptionalMethod, WithSpread, WithOverload} from './computed_property_key';
import type {WithProtoMethod} from './computed_property_key';
import {STR_KEY, NUM_KEY, IFACE_KEY} from './computed_property_key';

// String literal computed property key resolves to a named property
declare const strObj: WithStrKey;
strObj.myProp as number; // OK
strObj.myProp as string; // ERROR

// Number literal computed property key resolves to a named property
declare const numObj: WithNumKey;
numObj[42] as string; // OK
numObj[42] as number; // ERROR

// Indexer still works
declare const indexerObj: WithIndexer;
indexerObj["anything"] as number; // OK

// Interface with computed property
declare const ifaceObj: WithComputedIface;
ifaceObj.ifaceProp as boolean; // OK
ifaceObj.ifaceProp as string; // ERROR

// Mixed computed property and indexer
declare const mixedObj: Mixed;
mixedObj.myProp as number; // OK

// Optional computed method
declare const optMethodObj: WithOptionalMethod;
optMethodObj.myMethod as ((x: number) => void) | void; // OK
optMethodObj.myMethod as string; // ERROR

// Computed property with spread
declare const spreadObj: WithSpread;
spreadObj.spreadProp as number; // OK
spreadObj.myProp as number; // OK (from spread)

// Proto computed method in declare class
declare const protoObj: WithProtoMethod;
protoObj.protoMethod() as string; // OK
protoObj.protoMethod() as number; // ERROR

// Overloaded computed method in interface
declare const overloadObj: WithOverload;
overloadObj.overloaded(42) as number; // OK
overloadObj.overloaded("hi") as string; // OK

// String literal computed key
import type {WithStrLitKey} from './computed_property_key';
declare const strLitObj: WithStrLitKey;
strLitObj.directStr as number; // OK
strLitObj.directStr as string; // ERROR

// Number literal computed key
import type {WithNumLitKey} from './computed_property_key';
declare const numLitObj: WithNumLitKey;
numLitObj[100] as string; // OK
numLitObj[100] as number; // ERROR

// Member expression computed key
import type {WithMemberKey} from './computed_property_key';
declare const memberObj: WithMemberKey;
memberObj.nsProp as boolean; // OK
memberObj.nsProp as string; // ERROR

// Member expression method in interface
import type {WithMemberMethod} from './computed_property_key';
declare const memberMethodObj: WithMemberMethod;
memberMethodObj.nsMethod() as string; // OK
memberMethodObj.nsMethod() as number; // ERROR

// Member expression field in declare class
declare const memberClassObj: WithMemberClass;
import {WithMemberClass} from './computed_property_key';
memberClassObj.nsClassProp as number; // OK
memberClassObj.nsClassProp as string; // ERROR

// Accessibility modifiers with computed property in declare class
import {WithAccessibility} from './computed_property_key';
declare const accObj: WithAccessibility;
accObj.protectedProp as string; // OK
accObj.classProp as number; // ERROR: private
accObj.protoMethod as boolean; // OK

// Union key should error (not resolved as computed property)
import type {WithUnionKey} from './computed_property_key';
declare const unionObj: WithUnionKey;
unionObj.a; // ERROR

// Computed methods declared without bodies in a class
import {WithComputedMethodSignatures, CLASS_METHOD_SYM} from './computed_property_key';
declare const methodSigs: WithComputedMethodSignatures;
for (const n of methodSigs) {
  n as number; // OK
}
methodSigs.classMethod() as string; // OK
methodSigs[CLASS_METHOD_SYM]() as boolean; // OK
methodSigs.classMethod() as number; // ERROR
methodSigs.optionalMethod as void | (() => number); // OK
methodSigs[Symbol.asyncIterator] as void | (() => AsyncIterator<string>); // OK
methodSigs[Symbol.dispose] as () => void; // OK
