// Copyright (c) 2023 The Jaeger Authors
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

import transformTraceData from '../../../../model/transform-trace-data';

/*
   ┌─────────────────┐                    |
   │     Span A      │                    |                         spanA
   └───────┬─────────┘                    |                          /   
           │                              |                         /            
          ┌▼──────────────┐               |                       spanB  
          │    Span B     │               |                       /
          └─────┬─────────┘               |                      /   
                │                         |                     spanC
               ┌▼─────────────┐           |
               │    Span C    │           |             ((parent-child tree))
               └──────────────┘           |
*/

const trace = {
  traceID: 'trace-abc',
  spans: [
    {
      spanID: 'span-A',
      operationName: 'op-A',
      references: [],
      startTime: 1,
      duration: 29,
      processID: 'p1',
    },
    {
      spanID: 'span-B',
      operationName: 'op-B',
      references: [
        {
          refType: 'CHILD_OF',
          spanID: 'span-A',
        },
      ],
      startTime: 15,
      duration: 20,
      processID: 'p1',
    },
    {
      spanID: 'span-C',
      operationName: 'op-C',
      references: [
        {
          refType: 'CHILD_OF',
          spanID: 'span-B',
        },
      ],
      startTime: 20,
      duration: 20,
      processID: 'p1',
    },
  ],
  processes: {
    p1: {
      serviceName: 'service-one',
    },
  },
};

const transformedTrace = transformTraceData(trace);

const criticalPathSections = [
  {
    spanId: 'span-C',
    section_start: 20,
    section_end: 30,
  },
  {
    spanId: 'span-B',
    section_start: 15,
    section_end: 20,
  },
  {
    spanId: 'span-A',
    section_start: 1,
    section_end: 15,
  },
];

const test7 = {
  criticalPathSections,
  trace: transformedTrace,
};

export default test7;
