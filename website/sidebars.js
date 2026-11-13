/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 *
 * @noflow
 * @format
 */

/*
 * We should explicitly list all the labels below, so that src/js/docs-categories.js that generates
 * the /en/docs page will automatically work.
 *
 * For categories, the first item must be the index page.
 */
module.exports = {
  docsSidebar: [
    {
      type: 'doc',
      id: 'getting-started',
      label: 'Get Started',
    },
    {
      type: 'doc',
      id: 'flow-vs-typescript',
      label: 'Flow for TypeScript Users',
    },
    {
      type: 'category',
      label: 'Language Guide',
      items: [
        {
          type: 'category',
          label: 'Types',
          items: [
            'types/index',
            'types/primitives',
            'types/literals',
            'types/unknown',
            'types/empty',
            'types/any',
            'types/maybe',
            'types/objects',
            'types/arrays',
            'types/tuples',
            'types/functions',
            'types/classes',
            'types/aliases',
            'types/opaque-types',
            'types/interfaces',
            'types/generics',
            'types/unions',
            'types/intersections',
            'types/indexed-access',
            'types/conditional',
            'types/template-literal-types',
            'types/mapped-types',
            'types/type-guards',
            'types/assertion-functions',
            'types/typeof',
            'types/casting',
            'types/const-expression',
            'types/utilities',
            'types/modules',
            'types/comments',
          ],
        },
        {
          type: 'category',
          label: 'Narrowing & Safety',
          items: [
            'lang/refinements',
            'lang/types-and-expressions',
            'lang/variables',
            'lang/annotation-requirement',
          ],
        },
        {
          type: 'category',
          label: 'Type System Deep Dive',
          items: [
            'lang/subtypes',
            'lang/type-hierarchy',
            'lang/variance',
            'lang/nominal-structural',
            'lang/depth-subtyping',
            'lang/declaration-merging',
          ],
        },
        {
          type: 'category',
          label: 'Enums',
          items: [
            'enums/index',
            'enums/defining-enums',
            'enums/using-enums',
            'enums/migrating-legacy-patterns',
          ],
        },
        {
          type: 'category',
          label: 'Match',
          items: ['match/index', 'match/patterns', 'match/migration'],
        },
        {
          type: 'doc',
          id: 'declarations/index',
          label: 'Declarations',
        },
      ],
    },
    {
      type: 'category',
      label: 'React',
      items: [
        'react/index',
        'react/component-syntax',
        'react/hook-syntax',
        'react/component-types',
        'react/render-types',
        'react/events',
        'react/refs',
        'react/types',
        process.env.INTERNAL_STATIC_DOCS && {
          type: 'category',
          label: 'Multiplatform',
          collapsed: true,
          items: ['react/multiplatform'],
        },
        {
          type: 'category',
          label: 'Legacy Patterns',
          collapsed: true,
          items: ['react/function-and-class-components', 'react/hoc'],
        },
      ].filter(Boolean),
    },
    {
      type: 'category',
      label: 'Linting',
      items: [
        'linting/index',
        'linting/flowlint-comments',
        'linting/rule-reference',
        'linting/strict-import-export-lints',
        {
          type: 'doc',
          id: 'errors/index',
          label: 'Error Suppressions',
        },
      ],
    },
    {
      type: 'category',
      label: 'Tooling',
      items: [
        'editors/index',
        'tools/babel',
        'tools/swc',
        'tools/prettier',
        'tools/eslint',
        'tools/flow-remove-types',
      ],
    },
    {
      type: 'category',
      label: 'Reference',
      items: [
        {
          type: 'category',
          label: 'Configuration',
          items: [
            {type: 'doc', id: 'config/index', label: 'Overview'},
            {type: 'doc', id: 'config/options', label: 'Options'},
            {type: 'doc', id: 'config/version', label: 'Version'},
            {type: 'doc', id: 'config/include', label: 'Include'},
            {type: 'doc', id: 'config/ignore', label: 'Ignore'},
            {type: 'doc', id: 'config/untyped', label: 'Untyped'},
            {type: 'doc', id: 'config/declarations', label: 'Declarations'},
            {type: 'doc', id: 'config/libs', label: 'Libs'},
            {type: 'doc', id: 'config/lints', label: 'Lints'},
          ],
        },
        {
          type: 'category',
          label: 'CLI',
          items: ['cli/index', 'cli/apply-code-action', 'cli/coverage'],
        },
        {
          type: 'doc',
          id: 'strict/index',
          label: 'Flow Strict',
        },
        {
          type: 'category',
          label: 'Library Definitions',
          items: ['libdefs/index', 'libdefs/creation'],
        },
        {
          type: 'doc',
          id: 'modernizing-legacy-syntax',
          label: 'Modernizing Legacy Syntax',
        },
        {
          type: 'doc',
          id: 'glossary',
          label: 'Glossary',
        },
      ],
    },
    {
      type: 'doc',
      id: 'faq',
      label: 'FAQ',
    },
  ],
};
