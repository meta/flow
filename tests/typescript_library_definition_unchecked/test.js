import {allowed} from './node_modules/pkg/allowed';
import {rejected} from './node_modules/pkg/rejected'; // ERROR: rejected declarations aren't included
import {allowedDeclaration} from './declarations/allowed';
import {rejectedDeclaration} from './declarations/rejected'; // ERROR

allowed as number;
allowedDeclaration as number;

allowed as string; // ERROR
allowedDeclaration as string; // ERROR
packageGlobal; // ERROR: globals from rejected declarations aren't included
