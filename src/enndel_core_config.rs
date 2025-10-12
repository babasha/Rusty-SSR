/// Конфигурация сервера
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    pub v8_pool_size: usize,
}

impl ServerConfig {
    /// Создаёт конфигурацию по умолчанию
    pub fn default() -> Self {
        let num_cpus = num_cpus::get();
        let num_physical = num_cpus::get_physical();

        tracing::info!(
            "🖥️  Detected {} logical CPUs ({} physical cores)",
            num_cpus,
            num_physical
        );

        Self {
            host: "0.0.0.0".to_string(),
            port: 3000,
            v8_pool_size: num_cpus,
        }
    }

    /// Возвращает адрес для bind
    pub fn bind_address(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }
}
