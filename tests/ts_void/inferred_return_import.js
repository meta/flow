import {exportedNoReturn, exportedArrow, exportedAsync} from './inferred_return';

// The signatures of exported functions agree.
exportedNoReturn() as void; // OK
exportedNoReturn() as undefined; // ERROR
exportedArrow() as undefined; // ERROR
exportedAsync() as Promise<undefined>; // ERROR
