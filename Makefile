COMPOSE_DIR := deploy/docker
COMPOSE ?= docker compose
COMPOSE_ENV ?= ../../.env
ROOT_DIR := $(CURDIR)

.PHONY: start build-start build-target stop compose-up compose-down compose-smoke compose-test compose-logs gen-certs compose-golden-local compose-golden-prodlike compose-golden-down

start:
	cd $(COMPOSE_DIR) && $(COMPOSE) --env-file "$(COMPOSE_ENV)" up -d

build-start:
	cd $(COMPOSE_DIR) && $(COMPOSE) --env-file "$(COMPOSE_ENV)" up --build

build-target:
ifndef SERVICE
	$(error SERVICE is required. Usage: make build-target SERVICE=crawl4ai)
endif
	cd $(COMPOSE_DIR) && $(COMPOSE) --env-file "$(COMPOSE_ENV)" up -d --build $(SERVICE)

stop:
	cd $(COMPOSE_DIR) && $(COMPOSE) --env-file "$(COMPOSE_ENV)" down

compose-up:
	cd $(COMPOSE_DIR) && $(COMPOSE) --env-file "$(COMPOSE_ENV)" up --build

compose-down:
	cd $(COMPOSE_DIR) && $(COMPOSE) --env-file "$(COMPOSE_ENV)" down

compose-smoke:
	cd $(COMPOSE_DIR) && $(COMPOSE) --env-file "$(COMPOSE_ENV)" -f docker-compose.smoke.yml up -d --build
	cd $(COMPOSE_DIR) && $(COMPOSE) --env-file "$(COMPOSE_ENV)" -f docker-compose.smoke.yml run --rm smoke-tests
	cd $(COMPOSE_DIR) && $(COMPOSE) --env-file "$(COMPOSE_ENV)" -f docker-compose.smoke.yml down

compose-test:
	cd $(COMPOSE_DIR) && $(COMPOSE) --env-file "$(COMPOSE_ENV)" -f docker-compose.yml -f docker-compose.test.yml up -d --build
	cd $(COMPOSE_DIR) && $(COMPOSE) --env-file "$(COMPOSE_ENV)" -f docker-compose.yml -f docker-compose.test.yml run --rm smoke-tests
	cd $(COMPOSE_DIR) && $(COMPOSE) --env-file "$(COMPOSE_ENV)" -f docker-compose.yml -f docker-compose.test.yml down

compose-logs:
	cd $(COMPOSE_DIR) && $(COMPOSE) --env-file "$(COMPOSE_ENV)" logs -f $(SERVICE)

gen-certs:
	$(COMPOSE_DIR)/scripts/gen-certs.sh

compose-golden-local:
	PROFILE=golden-local $(ROOT_DIR)/tests/ops/golden-profile.sh verify

compose-golden-prodlike:
	PROFILE=golden-prodlike $(ROOT_DIR)/tests/ops/golden-profile.sh verify

compose-golden-down:
	PROFILE=golden-local $(ROOT_DIR)/tests/ops/golden-profile.sh down; PROFILE=golden-prodlike $(ROOT_DIR)/tests/ops/golden-profile.sh down
