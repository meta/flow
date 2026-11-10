// Computed property with a string literal key
export declare const STR_KEY: "myProp";
export type WithStrKey = {[STR_KEY]: number};

// Computed property with a number literal key
export declare const NUM_KEY: 42;
export type WithNumKey = {[NUM_KEY]: string};

// Indexer with label (still works)
export type WithIndexer = {[key: string]: number};

// Computed property in interface
export declare const IFACE_KEY: "ifaceProp";
export interface WithComputedIface {
  [IFACE_KEY]: boolean;
}

// Computed property in declare class
export declare const CLASS_KEY: "classProp";
export declare class WithComputedClass {
  [CLASS_KEY]: string;
}

// Symbol.iterator computed property
export type WithSymbolIterator = {[Symbol.iterator]: () => Iterator<number>};

// Mixed: computed property + indexer
export type Mixed = {
  [STR_KEY]: number;
  [key: string]: string | number;
};

// Optional computed method
export declare const METHOD_KEY: "myMethod";
export type WithOptionalMethod = {[METHOD_KEY]?(x: number): void};

// Computed property with spread
export declare const SPREAD_KEY: "spreadProp";
export type WithSpread = {[SPREAD_KEY]: number, ...WithStrKey};

// Computed method in declare class (proto member)
export declare const PROTO_KEY: "protoMethod";
export declare class WithProtoMethod {
  [PROTO_KEY](): string;
}

// Overloaded computed method in interface
export declare const OVERLOAD_KEY: "overloaded";
export interface WithOverload {
  [OVERLOAD_KEY](x: number): number;
  [OVERLOAD_KEY](x: string): string;
}

// String literal computed key (no binding needed)
export type WithStrLitKey = {["directStr"]: number};

// Number literal computed key (no binding needed)
export type WithNumLitKey = {[100]: string};

// Member expression computed key
export declare const Keys: { readonly foo: "nsProp" };
export type WithMemberKey = {[Keys.foo]: boolean};

// Member expression method in interface
export declare const MethodKeys: { readonly bar: "nsMethod" };
export interface WithMemberMethod {
  [MethodKeys.bar](): string;
}

// Member expression field in declare class
export declare const ClassKeys: { readonly baz: "nsClassProp" };
export declare class WithMemberClass {
  [ClassKeys.baz]: number;
}

// Accessibility modifiers with computed property in declare class
export declare const PROTECTED_KEY: "protectedProp";
export declare class WithAccessibility {
  protected [PROTECTED_KEY]: string;
  private [CLASS_KEY]: number;
  public [PROTO_KEY]: boolean;
}

// Union key type should not resolve to a computed property
export declare const UNION_KEY: "a" | "b";
export type WithUnionKey = {[UNION_KEY]: number}; // ERROR

// Computed methods declared without bodies in a class
export declare const CLASS_METHOD_KEY: "classMethod";
export declare const CLASS_METHOD_SYM: unique symbol;
export declare const OPTIONAL_METHOD_KEY: "optionalMethod";
export class WithComputedMethodSignatures {
  [Symbol.iterator](): Iterator<number>;
  [CLASS_METHOD_KEY](): string;
  [CLASS_METHOD_SYM](): boolean;
  [OPTIONAL_METHOD_KEY]?(): number;
  [Symbol.asyncIterator]?(): AsyncIterator<string>;
  get [Symbol.dispose](): () => void;
}
