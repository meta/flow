import type {Exported} from './function_type';

// No error here: `Exported` is only reported where it is declared.
declare var f: Exported;
f(1);
