pub mod api_proxy;
pub mod cache_admin;
pub mod lazy_images;
pub mod metrics;
pub mod ssr;

pub use api_proxy::api_proxy_handler;
pub use cache_admin::invalidate_products_handler;
pub use lazy_images::lazy_images_handler;
pub use metrics::{metrics_handler, metrics_prometheus_handler};
pub use ssr::ssr_handler;
