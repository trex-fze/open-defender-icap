COMPOSE_DIR := deploy/docker
COMPOSE ?= docker compose

.PHONY: compose-up compose-down compose-smoke compose-test compose-hybrid compose-logs gen-certs

compose-up:
	cd $(COMPOSE_DIR) && $(COMPOSE) up --build

compose-down:
	cd $(COMPOSE_DIR) && $(COMPOSE) down

compose-smoke:
	cd $(COMPOSE_DIR) && $(COMPOSE) -f docker-compose.smoke.yml up --build --abort-on-container-exit

compose-test:
	cd $(COMPOSE_DIR) && $(COMPOSE) -f docker-compose.yml -f docker-compose.test.yml up --build --abort-on-container-exit

compose-hybrid:
	cd $(COMPOSE_DIR) && $(COMPOSE) -f docker-compose.yml -f docker-compose.lmstudio.yml -f docker-compose.ollama.yml up --build

compose-logs:
	cd $(COMPOSE_DIR) && $(COMPOSE) logs -f $(SERVICE)

gen-certs:
	$(COMPOSE_DIR)/scripts/gen-certs.sh
