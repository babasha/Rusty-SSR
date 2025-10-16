#!/bin/bash
set -euo pipefail

SERVER_BIN="$(dirname "$0")/target/release/enddel-server"
SERVER_URL="http://127.0.0.1:3000"
MOCK_API_DIR="/tmp/enddel_bench_api"
MOCK_API_PORT=18001
MOCK_SERVER_LOG="/tmp/enddel_bench_mock.log"
APP_LOG="/tmp/enddel_bench_app.log"
WRK_OUT="/tmp/enddel_bench_wrk.out"

if ! command -v wrk >/dev/null 2>&1; then
  echo "wrk не найден. Установите wrk (brew install wrk) и повторите." >&2
  exit 1
fi

if [ ! -x "$SERVER_BIN" ]; then
  echo "Не найден исполняемый enddel-server ($SERVER_BIN). Соберите: cargo build --release" >&2
  exit 1
fi

cleanup() {
  if [ -n "${APP_PID:-}" ] && kill -0 "$APP_PID" 2>/dev/null; then
    kill "$APP_PID" 2>/dev/null || true
  fi
  if [ -n "${MOCK_PID:-}" ] && kill -0 "$MOCK_PID" 2>/dev/null; then
    kill "$MOCK_PID" 2>/dev/null || true
  fi
}
trap cleanup EXIT

rm -rf "$MOCK_API_DIR"
mkdir -p "$MOCK_API_DIR/api"
cat <<'JSON' >"$MOCK_API_DIR/api/products"
{"products":[{"id":1,"name":{"ru":"Тест"},"price":10.5,"unit":"kg","step":1.0,"stock_quantity":5,"category_id":1,"vendor_id":1,"slug":"test"}]}
JSON
python3 -m http.server "$MOCK_API_PORT" --directory "$MOCK_API_DIR" >"$MOCK_SERVER_LOG" 2>&1 &
MOCK_PID=$!
sleep 1

PRODUCT_API_BASE="http://127.0.0.1:${MOCK_API_PORT}/api" PRODUCT_LAZY_CACHE_CAPACITY=256 "$SERVER_BIN" >"$APP_LOG" 2>&1 &
APP_PID=$!

for attempt in {1..20}; do
  sleep 0.5
  if curl -s -o /dev/null "$SERVER_URL"; then
    break
  fi
  if ! kill -0 "$APP_PID" 2>/dev/null; then
    echo "Сервер упал, лог:" >&2
    cat "$APP_LOG" >&2
    exit 1
  fi
  if [ "$attempt" -eq 20 ]; then
    echo "Не удалось дождаться старта сервера." >&2
    exit 1
  fi
done

echo "Сервер запущен. Запускаем wrk..."
wrk -t12 -c1000 -d10s "$SERVER_URL" | tee "$WRK_OUT"

echo "===================================="
echo "Логи приложения (последние 20 строк):"
tail -n 20 "$APP_LOG" || true

echo "===================================="
echo "Логи mock API (последние 20 строк):"
tail -n 20 "$MOCK_SERVER_LOG" || true

echo "Результаты wrk сохранены в $WRK_OUT"
