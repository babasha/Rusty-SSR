use std::sync::OnceLock;

/// Кэш SSR бандла (читаем файл только 1 раз)
static SSR_BUNDLE: OnceLock<String> = OnceLock::new();

/// Инициализирует SSR бандл (загружает из файла один раз)
pub fn init_bundle() {
    SSR_BUNDLE.get_or_init(|| {
        std::fs::read_to_string("ssr-bundle-embedded.js").expect("Failed to read SSR bundle")
    });
}

/// Возвращает кэшированный SSR бандл
pub fn get_bundle() -> &'static str {
    SSR_BUNDLE.get().expect("Bundle not initialized")
}
