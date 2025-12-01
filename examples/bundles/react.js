/**
 * Example SSR bundle for React
 *
 * Build with Vite/webpack in IIFE format.
 *
 * Usage:
 *   1. Build your React app for SSR
 *   2. Bundle should include react-dom/server
 *   3. Create globalThis.renderPage wrapper
 */

// Your React app bundle would be here
// import React from 'react';
// import { renderToString } from 'react-dom/server';
// import App from './App';

/**
 * Global render function called by Rust V8 engine
 *
 * @param {string} url - The URL path to render
 * @param {object|string} data - Data passed from Rust
 * @returns {string} Complete HTML document
 */
globalThis.renderPage = async function(url, data) {
    const props = typeof data === 'string' ? JSON.parse(data) : data;

    // Example: render React component
    // const html = renderToString(<App url={url} {...props} />);

    const html = `<div id="root">
        <h1>Hello from React SSR</h1>
        <p>URL: ${url}</p>
        <p>Data: ${JSON.stringify(props)}</p>
    </div>`;

    return `<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>React SSR App</title>
    <link rel="stylesheet" href="/assets/style.css">
</head>
<body>
    ${html}
    <script>window.__INITIAL_DATA__ = ${JSON.stringify(props)}</script>
    <script type="module" src="/assets/client.js"></script>
</body>
</html>`;
};

console.log('[React SSR] Bundle loaded');
