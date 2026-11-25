import type { NodeClass } from './index';
import { ParagraphNode } from './index';
import type { NodeClassThroughReexport } from './index';

declare const nodeClass: NodeClass;
new nodeClass().getKey() as number; // tsc reports string ~> number (TS2352)
new ParagraphNode().getKey() as number; // tsc reports string ~> number (TS2352)
declare const reexportedClass: NodeClassThroughReexport;
new reexportedClass().getKey() as number; // tsc reports string ~> number (TS2352)
