/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 *
 * @flow
 * @format
 */

import type {FlowResult} from '../flowResult';
import type {AllInvocations} from './ShellMocker';
import type {LSPMessage} from './lsp';

export interface StepEnvWriteable {
  reportStdout(output: string): void;
  reportStderr(output: string): void;
  reportExitCode(code: number | string | null): void;
  setLSPMessagesSinceStartOfStep(messages: Array<LSPMessage>): void;
  setLSPStderrSinceStartOfStep(stderr: string): void;
  setMockInvocationsSinceStartOfStep(invocations: AllInvocations): void;
  setNewErrors(errors: FlowResult): void;
  setServerRunning(running: 'stopped' | 'running'): void;
  setLSPRunning(running: 'stopped' | 'running'): void;
  setProjectDir(dir: string): void;
  triggerFlowCheck(): void;
}

export interface StepEnvReadable {
  getStdout(): string;
  getStderr(): string;
  getExitCodes(): Array<number | string | null>;
  getLSPMessagesSinceStartOfStep(): Array<LSPMessage>;
  getLSPStderrSinceStartOfStep(): string;
  getMockInvocationsSinceStartOfStep(): AllInvocations;
  getOldErrors(): FlowResult;
  getNewErrors(): FlowResult;
  getServerRunning(): 'stopped' | 'running';
  getLSPRunning(): 'stopped' | 'running';
  getProjectDir(): string;
  // What `labels.write` ops have recorded so far in the current test.
  getLabels(): Map<string, string>;
  shouldRunFlow(): boolean;
}

function newEnv(
  oldErrors: FlowResult,
  labels: Map<string, string>,
): {
  envWrite: StepEnvWriteable,
  envRead: StepEnvReadable,
  ...
} {
  let stdout: Array<string> = [];
  let stderr: Array<string> = [];
  let exitCodes: Array<number | string | null> = [];
  let newErrors = oldErrors;
  let serverRunning: 'running' | 'stopped' = 'stopped';
  let lspRunning: 'running' | 'stopped' = 'stopped';
  let shouldRunFlow = false;
  let lspMessagesSinceStartOfStep: Array<LSPMessage> = [];
  let lspStderrSinceStartOfStep = '';
  let mockInvocationsSinceStartOfStep: AllInvocations = {};
  let projectDir = '';

  const envWrite: StepEnvWriteable = {
    reportStdout(output: string) {
      stdout.push(output);
    },

    reportStderr(output: string) {
      stderr.push(output);
    },

    reportExitCode(code: number | string | null) {
      exitCodes.push(code);
    },

    setLSPMessagesSinceStartOfStep(messages: Array<LSPMessage>) {
      lspMessagesSinceStartOfStep = messages;
    },

    setLSPStderrSinceStartOfStep(stderr: string) {
      lspStderrSinceStartOfStep = stderr;
    },

    setMockInvocationsSinceStartOfStep(invocations: AllInvocations) {
      mockInvocationsSinceStartOfStep = invocations;
    },

    setNewErrors(errors: FlowResult) {
      newErrors = errors;
    },

    setServerRunning(running: 'stopped' | 'running') {
      serverRunning = running;
    },

    setLSPRunning(running: 'stopped' | 'running') {
      lspRunning = running;
    },

    setProjectDir(dir: string) {
      projectDir = dir;
    },

    triggerFlowCheck() {
      shouldRunFlow = true;
    },
  };

  const envRead: StepEnvReadable = {
    getStdout() {
      return stdout.join('\n');
    },

    getStderr() {
      return stderr.join('\n');
    },

    getExitCodes() {
      return exitCodes.slice();
    },

    getLSPMessagesSinceStartOfStep() {
      return lspMessagesSinceStartOfStep;
    },

    getLSPStderrSinceStartOfStep() {
      return lspStderrSinceStartOfStep;
    },

    getMockInvocationsSinceStartOfStep() {
      return mockInvocationsSinceStartOfStep;
    },

    getOldErrors() {
      return oldErrors;
    },

    getNewErrors() {
      return newErrors;
    },

    getServerRunning() {
      return serverRunning;
    },

    getLSPRunning() {
      return lspRunning;
    },

    getProjectDir() {
      return projectDir;
    },

    getLabels() {
      return labels;
    },

    shouldRunFlow() {
      return shouldRunFlow;
    },
  };

  return {envWrite, envRead};
}

module.exports = {
  newEnv,
};
