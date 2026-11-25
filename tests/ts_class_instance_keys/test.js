// @flow
import {CustomElement, Editor, getRect, type EditorKey, type Ext} from 'lib';

const method: keyof Editor = 'getRootElement'; // ok
const inheritedMethod: keyof Editor = 'baseMethod'; // ok
const inheritedField: keyof Editor = 'baseField'; // ok
const ctor: keyof Editor = 'constructor'; // error
const staticMethod: keyof Editor = 'make'; // error
const viaInterface: keyof Ext = 'getRootElement'; // ok
const viaExportedAlias: EditorKey = 'baseMethod'; // ok

declare const editor: Editor;
getRect(editor); // ok
getRect({getRootElement: () => null}); // ok
const picked: Pick<Editor, 'getRootElement' | 'baseMethod'> = editor; // ok
picked.getRootElement() as null | string; // ok
const omitted: Omit<Editor, 'field'> = editor; // ok
omitted.baseMethod(); // ok
omitted.field; // error

class FlowBase {
  m(): void {}
}
class FlowClass extends FlowBase {
  f: number = 1;
  n(): void {}
}
const flowMethod: keyof FlowClass = 'n'; // error: a Flow class's keys are its own properties

// The methods of a class are keys only when the class is declared in TypeScript.
const ownOfTsClass: keyof CustomElement = 'own'; // ok
const fieldOfFlowSuper: keyof CustomElement = 'tabIndex'; // ok
const methodOfFlowSuper: keyof CustomElement = 'focus'; // error
const pickedOwn: Pick<CustomElement, 'own'> = {own() {}}; // ok
declare const readonlyCustom: Readonly<CustomElement>;
readonlyCustom.focus; // error
