import type { NodeClass, Keyed } from './index';
import { ParagraphNode, KeyedClass } from './index';
import type { NodeClassThroughReexport } from './index';
import { PEER } from './node';

declare const nodeClass: NodeClass;
new nodeClass().getKey() as number; // tsc reports string ~> number (TS2352)
new ParagraphNode().getKey() as number; // tsc reports string ~> number (TS2352)
declare const reexportedClass: NodeClassThroughReexport;
new reexportedClass().getKey() as number; // tsc reports string ~> number (TS2352)

declare const keyed: Keyed;
keyed.key as string; // tsc reports number ~> string (TS2352)
keyed.k as number; // tsc reports string ~> number (TS2352)
keyed[PEER] as number; // tsc reports boolean ~> number (TS2352)
new KeyedClass().k() as number; // tsc reports string ~> number (TS2352)
