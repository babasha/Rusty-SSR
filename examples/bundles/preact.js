/**
 * Example SSR bundle for Preact
 *
 * Build with Vite/esbuild in IIFE format, then wrap with this pattern.
 *
 * Usage:
 *   1. Build your Preact app: vite build --ssr
 *   2. Bundle output should expose renderToString
 *   3. Create globalThis.renderPage wrapper
 */

// Your Preact app bundle would be here (from vite build --ssr)
// import { h } from 'preact';
// import renderToString from 'preact-render-to-string';
// import App from './App';

/**
 * Global render function called by Rust V8 engine
 *
 * @param {string} url - The URL path to render (e.g., "/", "/products/123")
 * @param {object|string} data - Data passed from Rust (parsed JSON or string)
 * @returns {string} Complete HTML document
 */
globalThis.renderPage = async function(url, data) {
    // Parse data if it's a string
    const props = typeof data === 'string' ? JSON.parse(data) : data;

    // Example: render Preact component
    // const html = renderToString(<App url={url} {...props} />);

    // For demo purposes, return static HTML
    const html = `<div id="app">
        <h1>Hello from Preact SSR</h1>
        <p>URL: ${url}</p>
        <p>Data: ${JSON.stringify(props)}</p>
    </div>`;

    // Return complete HTML document
    return `<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Preact SSR App</title>
    <!-- Add your CSS here -->
    <link rel="stylesheet" href="/assets/style.css">
</head>
<body>
    ${html}
    <!-- Hydration data -->
    <script>window.__INITIAL_DATA__ = ${JSON.stringify(props)}</script>
    <!-- Client bundle -->
    <script type="module" src="/assets/client.js"></script>
</body>
</html>`;
};

console.log('[Preact SSR] Bundle loaded');
