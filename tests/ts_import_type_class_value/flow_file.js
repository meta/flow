// `import type` means the same in Flow files.
import type { LexicalNode } from './node';

declare const nodeClass: typeof LexicalNode;
new nodeClass().getKey() as number; // error: string ~> number
new LexicalNode(); // error: imported with `import type`

import * as FlowExports from './flow_export_type';
declare const notExported: typeof FlowExports.g; // error: `g` is no value export
