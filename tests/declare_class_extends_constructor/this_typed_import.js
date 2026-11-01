// The exported type's signature is built through annotation inference. Its
// inherited `this` must still refer to the exported class when imported.
import type {UserBufferType} from './this_typed';

declare const buffer: UserBufferType;
buffer.fill(0) as UserBufferType; // ok
buffer.fill(0).toString('hex') as string; // ok
