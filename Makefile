.PHONY: build-prod build-dev up-prod up-dev down-prod down-dev restart-dev

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

restart-dev:
ifdef container
	docker-compose -f docker-compose.yml -f docker-compose.override.yml restart $(container)
else
	docker-compose -f docker-compose.yml -f docker-compose.override.yml restart
endif
