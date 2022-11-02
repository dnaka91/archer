// Copyright (c) 2017 Uber Technologies, Inc.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

import deepFreeze from 'deep-freeze';

import { FALLBACK_DAG_MAX_NUM_SERVICES } from './index';
import getVersion from '../utils/version/get-version';

const { version } = require('../../package.json');

export default deepFreeze(
  Object.defineProperty(
    {
      archiveEnabled: true,
      dependencies: {
        dagMaxNumServices: FALLBACK_DAG_MAX_NUM_SERVICES,
        menuEnabled: true,
      },
      menu: [
        {
          label: 'About Jaeger',
          items: [
            {
              label: 'Website/Docs',
              url: 'https://dnaka91.github.io/archer',
            },
            {
              label: 'GitHub',
              url: 'https://github.com/dnaka91/archer',
            },
            {
              label: `Jaeger ${getVersion().gitVersion}`,
            },
            {
              label: `Commit ${getVersion().gitCommit.substring(0, 7)}`,
            },
            {
              label: `Build ${getVersion().buildDate}`,
            },
            {
              label: `Jaeger UI v${version}`,
            },
          ],
        },
      ],
      search: {
        maxLookback: {
          label: '2 Days',
          value: '2d',
        },
        maxLimit: 1500,
      },
      tracking: {
        gaID: null,
        trackErrors: true,
        customWebAnalytics: null,
      },
      linkPatterns: [],
      monitor: {
        menuEnabled: false,
        emptyState: {
          mainTitle: 'Get started with Service Performance Monitoring',
          subTitle:
            'A high-level monitoring dashboard that helps you cut down the time to identify and resolve anomalies and issues.',
          description:
            'Service Performance Monitoring aggregates tracing data into RED metrics and visualizes them in service and operation level dashboards.',
          button: {
            text: 'Read the Documentation',
            onClick: () => window.open('https://dnaka91.github.io/archer'),
          },
          alert: {
            message: 'Service Performance Monitoring requires a Prometheus-compatible time series database.',
            type: 'info',
          },
        },
        docsLink: 'https://dnaka91.github.io/archer',
      },
      deepDependencies: {
        menuEnabled: false,
      },
      qualityMetrics: {
        menuEnabled: false,
        menuLabel: 'Trace Quality',
      },
    },
    // fields that should be individually merged vs wholesale replaced
    '__mergeFields',
    { value: ['dependencies', 'search', 'tracking'] }
  )
);

export const deprecations = [
  {
    formerKey: 'dependenciesMenuEnabled',
    currentKey: 'dependencies.menuEnabled',
  },
  {
    formerKey: 'gaTrackingID',
    currentKey: 'tracking.gaID',
  },
];
