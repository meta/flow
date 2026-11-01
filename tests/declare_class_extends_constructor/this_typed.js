// `Uint8Array` is an interface plus a `declare var` of its constructor
// interface. A `this`-typed member inherited through the constructor still
// refers to the derived class.

declare class DeclaredBuffer extends Uint8Array {
  toString(encoding?: string): string;
}
declare const declared: DeclaredBuffer;
declared.fill(0) as DeclaredBuffer; // ok
declared.fill(0).toString('hex') as string; // ok

class UserBuffer extends Uint8Array {
  toString(encoding?: string): string {
    return encoding ?? '';
  }
}
export type UserBufferType = UserBuffer;
new UserBuffer(2).fill(0) as UserBuffer; // ok
new UserBuffer(2).fill(0) as DeclaredBuffer; // error: `UserBuffer` ~> `DeclaredBuffer`

// The constructor return is concretized through an EvalT before its type
// application can be this-specialized.
type EvaluatedTypedArray = {instance: Uint8Array<ArrayBuffer>}['instance'];
interface EvaluatedTypedArrayConstructor {
  new(length: number): EvaluatedTypedArray;
}
declare const EvaluatedTypedArray: EvaluatedTypedArrayConstructor;
class EvaluatedUserBuffer extends EvaluatedTypedArray {}
new EvaluatedUserBuffer(2).fill(0) as EvaluatedUserBuffer; // ok
