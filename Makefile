.PHONY: build-prod build-dev up up-prod down down-prod rewatch restart-dev

build-prod:
	docker-compose -f docker-compose.yml -f docker-compose.prod.yml build

build-dev:
	docker-compose -f docker-compose.yml -f docker-compose.override.yml build

up-prod:
	docker-compose -f docker-compose.yml -f docker-compose.prod.yml up -d

up:
	docker-compose -f docker-compose.yml -f docker-compose.override.yml up -d

down-prod:
	docker-compose -f docker-compose.yml -f docker-compose.prod.yml down

down:
	docker-compose -f docker-compose.yml -f docker-compose.override.yml down --remove-orphans

rewatch:
	docker-compose -f docker-compose.yml -f docker-compose.override.yml restart node

restart-dev:
ifdef container
	docker-compose -f docker-compose.yml -f docker-compose.override.yml restart $(container)
else
	docker-compose -f docker-compose.yml -f docker-compose.override.yml restart
endif
