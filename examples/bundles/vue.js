/**
 * Example SSR bundle for Vue 3
 *
 * Build with Vite SSR mode.
 *
 * Usage:
 *   1. vite build --ssr src/entry-server.js
 *   2. Bundle should expose renderToString from @vue/server-renderer
 *   3. Create globalThis.renderPage wrapper
 */

// Your Vue app bundle would be here
// import { createSSRApp } from 'vue';
// import { renderToString } from '@vue/server-renderer';
// import App from './App.vue';

/**
 * Global render function called by Rust V8 engine
 *
 * @param {string} url - The URL path to render
 * @param {object|string} data - Data passed from Rust
 * @returns {string} Complete HTML document
 */
globalThis.renderPage = async function(url, data) {
    const props = typeof data === 'string' ? JSON.parse(data) : data;

    // Example: render Vue component
    // const app = createSSRApp(App, { url, ...props });
    // const html = await renderToString(app);

    const html = `<div id="app">
        <h1>Hello from Vue SSR</h1>
        <p>URL: ${url}</p>
        <p>Data: ${JSON.stringify(props)}</p>
    </div>`;

    return `<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Vue SSR App</title>
    <link rel="stylesheet" href="/assets/style.css">
</head>
<body>
    ${html}
    <script>window.__INITIAL_DATA__ = ${JSON.stringify(props)}</script>
    <script type="module" src="/assets/client.js"></script>
</body>
</html>`;
};

console.log('[Vue SSR] Bundle loaded');
