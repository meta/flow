/**
 * Regression test for annotation_inference AnnotSpecializeT match ordering fix.
 *
 * Tests that accessing .constructor on an imported instance and extending it
 * does not produce invalid-exported-annotation errors.
 */

import type {EventSubscription} from './event_types';

class EventEmitter {
  constructor() {}
  addEventListener(
    eventType: string,
    listener: (...args: ReadonlyArray<unknown>) => unknown,
  ): EventSubscription {
    return {remove: () => {}};
  }
}

const emitter: EventEmitter = new EventEmitter();

module.exports = emitter;
