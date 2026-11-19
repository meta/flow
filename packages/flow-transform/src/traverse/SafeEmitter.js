/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 *
 * @flow strict
 * @format
 */

'use strict';

import type {ESNode} from 'flow-estree';

export type EmitterListener = (node: ESNode) => void;

/**
 * Creates an object which can listen for and emit events.
 * This is similar to the EventEmitter API in Node's standard library, but it has a few differences.
 * The goal is to allow multiple modules to attach arbitrary listeners to the same emitter, without
 * letting the modules know about each other at all.
 * 1. It has no special keys like `error` and `newListener`, which would allow modules to detect when
 * another module throws an error or registers a listener.
 * 2. It calls listener functions without any `this` value. (`EventEmitter` calls listeners with a
 * `this` value of the emitter instance, which would give listeners access to other listeners.)
 */
export class SafeEmitter {
  // $FlowExpectedError[incompatible-type] - Object.create is always typed as returning `unknown`
  readonly listeners: {[key: string]: Array<EmitterListener>} =
    Object.create(null);

  on(eventName: string, listener: EmitterListener): void {
    if (eventName in this.listeners) {
      this.listeners[eventName].push(listener);
    } else {
      this.listeners[eventName] = [listener];
    }
  }
  emit(eventName: string, node: ESNode): void {
    if (eventName in this.listeners) {
      this.listeners[eventName].forEach(listener => listener(node));
    }
  }
  eventNames(): Array<string> {
    return Object.keys(this.listeners);
  }
}
