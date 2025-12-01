/**
 * Example SSR bundle for Solid.js
 *
 * Build with Vite + vite-plugin-solid.
 *
 * Usage:
 *   1. Configure vite for SSR with solid-js
 *   2. Use renderToString from 'solid-js/web'
 *   3. Create globalThis.renderPage wrapper
 */

// Your Solid app bundle would be here
// import { renderToString } from 'solid-js/web';
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

    // Example: render Solid component
    // const html = await renderToString(() => <App url={url} {...props} />);

    const html = `<div id="app">
        <h1>Hello from Solid SSR</h1>
        <p>URL: ${url}</p>
        <p>Data: ${JSON.stringify(props)}</p>
    </div>`;

    return `<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Solid SSR App</title>
    <link rel="stylesheet" href="/assets/style.css">
</head>
<body>
    ${html}
    <script>window.__INITIAL_DATA__ = ${JSON.stringify(props)}</script>
    <script type="module" src="/assets/client.js"></script>
</body>
</html>`;
};

console.log('[Solid SSR] Bundle loaded');
