COMPOSE_DIR := deploy/docker
COMPOSE ?= docker compose
COMPOSE_ENV ?= ../../.env
ROOT_DIR := $(CURDIR)

.PHONY: start build-start stop compose-up compose-down compose-smoke compose-test compose-logs gen-certs compose-golden-local compose-golden-prodlike compose-golden-down

start:
	cd $(COMPOSE_DIR) && $(COMPOSE) --env-file "$(COMPOSE_ENV)" up -d

build-start:
	cd $(COMPOSE_DIR) && $(COMPOSE) --env-file "$(COMPOSE_ENV)" up --build

stop:
	cd $(COMPOSE_DIR) && $(COMPOSE) --env-file "$(COMPOSE_ENV)" down

compose-up:
	cd $(COMPOSE_DIR) && $(COMPOSE) --env-file "$(COMPOSE_ENV)" up --build

compose-down:
	cd $(COMPOSE_DIR) && $(COMPOSE) --env-file "$(COMPOSE_ENV)" down

compose-smoke:
	cd $(COMPOSE_DIR) && $(COMPOSE) --env-file "$(COMPOSE_ENV)" -f docker-compose.smoke.yml up --build --abort-on-container-exit

compose-test:
	cd $(COMPOSE_DIR) && $(COMPOSE) --env-file "$(COMPOSE_ENV)" -f docker-compose.yml -f docker-compose.test.yml up --build --abort-on-container-exit

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
