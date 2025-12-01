/**
 * Minimal SSR bundle example
 *
 * This is the simplest possible SSR bundle.
 * Use this as a starting point for any framework.
 */

/**
 * Global render function called by Rust V8 engine
 *
 * Contract:
 *   - Input: (url: string, data: string | object)
 *   - Output: string (complete HTML document)
 *
 * @param {string} url - The URL path to render (e.g., "/", "/about", "/products/123")
 * @param {object|string} data - JSON data passed from Rust via engine.render_with_data()
 * @returns {string} Complete HTML document ready to send to browser
 */
globalThis.renderPage = async function(url, data) {
    // Parse data if passed as JSON string
    const props = typeof data === 'string' ? JSON.parse(data) : (data || {});

    // Your rendering logic here
    // This could be any framework: React, Preact, Vue, Solid, Svelte, vanilla JS...

    return `<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Page: ${url}</title>
</head>
<body>
    <h1>Hello from Rusty SSR!</h1>
    <p>Rendered URL: ${url}</p>
    <pre>${JSON.stringify(props, null, 2)}</pre>
</body>
</html>`;
};

console.log('[Minimal SSR] Bundle loaded and ready');
