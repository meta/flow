// @flow
import type {SerializedEmojiNode, SerializedTextNode} from './lib';

declare const emoji: SerializedEmojiNode;
const text: SerializedTextNode = emoji; // tsc: ok
