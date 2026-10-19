#!/usr/bin/env node

throw new Error(
  'Do not publish packages/native-platforms workspace stubs. Push a matching v* tag so release.yml builds and publishes the real native binary packages.',
)
