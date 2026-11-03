/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 *
 * @format
 */

const {fbContent} = require('docusaurus-plugin-internaldocs-fb/internal');
const webpack = require('webpack');
const MonacoWebpackPlugin = require('monaco-editor-webpack-plugin');
const allFlowVersions = require('./src/js/flow-versions');

const defaultFlowVersion =
  allFlowVersions.find(version => version !== 'master') || 'master';

/** @type {import('@docusaurus/types').DocusaurusConfig} */
module.exports = async () => {
  const flowCheckRemarkPlugin = (
    await import('./src/js/flow-check-remark-plugin.mjs')
  ).default;
  return {
    title: 'Flow',
    tagline: 'A Static Type Checker for JavaScript',
    url: 'https://flow.org',
    baseUrl: process.env.DOCUSAURUS_BASE_URL || '/',
    onBrokenLinks: 'throw',
    onBrokenMarkdownLinks: 'warn',
    favicon: 'img/favicon.png',
    organizationName: 'meta', // Usually your GitHub org/user name.
    projectName: 'flow', // Usually your repo name.
    trailingSlash: true,
    markdown: {
      mermaid: true,
    },
    themes: ['@docusaurus/theme-mermaid'],
    plugins: [
      function polyfillNodeBuiltinsForFlowJS(context, options) {
        return {
          name: 'polyfillNodeBuiltinsForFlowJS',
          configureWebpack(_config, isServer) {
            return {
              resolve: {fallback: {fs: false, constants: false, crypto: false}},
              plugins: isServer
                ? []
                : [
                    new webpack.ProvidePlugin({
                      Buffer: ['buffer', 'Buffer'],
                      process: require.resolve('process/browser'),
                    }),
                  ],
            };
          },
        };
      },
      function enableWasm() {
        return {
          name: 'enableWasm',
          configureWebpack() {
            return {
              experiments: {syncWebAssembly: true},
              module: {
                rules: [
                  {
                    test: /\.wasm$/i,
                    loader: 'file-loader',
                    type: 'javascript/auto',
                  },
                ],
              },
            };
          },
        };
      },
      function enableSomeEnvVarsAsBuildTimeConstants() {
        return {
          name: 'enableSomeEnvVarsAsBuildTimeConstants',
          configureWebpack() {
            return {
              plugins: [
                new webpack.EnvironmentPlugin({
                  INTERNAL_STATIC_DOCS:
                    process.env.INTERNAL_STATIC_DOCS || false,
                }),
              ],
            };
          },
        };
      },
      function enableMonacoEditorPlugin() {
        return {
          name: 'enableMonacoEditorPlugin',
          configureWebpack() {
            return {
              // https://stackoverflow.com/questions/69265357/monaco-editor-worker
              plugins: [new MonacoWebpackPlugin()],
            };
          },
        };
      },
    ],
    themeConfig: {
      // Default Open Graph / Twitter share image for every page. Individual
      // pages can override via the `image:` frontmatter field (the homepage
      // does, in src/pages/index.mdx). Path is relative to website/static/.
      image: 'img/og-share.png',
      metadata: [{name: 'twitter:card', content: 'summary_large_image'}],
      algolia: process.env.INTERNAL_STATIC_DOCS
        ? undefined
        : {
            appId: 'P6T3E8XPGT',
            apiKey: '01f111c0b2980e54f1307e982fa2c218',
            indexName: 'flow',
            contextualSearch: true,
          },
      prism: {
        theme: require('prism-react-renderer').themes.github,
        // Dark code blocks matching the landing page, which renders its
        // flow-check blocks with vsDark (see landing/Showcase.js). In dark mode
        // Docusaurus uses darkTheme, so the whole site now shares one code look.
        darkTheme: require('prism-react-renderer').themes.vsDark,
      },
      colorMode: {
        // Dark by default to match the redesigned (dark) landing page. The
        // toggle is enabled so readers can switch to the light theme; we don't
        // follow the OS preference so the dark default is what everyone sees
        // first. (The Try playground stays light in both modes — see TryFlow.)
        defaultMode: 'dark',
        disableSwitch: false,
        respectPrefersColorScheme: false,
      },
      navbar: {
        logo: {
          alt: 'Flow Logo',
          src: 'img/logo.svg',
        },
        items: [
          {
            to: 'en/docs/getting-started',
            activeBasePath: 'en/docs/getting-started',
            label: 'Getting Started',
            position: 'left',
          },
          {
            to: 'en/docs/',
            // getting-started lives under en/docs but has its own nav item, so
            // exclude it here — otherwise both "Getting Started" and "Docs"
            // light up as active on the getting-started page.
            activeBaseRegex: '^/en/docs/(?!getting-started)',
            label: 'Docs',
            position: 'left',
          },
          process.env.INTERNAL_STATIC_DOCS
            ? {to: 'https://flow.org/try', label: 'Try', position: 'left'}
            : {
                to: 'try/',
                activeBasePath: 'try',
                label: 'Try',
                position: 'left',
              },
          {
            to: 'blog/',
            label: 'Blog',
            position: 'left',
          },
          {
            href: `https://github.com/meta/flow/releases/tag/${defaultFlowVersion}`,
            // `defaultFlowVersion` falls back to 'master' when past releases
            // aren't included in the build; show it as 'main' to match the
            // renamed GitHub branch (and the Try Flow version selector).
            label: defaultFlowVersion.replace('master', 'main'),
            position: 'right',
            className: 'navbar__version-pill',
          },
          {
            href: 'https://x.com/flowtype',
            'aria-label': 'Flow on X (formerly Twitter)',
            position: 'right',
            className: 'navbar__icon twitter__link',
          },
          // Please keep GitHub link to the right for consistency.
          {
            href: 'https://github.com/meta/flow',
            'aria-label': 'GitHub',
            position: 'right',
            className: 'navbar__icon github__link',
          },
        ],
      },
      footer: {
        style: 'dark',
        links: [
          {
            title: 'Learn',
            items: [
              {
                label: 'Getting Started',
                to: 'en/docs/getting-started',
              },
              {
                label: 'Flow for TypeScript Users',
                to: 'en/docs/flow-vs-typescript',
              },
            ],
          },
          {
            title: 'Community',
            items: [
              {
                label: 'X',
                href: 'https://x.com/flowtype',
              },
              {
                label: 'Discord',
                href: 'https://discordapp.com/invite/8ezwRUK',
              },
            ],
          },
          {
            title: 'More',
            items: [
              {
                label: 'Medium',
                to: 'https://medium.com/flow-type',
              },
              {
                label: 'GitHub',
                href: 'https://github.com/meta/flow',
              },
            ],
          },
          {
            title: 'Legal',
            // Please do not remove the privacy and terms, it's a legal requirement.
            items: [
              {
                label: 'Privacy',
                href: 'https://opensource.facebook.com/legal/privacy/',
              },
              {
                label: 'Terms',
                href: 'https://opensource.facebook.com/legal/terms/',
              },
              {
                label: 'Data Policy',
                href: 'https://opensource.facebook.com/legal/data-policy/',
              },
              {
                label: 'Cookie Policy',
                href: 'https://opensource.facebook.com/legal/cookie-policy/',
              },
            ],
          },
        ],
        logo: {
          alt: 'Facebook Open Source Logo',
          src: 'img/oss_logo.png',
          href: 'https://opensource.facebook.com',
        },
        // Please do not remove the credits, help to publicize Docusaurus :)
        copyright: `Copyright © ${new Date().getFullYear()} Meta Platforms, Inc. Built with Docusaurus.`,
      },
    },
    customFields: {
      /* The latest release might be unavailable if INCLUDE_PAST_RELEASES env var
       is not on. In that case, we default to master. */
      flowVersion: defaultFlowVersion,
      allFlowVersions,
      fbRepoName: 'fbsource',
    },
    presets: [
      [
        require.resolve('docusaurus-plugin-internaldocs-fb/docusaurus-preset'),
        {
          docs: {
            routeBasePath: 'en/docs',
            sidebarPath: require.resolve('./sidebars.js'),
            // Docusaurus shells out to `git log` to get the timestamp; works for
            // the public GitHub-driven build but throws in fbsource (Sapling, no
            // `.git`), which would break the internal staticdocs build.
            showLastUpdateTime: !process.env.INTERNAL_STATIC_DOCS,
            editUrl: fbContent({
              internal:
                'https://www.internalfb.com/code/fbsource/fbcode/flow/website/',
              external: 'https://github.com/meta/flow/edit/main/website/',
            }),
            remarkPlugins: [
              [require('@docusaurus/remark-plugin-npm2yarn'), {sync: true}],
              flowCheckRemarkPlugin,
            ],
          },
          staticDocsProject: 'flow',
          blog: {
            path: 'blog',
            postsPerPage: 50,
            showReadingTime: false,
            blogSidebarCount: 'ALL',
            blogSidebarTitle: 'All our posts',
          },
          pages: {
            // Same flow-check transform as docs, so `js flow-check` blocks in
            // src/pages/*.mdx (e.g. the landing page) route through the
            // FlowCheckCodeBlock component and pick up its inline-error UI and
            // landing-only dark Prism theme.
            remarkPlugins: [flowCheckRemarkPlugin],
          },
          theme: {
            customCss: require.resolve('./src/css/custom.css'),
          },
          googleAnalytics: process.env.INTERNAL_STATIC_DOCS
            ? undefined
            : {trackingID: 'UA-49208336-4', anonymizeIP: true},
        },
      ],
    ],
  };
};
