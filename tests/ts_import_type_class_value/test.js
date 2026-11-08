import type { NodeClass } from './index';
import { ParagraphNode } from './index';

declare const nodeClass: NodeClass;
new nodeClass().getKey() as number; // tsc reports string ~> number (TS2352)
new ParagraphNode().getKey() as number; // tsc reports string ~> number (TS2352)
