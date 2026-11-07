/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 *
 * @format
 */

'use strict';

const fs = require('fs');
const os = require('os');
const path = require('path');
const {execSync} = require('child_process');

const packageDir = path.join(__dirname, '..');
const distDir = path.join(packageDir, 'dist');
const binUpgradeJs = path.join(distDir, 'bin/upgrade.js');
const binCodemodJs = path.join(distDir, 'bin/runSpecificCodemod.js');

function ensureDependencies() {
  if (fs.existsSync(path.join(packageDir, 'node_modules/.bin/babel'))) {
    return;
  }

  execSync(
    'yarn install --frozen-lockfile --offline --ignore-scripts --non-interactive',
    {
      cwd: packageDir,
      stdio: 'inherit',
    },
  );
}

function buildPackage() {
  ensureDependencies();
  execSync('yarn build', {
    cwd: packageDir,
    stdio: 'inherit',
  });
}

describe('build output', () => {
  beforeAll(() => {
    // Ensure dist is built before running tests
    buildPackage();
  });

  test('dist directory exists after build', () => {
    expect(fs.existsSync(distDir)).toBe(true);
  });

  test('bin/upgrade.js uses CommonJS syntax (not ESM)', () => {
    const content = fs.readFileSync(binUpgradeJs, 'utf8');

    // Should use CommonJS require()
    expect(content).toContain('require(');

    // Should NOT have ESM import statements
    // This regex matches: import ... from '...' or import ... from "..."
    const esmImportRegex = /^import\s+.+\s+from\s+['"].+['"]/m;
    expect(content).not.toMatch(esmImportRegex);
  });

  test('bin/runSpecificCodemod.js uses CommonJS syntax (not ESM)', () => {
    const content = fs.readFileSync(binCodemodJs, 'utf8');

    expect(content).toContain('require(');

    const esmImportRegex = /^import\s+.+\s+from\s+['"].+['"]/m;
    expect(content).not.toMatch(esmImportRegex);
  });

  test('upgrade.js CLI runs successfully', () => {
    const result = execSync('node dist/bin/upgrade.js --help', {
      cwd: packageDir,
      encoding: 'utf8',
    });
    expect(result).toContain('Usage:');
    expect(result).toContain('flow-upgrade');
  });

  test('flow-codemod CLI runs successfully', () => {
    const result = execSync('node dist/bin/runSpecificCodemod.js --help', {
      cwd: packageDir,
      encoding: 'utf8',
    });
    expect(result).toContain('Usage:');
    expect(result).toContain('codemod');
  });
});

describe('packaged installation', () => {
  let tempDir;
  let tarballPath;

  beforeAll(() => {
    // Build and pack the package
    buildPackage();

    // Create tarball
    execSync('yarn pack --filename flow-upgrade-test.tgz', {
      cwd: packageDir,
      encoding: 'utf8',
    });
    tarballPath = path.join(packageDir, 'flow-upgrade-test.tgz');

    // Create temp directory for test installation
    tempDir = fs.mkdtempSync(path.join(os.tmpdir(), 'flow-upgrade-test-'));

    // Extract tarball directly instead of using yarn add (avoids network calls)
    execSync(`tar -xzf ${tarballPath} -C ${tempDir}`, {stdio: 'inherit'});

    // The tarball extracts to a 'package' directory
    const extractedDir = path.join(tempDir, 'package');

    const manifest = JSON.parse(
      fs.readFileSync(path.join(extractedDir, 'package.json'), 'utf8'),
    );
    const dependencies = Object.keys({
      ...manifest.dependencies,
      ...manifest.peerDependencies,
    });
    const dstNodeModules = path.join(extractedDir, 'node_modules');
    for (const dependency of dependencies) {
      const dependencyManifest = require.resolve(`${dependency}/package.json`, {
        paths: [packageDir],
      });
      const destination = path.join(dstNodeModules, dependency);
      fs.mkdirSync(path.dirname(destination), {recursive: true});
      fs.symlinkSync(path.dirname(dependencyManifest), destination, 'junction');
    }

    // Update tempDir to point to the extracted package
    tempDir = extractedDir;
  });

  afterAll(() => {
    // Cleanup
    if (tempDir == null) {
      return;
    }

    const parentDir = path.dirname(tempDir);
    if (
      parentDir &&
      fs.existsSync(parentDir) &&
      parentDir.includes('flow-upgrade-test-')
    ) {
      fs.rmSync(parentDir, {recursive: true, force: true});
    }
    if (tarballPath && fs.existsSync(tarballPath)) {
      fs.unlinkSync(tarballPath);
    }
  });

  test('flow-upgrade CLI works when installed as a package', () => {
    const result = execSync('node dist/bin/upgrade.js --help', {
      cwd: tempDir,
      encoding: 'utf8',
    });
    expect(result).toContain('Usage:');
    expect(result).toContain('flow-upgrade');
  });

  test('flow-codemod CLI works when installed as a package', () => {
    const result = execSync('node dist/bin/runSpecificCodemod.js --help', {
      cwd: tempDir,
      encoding: 'utf8',
    });
    expect(result).toContain('Usage:');
    expect(result).toContain('codemod');
  });
});
