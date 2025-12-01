#!/usr/bin/env node
/**
 * Universal SSR Bundle Builder
 *
 * This script wraps your framework's SSR output into a bundle
 * compatible with Rusty SSR.
 *
 * Usage:
 *   node scripts/build-bundle.js <input> <output> [options]
 *
 * Examples:
 *   node scripts/build-bundle.js dist/server/entry.js ssr-bundle.js
 *   node scripts/build-bundle.js dist/server/entry.js ssr-bundle.js --iife SSRBundle
 *
 * Options:
 *   --iife <name>     IIFE global name if your bundle exports via window/globalThis
 *   --esm             Bundle is ES module format (uses default export)
 *   --render <name>   Name of render function in bundle (default: renderToString)
 *   --fn <name>       Name of global function to create (default: renderPage)
 */

import fs from 'fs';
import path from 'path';

// Parse arguments
const args = process.argv.slice(2);

if (args.length < 2 || args.includes('--help') || args.includes('-h')) {
    console.log(`
Rusty SSR Bundle Builder

Usage:
  node scripts/build-bundle.js <input> <output> [options]

Arguments:
  input     Path to your SSR bundle (from Vite/webpack/esbuild)
  output    Output path for Rusty SSR compatible bundle

Options:
  --iife <name>     IIFE global name (e.g., --iife SSRBundle)
  --render <name>   Render function name in bundle (default: renderToString)
  --fn <name>       Global function name to expose (default: renderPage)
  --help, -h        Show this help

Examples:
  # Wrap IIFE bundle (Preact/React)
  node scripts/build-bundle.js dist/server.js ssr-bundle.js --iife SSRBundle

  # Simple passthrough (already has globalThis.renderPage)
  node scripts/build-bundle.js dist/server.js ssr-bundle.js
`);
    process.exit(args.includes('--help') || args.includes('-h') ? 0 : 1);
}

const inputPath = args[0];
const outputPath = args[1];

// Parse options
const options = {
    iife: null,
    render: 'renderToString',
    fn: 'renderPage',
};

for (let i = 2; i < args.length; i++) {
    if (args[i] === '--iife' && args[i + 1]) {
        options.iife = args[++i];
    } else if (args[i] === '--render' && args[i + 1]) {
        options.render = args[++i];
    } else if (args[i] === '--fn' && args[i + 1]) {
        options.fn = args[++i];
    }
}

// Read input bundle
if (!fs.existsSync(inputPath)) {
    console.error(`Error: Input file not found: ${inputPath}`);
    process.exit(1);
}

const inputCode = fs.readFileSync(inputPath, 'utf-8');

// Generate wrapper
let outputCode;

if (options.iife) {
    // IIFE format: wrap and expose render function
    outputCode = `
// ============ SSR Bundle (IIFE wrapped) ============
${inputCode}
// ============ End SSR Bundle ============

// Expose render function globally for Rusty SSR
if (typeof ${options.iife} !== 'undefined' && typeof ${options.iife}.${options.render} === 'function') {
    globalThis.${options.fn} = ${options.iife}.${options.render};
    console.log('[Rusty SSR] Bundle loaded: ${options.iife}.${options.render} -> globalThis.${options.fn}');
} else {
    console.error('[Rusty SSR] Error: ${options.iife}.${options.render} not found in bundle');
}
`;
} else {
    // Check if bundle already has globalThis.renderPage
    if (inputCode.includes(`globalThis.${options.fn}`)) {
        // Passthrough - bundle is already compatible
        outputCode = inputCode;
        console.log(`Bundle already has globalThis.${options.fn}, using as-is`);
    } else {
        // Assume bundle exports the function directly
        outputCode = `
// ============ SSR Bundle ============
${inputCode}
// ============ End SSR Bundle ============

// Note: Ensure your bundle exposes globalThis.${options.fn}
// See examples/bundles/ for reference implementations
`;
    }
}

// Write output
const outputDir = path.dirname(outputPath);
if (outputDir && !fs.existsSync(outputDir)) {
    fs.mkdirSync(outputDir, { recursive: true });
}

fs.writeFileSync(outputPath, outputCode);

const inputSize = (inputCode.length / 1024).toFixed(2);
const outputSize = (outputCode.length / 1024).toFixed(2);

console.log(`
Rusty SSR Bundle Created
  Input:  ${inputPath} (${inputSize} KB)
  Output: ${outputPath} (${outputSize} KB)
  Global: ${options.fn}()
`);
