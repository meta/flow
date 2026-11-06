import type {Exported} from './object_type';

// No error here: `Exported` is only reported where it is declared.
declare var d: Exported;
d['x'] as number;

declare var l: LibDict;
l['x'] as number;
