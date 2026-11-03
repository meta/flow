// Node and TypeScript resolve `.//dep` like `./dep`, as `tsc` emits in
// declaration files.
import {value} from './/dep';
import {value as fromParent} from '..//relative_import_empty_name/dep';

value as number; // OK
fromParent as number; // OK
value as string; // ERROR
