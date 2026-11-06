// Abstract method annotations depend on classes declared later.
export abstract class Visitor {
  abstract visit(node: Node): Node; // OK
}
export declare class Node { value: number; }

// Abstract property annotations depend on classes declared later.
export abstract class Container {
  abstract node: Entry; // OK
}
export declare class Entry { value: string; }

// Ambient class index signatures depend on classes declared later.
export class Dictionary {
  [key: string]: Item; // OK
}
export declare class Item { value: boolean; }
