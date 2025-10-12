use std::sync::{mpsc, Arc, Mutex};
use tokio::sync::oneshot;
use std::thread;

use super::{renderer, runtime};

/// Конфигурация V8 thread pool
#[derive(Debug, Clone)]
pub struct AdaptivePoolConfig {
    /// Количество потоков (обычно = CPU cores)
    pub num_threads: usize,
}

impl Default for AdaptivePoolConfig {
    fn default() -> Self {
        let num_cpus = num_cpus::get();
        Self {
            num_threads: num_cpus,
        }
    }
}

/// Запрос на рендеринг
struct RenderRequest {
    url: String,
    response_tx: oneshot::Sender<Result<String, String>>,
}

/// Адаптивный пул V8 isolate с динамическим масштабированием
pub struct AdaptiveV8Pool {
    config: AdaptivePoolConfig,
    request_tx: mpsc::SyncSender<RenderRequest>,
    request_rx: Arc<Mutex<mpsc::Receiver<RenderRequest>>>,
    worker_count: Arc<Mutex<usize>>,
}

impl AdaptiveV8Pool {
    /// Создаёт новый V8 thread pool
    pub fn new(config: AdaptivePoolConfig) -> Self {
        tracing::info!(
            "🔧 Creating V8 pool with {} threads",
            config.num_threads
        );

        // Bounded channel с размером очереди 100
        let (request_tx, request_rx) = mpsc::sync_channel(100);
        let request_rx = Arc::new(Mutex::new(request_rx));
        let worker_count = Arc::new(Mutex::new(0));

        let pool = Self {
            config: config.clone(),
            request_tx,
            request_rx: Arc::clone(&request_rx),
            worker_count: Arc::clone(&worker_count),
        };

        // Создаём фиксированное количество воркеров
        for i in 0..config.num_threads {
            pool.spawn_worker(i);
        }

        tracing::info!("✅ Started {} workers", config.num_threads);

        pool
    }

    /// Создаёт нового воркера
    fn spawn_worker(&self, id: usize) {
        let request_rx = Arc::clone(&self.request_rx);
        let worker_count = Arc::clone(&self.worker_count);

        // Увеличиваем счётчик воркеров
        {
            let mut count = worker_count.lock().unwrap();
            *count += 1;
        }

        thread::spawn(move || {
            tracing::debug!("🟢 Worker {} started", id);

            // Инициализируем V8 runtime для этого потока
            if let Err(e) = runtime::init_runtime() {
                tracing::error!("❌ Failed to initialize V8 runtime for worker {}: {}", id, e);
                let mut count = worker_count.lock().unwrap();
                *count -= 1;
                return;
            }

            let mut requests_processed = 0usize;

            // Основной цикл воркера - просто блокирующий wait без timeout
            loop {
                let request = {
                    let rx = request_rx.lock().unwrap();

                    // Блокирующий recv - ждём пока не придёт запрос
                    match rx.recv() {
                        Ok(req) => Some(req),
                        Err(_) => {
                            tracing::debug!("🔴 Worker {} channel disconnected", id);
                            break;
                        }
                    }
                };

                if let Some(req) = request {
                    // Обрабатываем запрос
                    let result = runtime::with_runtime(|js_runtime| {
                        renderer::render_html(&req.url, js_runtime)
                    });

                    // Отправляем результат
                    let _ = req.response_tx.send(result);

                    // Обновляем статистику
                    requests_processed += 1;
                }
            }

            // Воркер завершается
            tracing::debug!(
                "🔴 Worker {} stopped (processed {} requests)",
                id,
                requests_processed
            );

            // Уменьшаем счётчик воркеров
            let mut count = worker_count.lock().unwrap();
            *count -= 1;
        });
    }

    /// Рендерит HTML через пул
    pub async fn render(&self, url: String) -> Result<String, String> {
        // Создаём канал для ответа
        let (response_tx, response_rx) = oneshot::channel();

        // Отправляем запрос в пул (synchronous send)
        self.request_tx
            .send(RenderRequest { url, response_tx })
            .map_err(|_| "Failed to send render request".to_string())?;

        // Ждём ответа (async recv)
        response_rx
            .await
            .map_err(|_| "Failed to receive render response".to_string())?
    }

    /// Возвращает текущее количество активных воркеров
    pub fn worker_count(&self) -> usize {
        *self.worker_count.lock().unwrap()
    }
}

impl Drop for AdaptiveV8Pool {
    fn drop(&mut self) {
        tracing::info!("🛑 Shutting down adaptive V8 pool");
        // Каналы автоматически закроются, воркеры завершатся
    }
}
