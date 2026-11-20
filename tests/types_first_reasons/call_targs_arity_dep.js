import {f, getF} from './call_targs_arity';
f<number>(1); // Err: too few type arguments, reported at the call
async function g() {
  (await getF())<number>(1); // Err: too few type arguments, reported at the call
  (await getF())<number, string, boolean>(1); // Err: too many type arguments, reported at the call
}
