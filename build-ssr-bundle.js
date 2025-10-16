/**
 * SSR Bundle Builder
 *
 * Этот скрипт объединяет SSR entry point (IIFE формат) и создаёт
 * глобальную функцию renderPage() для вызова из Rust через V8.
 *
 * Требования:
 * - SSR должен быть собран в IIFE формате (vite.config.ts: output.format = 'iife')
 * - IIFE экспортирует renderToString функцию
 *
 * Использование:
 *   node build-ssr-bundle.js
 */

import fs from 'fs';
import path from 'path';
import { fileURLToPath } from 'url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));

// Читаем SSR entry (IIFE формат из Vite)
const ssrCode = fs.readFileSync(path.join(__dirname, '../EnndelClient/dist/server/server-entry.js'), 'utf-8');

// Создаём обёртку которая экспортирует renderToString глобально
const wrappedBundle = `
// ============ SSR Bundle Start ============
${ssrCode}
// ============ SSR Bundle End ============

// Экспортируем renderToString глобально для Rust
globalThis.renderToString = SSRBundle.renderToString;

// Глобальная функция для рендеринга (вызывается из Rust)
globalThis.renderPage = async function(url, productsData) {
    try {
        // Вызываем рендеринг
        const context = {
            url: url,
            headers: {},
            userAgent: 'Rust-V8-SSR/1.0',
            productsData: productsData || []
        };

        const result = await SSRBundle.renderToString(context);

        // Формируем полный HTML
        const html = \`<!DOCTYPE html>
<html lang="ru">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Enddel - Магазин</title>
    <link rel="stylesheet" href="/assets/index-CXxKVYtV.css">
    \${result.head || ''}
</head>
<body>
    <div id="app" data-preact-root>\${result.html}</div>
    <script>window.__INITIAL_DATA__ = \${JSON.stringify(result.initialData)}</script>
    <script type="module" src="/assets/index-sTKbqqrz.js"></script>
</body>
</html>\`;

        return html;
    } catch (error) {
        console.error('SSR Error:', error);
        throw error;
    }
};

console.log('✅ SSR bundle loaded and ready');
`;

// Сохраняем
const outputPath = path.join(__dirname, 'ssr-bundle-embedded.js');
fs.writeFileSync(outputPath, wrappedBundle);

console.log(`✅ SSR bundle created: ${outputPath}`);
console.log(`📦 Size: ${(wrappedBundle.length / 1024).toFixed(2)} KB`);
