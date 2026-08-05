.PHONY: help up down clean logs build test bench run image lint fmt

help:
	@echo "Targets:"
	@echo "  up       Start observability stack (Prometheus, Grafana, Jaeger, OTel)"
	@echo "  down     Stop containers (keeps volumes)"
	@echo "  clean    Stop and remove volumes"
	@echo "  build    Cargo build --release"
	@echo "  test     Cargo test (unit + property + integration)"
	@echo "  bench    Criterion benchmarks — HTML report at target/criterion/report/index.html"
	@echo "  run      Cargo run --release (order-book binary listening on :50051 gRPC and :9100 HTTP)"
	@echo "  image    Build a slim container image"
	@echo "  lint     Clippy + rustfmt --check"
	@echo "  fmt      Rustfmt"

up:
	docker compose up -d
	@echo "→ Grafana:    http://localhost:3000"
	@echo "→ Prometheus: http://localhost:9090"
	@echo "→ Jaeger:     http://localhost:16686"

down:
	docker compose down

clean:
	docker compose down -v

logs:
	docker compose logs -f --tail=100

build:
	cargo build --release

test:
	cargo test --all --all-features

bench:
	cargo bench --bench matching

run:
	cargo run --release

image:
	docker build -t order-book:local .

lint:
	cargo clippy --all-targets --all-features -- -D warnings
	cargo fmt --all -- --check

fmt:
	cargo fmt --all
