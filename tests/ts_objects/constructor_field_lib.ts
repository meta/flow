// Pure TS: classes that type `this.constructor` with a field named
// `constructor`, the way Lexical's node classes do.

export type Ctor<T> = {new (...args: any[]): T};

export declare class BaseNode {
  ['constructor']: Ctor<BaseNode>;
  constructor(key?: string);
  getKey(): string;
}

export declare class TextNode extends BaseNode {
  ['constructor']: Ctor<TextNode>;
  constructor(text: string, key?: string);
}

// Inherits the constructor of `TextNode`.
export declare class TabNode extends TextNode {
  ['constructor']: Ctor<TabNode>;
}

// Inherits both the constructor and the field of `TextNode`.
export declare class MidNode extends TextNode {}

declare const AnyBase: any;

// Inherits the constructor of `any`.
export declare class AnyNode extends AnyBase {
  ['constructor']: Ctor<AnyNode>;
}

new TextNode('text'); // OK
new TabNode('tab'); // OK

class LocalNode extends TextNode {
  constructor() {
    super('local'); // OK
  }
}

new TextNode('text').constructor satisfies Ctor<TextNode>; // OK
new LocalNode().constructor satisfies Ctor<TextNode>; // OK: inherited field
new TextNode('text').constructor satisfies number; // ERROR

export declare class CallableNode {
  ['constructor']: () => number;
  constructor(s: string);
}

export declare class PlainTsNode {
  constructor(n: number);
}
declare const plain: PlainTsNode;
export const plainCtor = plain.constructor;
