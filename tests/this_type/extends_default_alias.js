import type {J} from './extends_default_alias_export';

declare var j: J;
j.v as string; // OK
j.v as boolean; // ERROR
