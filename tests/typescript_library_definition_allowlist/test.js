import {rootTs} from './root';
import {rootMts} from './root.d.mts';
import {rootCts} from './root.d.cts';
import {single} from './single/top';
import {recursive} from './recursive/top';
import {nestedMts} from './recursive/nested/allowed.d.mts';
import {nestedCts} from './recursive/nested/allowed.d.cts';
import {rejectedTs} from './single/nested/rejected'; // ERROR: rejected declarations aren't included
import {rejectedMts} from './single/nested/rejected.d.mts'; // ERROR
import {rejectedCts} from './single/nested/rejected.d.cts'; // ERROR
import {other} from './other/rejected'; // ERROR

rootTs as number;
rootMts as string;
rootCts as boolean;
single as number;
recursive as number;
nestedMts as string;
nestedCts as boolean;

rootTs as string; // ERROR
rootMts as number; // ERROR
rootCts as string; // ERROR
single as string; // ERROR
recursive as string; // ERROR
nestedMts as number; // ERROR
nestedCts as string; // ERROR
