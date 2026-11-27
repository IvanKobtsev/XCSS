// These tests start Vite or analyze full TypeScript projects on shared CI workers.
export const compilerTimeout = 15_000

// Configuration restarts include two 10-second polling windows plus setup/cleanup.
export const restartTimeout = 30_000
