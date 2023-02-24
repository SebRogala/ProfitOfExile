.PHONY: build-prod build-dev up-prod up-dev down-prod down-dev restart-prod-container restart-dev-container

build-prod:
	docker-compose -f docker-compose.yml -f docker-compose.prod.yml build

build-dev:
	docker-compose -f docker-compose.yml -f docker-compose.override.yml build

up-prod:
	docker-compose -f docker-compose.yml -f docker-compose.prod.yml up -d

up-dev:
	docker-compose -f docker-compose.yml -f docker-compose.override.yml up -d

down-prod:
	docker-compose -f docker-compose.yml -f docker-compose.prod.yml down

down-dev:
	docker-compose -f docker-compose.yml -f docker-compose.override.yml down

restart-prod-container:
ifdef container
	docker-compose -f docker-compose.yml -f docker-compose.prod.yml restart $(container)
else
	@echo "required container=[container name]"
endif

restart-dev-container:
ifdef container
	docker-compose -f docker-compose.yml -f docker-compose.override.yml restart $(container)
else
	@echo "required container=[container name]"
endif
